use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{anyhow, Result};

use crate::domain::{
    DataKind, Endpoint, Envelope, Field, FieldLoc, GlobalSettings, HeaderKv, Project, RequestLog,
    Scene, SceneKind,
};
use crate::import::{
    attach_scenes, import_prompt, strip_markdown_fences, validate_import, ImportDraft,
};
use crate::llm::ModelClient;
use crate::runtime::RuntimeHub;
use crate::scenes::{
    default_success_envelope, generate_non_success, relabel_envelope, scene_http_status,
};
use crate::store::{self, Store};
use serde_json::{json, Map, Value};

pub struct AppService {
    store: Arc<Mutex<Store>>,
    runtime: RuntimeHub,
    model: Arc<Mutex<Box<dyn ModelClient + Send + Sync>>>,
    // import_paste and commit_import are separate UI steps; keep the paste so source_text is the original.
    last_paste: Mutex<HashMap<String, String>>,
}

impl AppService {
    pub fn new(store: Arc<Mutex<Store>>, model: impl ModelClient + Send + Sync + 'static) -> Self {
        let runtime = RuntimeHub::new(store.clone());
        Self {
            store,
            runtime,
            model: Arc::new(Mutex::new(Box::new(model))),
            last_paste: Mutex::new(HashMap::new()),
        }
    }

    pub fn load_settings(&self) -> Result<GlobalSettings> {
        lock(&self.store)?.load_settings()
    }

    pub fn save_settings(&self, settings: &GlobalSettings) -> Result<()> {
        lock(&self.store)?.save_settings(settings)
    }

    pub fn set_model(&self, model: impl ModelClient + Send + Sync + 'static) -> Result<()> {
        *lock(&self.model)? = Box::new(model);
        Ok(())
    }

    pub fn list_projects(&self) -> Result<Vec<Project>> {
        lock(&self.store)?.list_projects()
    }

    pub fn list_endpoints(&self, project_id: &str) -> Result<Vec<Endpoint>> {
        lock(&self.store)?.list_endpoints(project_id)
    }

    pub fn get_project(&self, id: &str) -> Result<Option<Project>> {
        lock(&self.store)?.get_project(id)
    }

    pub fn get_endpoint(&self, id: &str) -> Result<Option<Endpoint>> {
        lock(&self.store)?.get_endpoint(id)
    }

    pub fn list_fields(&self, endpoint_id: &str) -> Result<Vec<Field>> {
        lock(&self.store)?.list_fields(endpoint_id)
    }

    pub fn get_scene(&self, endpoint_id: &str, kind: SceneKind) -> Result<Option<Scene>> {
        lock(&self.store)?.get_scene(endpoint_id, kind)
    }

    pub fn is_running(&self, project_id: &str) -> bool {
        self.runtime.is_running(project_id)
    }

    pub fn save_project(&self, project: Project) -> Result<()> {
        let id = project.id.clone();
        let old_envelope = lock(&self.store)?.get_project(&id)?.map(|p| p.envelope);
        lock(&self.store)?.upsert_project(&project)?;
        if let Some(old) = old_envelope {
            let old = old.sanitized();
            let new = project.envelope.sanitized();
            if old != new {
                self.relabel_project_scenes(&id, &old, &new)?;
            }
        }
        self.runtime.refresh(&id)
    }

    fn relabel_project_scenes(
        &self,
        project_id: &str,
        old: &Envelope,
        new: &Envelope,
    ) -> Result<()> {
        let store = lock(&self.store)?;
        for ep in store.list_endpoints(project_id)? {
            for kind in SceneKind::all() {
                let Some(mut scene) = store.get_scene(&ep.id, kind)? else {
                    continue;
                };
                let Ok(body) = serde_json::from_str::<Value>(&scene.body_json) else {
                    continue;
                };
                let next = relabel_envelope(&body, old, new);
                if next != body {
                    scene.body_json = pretty_json(&next);
                    store.upsert_scene(&scene)?;
                }
            }
            for (data_kind, raw) in store.list_data_kind_success(&ep.id)? {
                let Ok(body) = serde_json::from_str::<Value>(&raw) else {
                    continue;
                };
                let next = relabel_envelope(&body, old, new);
                if next != body {
                    store.upsert_data_kind_success(&ep.id, data_kind, &pretty_json(&next))?;
                }
            }
        }
        Ok(())
    }

    pub fn clear_logs(&self, project_id: &str) -> Result<()> {
        lock(&self.store)?.clear_logs(project_id)
    }

    pub fn create_project(
        &self,
        name: &str,
        port: u16,
        success_code: &str,
        fail_code: &str,
        headers: Vec<HeaderKv>,
    ) -> Result<Project> {
        let project = Project {
            id: store::new_id(),
            name: name.to_string(),
            port,
            success_code: success_code.to_string(),
            fail_code: fail_code.to_string(),
            default_headers: headers,
            envelope: Envelope::default(),
        };
        lock(&self.store)?.upsert_project(&project)?;
        Ok(project)
    }

    pub fn start(&self, project_id: &str) -> Result<()> {
        self.runtime.start(project_id)
    }

    pub fn stop(&self, project_id: &str) -> Result<()> {
        self.runtime.stop(project_id)
    }

    pub fn save_endpoint(&self, ep: Endpoint) -> Result<()> {
        let project_id = ep.project_id.clone();
        lock(&self.store)?
            .upsert_endpoint(&ep)
            .map_err(map_unique)?;
        self.runtime.refresh(&project_id)
    }

    pub fn switch_data_kind(&self, endpoint_id: &str, kind: DataKind) -> Result<()> {
        let (project_id, restored) = {
            let store = lock(&self.store)?;
            let mut endpoint = store
                .get_endpoint(endpoint_id)?
                .ok_or_else(|| anyhow!("endpoint not found: {endpoint_id}"))?;
            if endpoint.data_kind == kind {
                return Ok(());
            }
            if let Some(scene) = store.get_scene(endpoint_id, SceneKind::Success)? {
                store.upsert_data_kind_success(endpoint_id, endpoint.data_kind, &scene.body_json)?;
            }
            let cached = store.get_data_kind_success(endpoint_id, kind)?;
            endpoint.data_kind = kind;
            store.upsert_endpoint(&endpoint).map_err(map_unique)?;
            if let Some(body) = cached {
                store.upsert_scene(&Scene {
                    endpoint_id: endpoint_id.to_string(),
                    kind: SceneKind::Success,
                    http_status: scene_http_status(SceneKind::Success),
                    body_json: body,
                })?;
                (endpoint.project_id, true)
            } else {
                (endpoint.project_id, false)
            }
        };
        if !restored {
            self.regenerate_scene_from_fields(endpoint_id, SceneKind::Success)?;
        }
        self.regenerate_scene_from_fields(endpoint_id, SceneKind::Empty)?;
        self.runtime.refresh(&project_id)
    }

    pub fn delete_endpoint(&self, endpoint_id: &str) -> Result<()> {
        let project_id = {
            let store = lock(&self.store)?;
            let endpoint = store
                .get_endpoint(endpoint_id)?
                .ok_or_else(|| anyhow!("endpoint not found: {endpoint_id}"))?;
            let project_id = endpoint.project_id;
            store.delete_endpoint(endpoint_id)?;
            project_id
        };
        self.runtime.refresh(&project_id)
    }

    pub fn create_endpoint(&self, project_id: &str) -> Result<Endpoint> {
        let (project, endpoints) = {
            let store = lock(&self.store)?;
            let project = store
                .get_project(project_id)?
                .ok_or_else(|| anyhow!("project not found: {project_id}"))?;
            let endpoints = store.list_endpoints(project_id)?;
            (project, endpoints)
        };
        let endpoint = Endpoint {
            id: store::new_id(),
            project_id: project_id.to_string(),
            method: "POST".into(),
            path: unused_path(&endpoints),
            name: "新接口".into(),
            notes: String::new(),
            source_text: String::new(),
            deprecated: false,
            enabled: true,
            current_scene: SceneKind::Success,
            data_kind: DataKind::Object,
        };
        let success = default_success_envelope(&project.success_code, &project.envelope);
        {
            let store = lock(&self.store)?;
            store.upsert_endpoint(&endpoint).map_err(map_unique)?;
            for kind in SceneKind::all() {
                store.upsert_scene(&Scene {
                    endpoint_id: endpoint.id.clone(),
                    kind,
                    http_status: scene_http_status(kind),
                    body_json: pretty_json(&generate_non_success(
                        kind,
                        &project.success_code,
                        &project.fail_code,
                        &success,
                        &project.envelope,
                    )),
                })?;
            }
        }
        self.runtime.refresh(project_id)?;
        Ok(endpoint)
    }

    pub fn save_scene_body(&self, endpoint_id: &str, kind: SceneKind, body: String) -> Result<()> {
        let project_id = {
            let store = lock(&self.store)?;
            let endpoint = store
                .get_endpoint(endpoint_id)?
                .ok_or_else(|| anyhow!("endpoint not found: {endpoint_id}"))?;
            if kind == SceneKind::Success {
                store.upsert_data_kind_success(endpoint_id, endpoint.data_kind, &body)?;
            }
            store.upsert_scene(&Scene {
                endpoint_id: endpoint_id.to_string(),
                kind,
                http_status: scene_http_status(kind),
                body_json: body,
            })?;
            endpoint.project_id
        };
        self.runtime.refresh(&project_id)
    }

    pub fn set_scene(&self, endpoint_id: &str, kind: SceneKind) -> Result<()> {
        let project_id = {
            let store = lock(&self.store)?;
            let endpoint = store
                .get_endpoint(endpoint_id)?
                .ok_or_else(|| anyhow!("endpoint not found: {endpoint_id}"))?;
            store.set_current_scene(endpoint_id, kind)?;
            endpoint.project_id
        };
        self.runtime.refresh(&project_id)
    }

    pub fn save_fields(&self, endpoint_id: &str, fields: Vec<Field>) -> Result<()> {
        // Route snapshot reads scenes, not the field tree.
        lock(&self.store)?.replace_fields(endpoint_id, &fields)
    }

    pub fn import_paste(&self, project_id: &str, paste: &str) -> Result<Vec<ImportDraft>> {
        let drafts = parse_import(&self.store, &self.model, project_id, paste)?;
        self.remember_import_paste(project_id, paste)?;
        Ok(drafts)
    }

    pub(crate) fn remember_import_paste(&self, project_id: &str, paste: &str) -> Result<()> {
        lock(&self.last_paste)?.insert(project_id.to_string(), paste.to_string());
        Ok(())
    }

    /// Cloneable job so the UI can run the LLM parse off the UI thread.
    pub(crate) fn import_paste_job(
        &self,
        project_id: String,
        paste: String,
    ) -> impl FnOnce() -> Result<Vec<ImportDraft>> + Send {
        let store = self.store.clone();
        let model = self.model.clone();
        move || parse_import(&store, &model, &project_id, &paste)
    }

    pub fn commit_import(&self, project_id: &str, drafts: Vec<ImportDraft>) -> Result<()> {
        let paste = lock(&self.last_paste)?
            .get(project_id)
            .cloned()
            .unwrap_or_default();
        {
            let store = lock(&self.store)?;
            let project = store
                .get_project(project_id)?
                .ok_or_else(|| anyhow!("project not found: {project_id}"))?;
            for draft in drafts {
                let endpoint_id = store::new_id();
                let scenes = attach_scenes(
                    &draft,
                    &project.success_code,
                    &project.fail_code,
                    &project.envelope,
                );
                store.upsert_endpoint(&Endpoint {
                    id: endpoint_id.clone(),
                    project_id: project_id.to_string(),
                    method: draft.method,
                    path: draft.path,
                    name: draft.name,
                    notes: draft.notes,
                    source_text: paste.clone(),
                    deprecated: draft.deprecated,
                    enabled: !draft.deprecated,
                    current_scene: SceneKind::Success,
                    data_kind: infer_data_kind(&draft.success_body, &project.envelope),
                })?;
                let mut fields = draft.request_headers;
                fields.extend(draft.request_body_fields);
                fields.extend(draft.response_fields);
                for field in &mut fields {
                    field.endpoint_id = endpoint_id.clone();
                }
                store.replace_fields(&endpoint_id, &fields)?;
                for mut scene in scenes {
                    scene.endpoint_id = endpoint_id.clone();
                    store.upsert_scene(&scene)?;
                }
            }
        }
        self.runtime.refresh(project_id)
    }

    pub fn logs(&self, project_id: &str) -> Result<Vec<RequestLog>> {
        lock(&self.store)?.list_logs(project_id)
    }

    pub fn apply_generated_success(&self, endpoint_id: &str, raw: &str) -> Result<()> {
        let value = parse_generated_success(raw)?;
        self.save_scene_body(endpoint_id, SceneKind::Success, pretty_json(&value))
    }

    pub fn regenerate_scene_from_fields(&self, endpoint_id: &str, kind: SceneKind) -> Result<()> {
        let (project, fields, success_body, data_kind) = {
            let store = lock(&self.store)?;
            let endpoint = store
                .get_endpoint(endpoint_id)?
                .ok_or_else(|| anyhow!("endpoint not found: {endpoint_id}"))?;
            let project = store
                .get_project(&endpoint.project_id)?
                .ok_or_else(|| anyhow!("project not found: {}", endpoint.project_id))?;
            let fields = store.list_fields(endpoint_id)?;
            let data_kind = endpoint.data_kind;
            let success_body = store
                .get_scene(endpoint_id, SceneKind::Success)?
                .and_then(|s| serde_json::from_str(&s.body_json).ok())
                .unwrap_or_else(|| {
                    default_success_envelope(&project.success_code, &project.envelope)
                });
            (project, fields, success_body, data_kind)
        };
        let body = match kind {
            SceneKind::Success => {
                success_from_fields(&project.success_code, &fields, &project.envelope, data_kind)
            }
            other => generate_non_success(
                other,
                &project.success_code,
                &project.fail_code,
                &success_body,
                &project.envelope,
            ),
        };
        self.save_scene_body(endpoint_id, kind, pretty_json(&body))
    }
}

fn parse_import(
    store: &Mutex<Store>,
    model: &Mutex<Box<dyn ModelClient + Send + Sync>>,
    project_id: &str,
    paste: &str,
) -> Result<Vec<ImportDraft>> {
    let (success_code, fail_code, envelope) = {
        let store = lock(store)?;
        let project = store
            .get_project(project_id)?
            .ok_or_else(|| anyhow!("project not found: {project_id}"))?;
        (project.success_code, project.fail_code, project.envelope)
    };
    let prompt = import_prompt(paste, &success_code, &fail_code, &envelope);
    let raw = lock(model)?.complete_json(&prompt)?;
    Ok(validate_import(&raw)?)
}

fn map_unique(err: anyhow::Error) -> anyhow::Error {
    let s = err.to_string();
    if s.contains("UNIQUE constraint failed") {
        anyhow!("同一项目下 method + path 不能重复")
    } else {
        err
    }
}

fn unused_path(existing: &[Endpoint]) -> String {
    for i in 1.. {
        let path = if i == 1 {
            "/untitled".to_string()
        } else {
            format!("/untitled-{i}")
        };
        if !existing
            .iter()
            .any(|e| e.method.eq_ignore_ascii_case("POST") && e.path == path)
        {
            return path;
        }
    }
    "/untitled".into()
}

fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

pub(crate) fn parse_generated_success(raw: &str) -> Result<Value> {
    if let Ok(drafts) = validate_import(raw) {
        if let Some(draft) = drafts.into_iter().next() {
            return Ok(draft.success_body);
        }
    }
    let stripped = strip_markdown_fences(raw);
    serde_json::from_str(stripped).map_err(|e| anyhow!("模型输出无法解析为 JSON: {e}"))
}

pub(crate) const ARRAY_SAMPLE_COUNT: usize = 3;

pub(crate) fn infer_data_kind(success_body: &Value, envelope: &Envelope) -> DataKind {
    match success_body.get(&envelope.sanitized().data_key) {
        Some(Value::Array(_)) => DataKind::Array,
        _ => DataKind::Object,
    }
}

pub(crate) fn success_from_fields(
    success_code: &str,
    fields: &[Field],
    envelope: &Envelope,
    data_kind: DataKind,
) -> Value {
    let data = match data_kind {
        DataKind::Object => Value::Object(payload_from_fields(fields, envelope, None)),
        DataKind::Array => {
            let items = (1..=ARRAY_SAMPLE_COUNT)
                .map(|i| Value::Object(payload_from_fields(fields, envelope, Some(i as u32))))
                .collect();
            Value::Array(items)
        }
    };
    envelope.pack(crate::scenes::encode_code(success_code), "成功", data)
}

fn payload_from_fields(
    fields: &[Field],
    envelope: &Envelope,
    index: Option<u32>,
) -> Map<String, Value> {
    let env = envelope.sanitized();
    if let Some(parent) = data_wrapper_id(fields, envelope) {
        return object_from_fields(fields, Some(parent), index);
    }
    let mut map = object_from_fields(fields, None, index);
    map.remove(&env.code_key);
    map.remove(&env.msg_key);
    map.remove(&env.data_key);
    map
}

fn data_wrapper_id<'a>(fields: &'a [Field], envelope: &Envelope) -> Option<&'a str> {
    let data_key = envelope.sanitized().data_key;
    fields
        .iter()
        .find(|field| {
            field.location == FieldLoc::Response
                && field.parent_id.is_none()
                && field.name.trim() == data_key
                && fields.iter().any(|child| {
                    child.location == FieldLoc::Response
                        && child.parent_id.as_deref() == Some(field.id.as_str())
                })
        })
        .map(|field| field.id.as_str())
}

fn object_from_fields(
    fields: &[Field],
    parent_id: Option<&str>,
    index: Option<u32>,
) -> Map<String, Value> {
    let mut data = Map::new();
    for field in fields {
        if field.location != FieldLoc::Response {
            continue;
        }
        let matches_parent = match (field.parent_id.as_deref(), parent_id) {
            (None, None) => true,
            (Some(a), Some(b)) => a == b,
            _ => false,
        };
        if !matches_parent {
            continue;
        }
        let name = field.name.trim();
        if name.is_empty() {
            continue;
        }
        let ty = field.type_name.trim().to_ascii_lowercase();
        let value = if matches!(ty.as_str(), "object" | "map") {
            Value::Object(object_from_fields(fields, Some(&field.id), index))
        } else {
            sample_value(&field.type_name, name, index)
        };
        data.insert(name.to_string(), value);
    }
    data
}

fn sample_value(type_name: &str, name: &str, index: Option<u32>) -> Value {
    match type_name.trim().to_ascii_lowercase().as_str() {
        "integer" | "int" | "long" | "number" => json!(index.unwrap_or(0)),
        "boolean" | "bool" => json!(false),
        "array" | "list" => json!([]),
        "object" | "map" => json!({}),
        "null" => Value::Null,
        _ => match index {
            Some(i) => json!(format!("{name}{i}")),
            None => json!(""),
        },
    }
}

fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>> {
    mutex.lock().map_err(|_| anyhow!("mutex poisoned"))
}
