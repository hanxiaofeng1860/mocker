use std::sync::{Arc, Mutex};

use mocker::domain::{Endpoint, FieldLoc, SceneKind};
use mocker::llm::{LlmError, ModelClient};
use mocker::service::AppService;
use mocker::store::{self, Store};
use serde_json::{json, Value};
use tempfile::TempDir;

const SUCCESS_BODY: &str = r#"{"code":"0000","msg":"成功","data":{"ok":true}}"#;
const UPDATED_BODY: &str = r#"{"code":"0000","msg":"成功","data":{"ok":false,"n":1}}"#;

const IMPORT_JSON: &str = r#"{
  "endpoints": [
    {
      "name": "首页数据概览",
      "path": "/hl/pub/phone/v1/queryPhoneHomeData",
      "method": "POST",
      "deprecated": false,
      "notes": "首页",
      "request_headers": [
        {
          "name": "sn",
          "name_zh": "设备sn号",
          "type": "String",
          "required": true,
          "comment": "header",
          "enum_values": []
        }
      ],
      "request_body_fields": [],
      "response_fields": [
        {
          "name": "ok",
          "name_zh": "是否成功",
          "type": "Boolean",
          "required": true,
          "comment": "",
          "enum_values": []
        }
      ],
      "envelope": true,
      "success_body": {
        "code": "0000",
        "msg": "成功",
        "data": {"list": [{"id": "1"}], "total": 2, "extra": 1}
      }
    }
  ]
}"#;

struct Fake {
    json: String,
}

impl ModelClient for Fake {
    fn complete_json(&self, _prompt: &str) -> Result<String, LlmError> {
        Ok(self.json.clone())
    }
}

struct Noop;

impl ModelClient for Noop {
    fn complete_json(&self, _prompt: &str) -> Result<String, LlmError> {
        Err(LlmError::EmptyResponse)
    }
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn new_service(
    model: impl ModelClient + Send + Sync + 'static,
) -> (TempDir, Arc<Mutex<Store>>, AppService) {
    let dir = TempDir::new().unwrap();
    let store = Arc::new(Mutex::new(
        Store::open(dir.path().join("mocker.db")).unwrap(),
    ));
    let svc = AppService::new(store.clone(), model);
    (dir, store, svc)
}

#[tokio::test]
async fn create_start_then_save_scene_body_hot_reloads() {
    let (_dir, _store, svc) = new_service(Noop);
    let port = free_port();
    let project = svc
        .create_project("phone", port, "0000", "9999", Vec::new())
        .unwrap();
    assert_eq!(svc.list_projects().unwrap().len(), 1);

    let endpoint = Endpoint {
        id: store::new_id(),
        project_id: project.id.clone(),
        method: "POST".into(),
        path: "/api/queryPhoneHomeData".into(),
        name: "queryPhoneHomeData".into(),
        notes: String::new(),
        source_text: String::new(),
        deprecated: false,
        enabled: true,
        current_scene: SceneKind::Success,
    };
    svc.save_endpoint(endpoint.clone()).unwrap();
    svc.save_scene_body(&endpoint.id, SceneKind::Success, SUCCESS_BODY.into())
        .unwrap();
    assert_eq!(svc.list_endpoints(&project.id).unwrap().len(), 1);
    assert!(!svc.is_running(&project.id));

    svc.start(&project.id).unwrap();
    assert!(svc.is_running(&project.id));

    let url = format!("http://127.0.0.1:{port}/api/queryPhoneHomeData");
    let client = reqwest::Client::new();
    let first = client.post(&url).send().await.unwrap();
    assert_eq!(first.status(), 200);
    let first_body: Value = first.json().await.unwrap();
    assert_eq!(
        first_body,
        serde_json::from_str::<Value>(SUCCESS_BODY).unwrap()
    );

    svc.save_scene_body(&endpoint.id, SceneKind::Success, UPDATED_BODY.into())
        .unwrap();

    let second = client.post(&url).send().await.unwrap();
    assert_eq!(second.status(), 200);
    let second_body: Value = second.json().await.unwrap();
    assert_eq!(
        second_body,
        serde_json::from_str::<Value>(UPDATED_BODY).unwrap()
    );

    let logs = svc.logs(&project.id).unwrap();
    assert_eq!(logs.len(), 2);
    assert!(logs.iter().all(|l| l.hit));

    svc.stop(&project.id).unwrap();
    assert!(!svc.is_running(&project.id));
}

#[test]
fn import_paste_then_commit_writes_endpoints_fields_scenes_and_source_text() {
    let (_dir, store, svc) = new_service(Fake {
        json: IMPORT_JSON.into(),
    });
    let project = svc
        .create_project("phone", 18080, "0000", "9999", Vec::new())
        .unwrap();
    let paste = "PASTE-FIXTURE queryPhoneHomeData 首页";
    let drafts = svc.import_paste(&project.id, paste).unwrap();
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].path, "/hl/pub/phone/v1/queryPhoneHomeData");

    svc.commit_import(&project.id, drafts).unwrap();

    let endpoints = store.lock().unwrap().list_endpoints(&project.id).unwrap();
    assert_eq!(endpoints.len(), 1);
    let ep = &endpoints[0];
    assert_eq!(ep.method, "POST");
    assert_eq!(ep.path, "/hl/pub/phone/v1/queryPhoneHomeData");
    assert_eq!(ep.name, "首页数据概览");
    assert_eq!(ep.source_text, paste);
    assert!(ep.enabled);
    assert_eq!(ep.current_scene, SceneKind::Success);

    let fields = store.lock().unwrap().list_fields(&ep.id).unwrap();
    assert!(fields
        .iter()
        .any(|f| f.name == "sn" && f.location == FieldLoc::Header));
    assert!(fields
        .iter()
        .any(|f| f.name == "ok" && f.location == FieldLoc::Response));
    assert!(fields.iter().all(|f| f.endpoint_id == ep.id));

    let db = store.lock().unwrap();
    for kind in SceneKind::all() {
        let scene = db.get_scene(&ep.id, kind).unwrap().expect("scene");
        assert_eq!(scene.endpoint_id, ep.id);
        match kind {
            SceneKind::Success => {
                let body: Value = serde_json::from_str(&scene.body_json).unwrap();
                assert_eq!(body["data"]["total"], json!(2));
                assert_eq!(scene.http_status, 200);
            }
            SceneKind::Empty => {
                let body: Value = serde_json::from_str(&scene.body_json).unwrap();
                assert_eq!(body["data"]["list"], json!([]));
                assert_eq!(body["data"]["total"], 0);
                assert_eq!(scene.http_status, 200);
            }
            SceneKind::ParamError => assert_eq!(scene.http_status, 400),
            SceneKind::BusinessError => assert_eq!(scene.http_status, 200),
        }
    }
}
