use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{anyhow, Result};

use crate::domain::{
    Endpoint, Field, GlobalSettings, HeaderKv, Project, RequestLog, Scene, SceneKind,
};
use crate::import::{attach_scenes, import_prompt, validate_import, ImportDraft};
use crate::llm::ModelClient;
use crate::runtime::RuntimeHub;
use crate::scenes::scene_http_status;
use crate::store::{self, Store};

pub struct AppService {
    store: Arc<Mutex<Store>>,
    runtime: RuntimeHub,
    model: Mutex<Box<dyn ModelClient + Send + Sync>>,
    // import_paste and commit_import are separate UI steps; keep the paste so source_text is the original.
    last_paste: Mutex<HashMap<String, String>>,
}

impl AppService {
    pub fn new(store: Arc<Mutex<Store>>, model: impl ModelClient + Send + Sync + 'static) -> Self {
        let runtime = RuntimeHub::new(store.clone());
        Self {
            store,
            runtime,
            model: Mutex::new(Box::new(model)),
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

    pub fn is_running(&self, project_id: &str) -> bool {
        self.runtime.is_running(project_id)
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
        lock(&self.store)?.upsert_endpoint(&ep)?;
        self.runtime.refresh(&project_id)
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
        let (success_code, fail_code) = {
            let store = lock(&self.store)?;
            let project = store
                .get_project(project_id)?
                .ok_or_else(|| anyhow!("project not found: {project_id}"))?;
            (project.success_code, project.fail_code)
        };
        let prompt = import_prompt(paste, &success_code, &fail_code);
        let raw = lock(&self.model)?.complete_json(&prompt)?;
        let drafts = validate_import(&raw)?;
        lock(&self.last_paste)?.insert(project_id.to_string(), paste.to_string());
        Ok(drafts)
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
                let scenes = attach_scenes(&draft, &project.success_code, &project.fail_code);
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
}

fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>> {
    mutex.lock().map_err(|_| anyhow!("mutex poisoned"))
}
