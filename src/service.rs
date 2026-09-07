use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{anyhow, Result};

use crate::domain::{
    Endpoint, Envelope, Field, FieldLoc, GlobalSettings, HeaderKv, Project, RequestLog, Scene,
    SceneKind,
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
        let (project, fields, success_body) = {
            let store = lock(&self.store)?;
            let endpoint = store
                .get_endpoint(endpoint_id)?
                .ok_or_else(|| anyhow!("endpoint not found: {endpoint_id}"))?;
            let project = store
                .get_project(&endpoint.project_id)?
                .ok_or_else(|| anyhow!("project not found: {}", endpoint.project_id))?;
            let fields = store.list_fields(endpoint_id)?;
            let success_body = store
                .get_scene(endpoint_id, SceneKind::Success)?
                .and_then(|s| serde_json::from_str(&s.body_json).ok())
                .unwrap_or_else(|| {
                    default_success_envelope(&project.success_code, &project.envelope)
                });
            (project, fields, success_body)
        };
        let body = match kind {
            SceneKind::Success => {
                success_from_fields(&project.success_code, &fields, &project.envelope)
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

pub(crate) fn success_from_fields(
    success_code: &str,
    fields: &[Field],
    envelope: &Envelope,
) -> Value {
    let mut data = Map::new();
    for field in fields {
        if field.location != FieldLoc::Response || field.parent_id.is_some() {
            continue;
        }
        let name = field.name.trim();
        if name.is_empty() {
            continue;
        }
        data.insert(name.to_string(), sample_value(&field.type_name));
    }
    envelope.pack(
        crate::scenes::encode_code(success_code),
        "成功",
        Value::Object(data),
    )
}

fn sample_value(type_name: &str) -> Value {
    match type_name.trim().to_ascii_lowercase().as_str() {
        "integer" | "int" | "long" | "number" => json!(0),
        "boolean" | "bool" => json!(false),
        "array" | "list" => json!([]),
        "object" | "map" => json!({}),
        _ => json!(""),
    }
}

fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>> {
    mutex.lock().map_err(|_| anyhow!("mutex poisoned"))
}
