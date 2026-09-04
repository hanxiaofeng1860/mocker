use std::sync::{Arc, Mutex};

use mocker::domain::{Endpoint, Field, FieldLoc, GlobalSettings, RequestLog, SceneKind};
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

#[test]
fn load_save_settings_round_trip() {
    let (_dir, _, svc) = new_service(Noop);
    let settings = GlobalSettings {
        selected_source_id: "manual".into(),
        selected_model: "gpt".into(),
        manual_base_url: "http://127.0.0.1:1".into(),
        manual_api_key: "sk-test".into(),
        manual_protocol: "openai-chat".into(),
        manual_model: "gpt".into(),
    };
    svc.save_settings(&settings).unwrap();
    let loaded = svc.load_settings().unwrap();
    assert_eq!(loaded.selected_source_id, "manual");
    assert_eq!(loaded.manual_api_key, "sk-test");
    assert_eq!(loaded.manual_protocol, "openai-chat");
}

#[test]
fn set_model_replaces_client_for_import() {
    let (_dir, _, svc) = new_service(Noop);
    let project = svc
        .create_project("phone", 19999, "0000", "9999", Vec::new())
        .unwrap();
    assert!(svc.import_paste(&project.id, "paste").is_err());
    svc.set_model(Fake {
        json: IMPORT_JSON.into(),
    })
    .unwrap();
    let drafts = svc.import_paste(&project.id, "paste").unwrap();
    assert_eq!(drafts.len(), 1);
}

#[test]
fn create_endpoint_writes_four_scenes_and_unique_paths() {
    let (_dir, store, svc) = new_service(Noop);
    let project = svc
        .create_project("phone", 19001, "0000", "9999", Vec::new())
        .unwrap();
    let a = svc.create_endpoint(&project.id).unwrap();
    let b = svc.create_endpoint(&project.id).unwrap();
    assert_eq!(a.method, "POST");
    assert_eq!(a.path, "/untitled");
    assert_eq!(b.path, "/untitled-2");
    assert_eq!(a.name, "新接口");
    for kind in SceneKind::all() {
        let scene = store
            .lock()
            .unwrap()
            .get_scene(&a.id, kind)
            .unwrap()
            .expect("scene");
        assert_eq!(scene.endpoint_id, a.id);
        let body: Value = serde_json::from_str(&scene.body_json).unwrap();
        match kind {
            SceneKind::Success => assert_eq!(body["msg"], "成功"),
            SceneKind::Empty => assert_eq!(body["code"], "0000"),
            SceneKind::ParamError => {
                assert_eq!(scene.http_status, 400);
                assert_eq!(body["msg"], "参数错误");
            }
            SceneKind::BusinessError => assert_eq!(body["msg"], "失败"),
        }
    }
}

#[test]
fn delete_endpoint_removes_it_and_scenes() {
    let (_dir, store, svc) = new_service(Noop);
    let project = svc
        .create_project("phone", 19011, "0000", "9999", Vec::new())
        .unwrap();
    let a = svc.create_endpoint(&project.id).unwrap();
    let b = svc.create_endpoint(&project.id).unwrap();
    svc.delete_endpoint(&a.id).unwrap();
    assert!(store.lock().unwrap().get_endpoint(&a.id).unwrap().is_none());
    assert!(store
        .lock()
        .unwrap()
        .get_scene(&a.id, SceneKind::Success)
        .unwrap()
        .is_none());
    assert!(store.lock().unwrap().get_endpoint(&b.id).unwrap().is_some());
}

#[test]
fn regenerate_scene_from_fields_builds_success_data() {
    let (_dir, _, svc) = new_service(Noop);
    let project = svc
        .create_project("phone", 19002, "0000", "9999", Vec::new())
        .unwrap();
    let ep = svc.create_endpoint(&project.id).unwrap();
    svc.save_fields(
        &ep.id,
        vec![Field {
            id: store::new_id(),
            endpoint_id: ep.id.clone(),
            location: FieldLoc::Response,
            name: "todayInbound".into(),
            name_zh: "今日呼入".into(),
            type_name: "Integer".into(),
            required: true,
            comment: String::new(),
            enum_values: Vec::new(),
            parent_id: None,
        }],
    )
    .unwrap();
    svc.regenerate_scene_from_fields(&ep.id, SceneKind::Success)
        .unwrap();
    let scene = svc
        .get_scene(&ep.id, SceneKind::Success)
        .unwrap()
        .expect("scene");
    let body: Value = serde_json::from_str(&scene.body_json).unwrap();
    assert_eq!(body["code"], "0000");
    assert_eq!(body["data"]["todayInbound"], json!(0));

    svc.regenerate_scene_from_fields(&ep.id, SceneKind::ParamError)
        .unwrap();
    let err = svc
        .get_scene(&ep.id, SceneKind::ParamError)
        .unwrap()
        .expect("scene");
    let err_body: Value = serde_json::from_str(&err.body_json).unwrap();
    assert_eq!(err_body["msg"], "参数错误");
}

#[test]
fn apply_generated_success_accepts_import_json_and_raw_object() {
    let (_dir, _, svc) = new_service(Noop);
    let project = svc
        .create_project("phone", 19003, "0000", "9999", Vec::new())
        .unwrap();
    let ep = svc.create_endpoint(&project.id).unwrap();
    svc.apply_generated_success(&ep.id, IMPORT_JSON).unwrap();
    let scene = svc
        .get_scene(&ep.id, SceneKind::Success)
        .unwrap()
        .expect("scene");
    let body: Value = serde_json::from_str(&scene.body_json).unwrap();
    assert_eq!(body["data"]["total"], json!(2));

    svc.apply_generated_success(&ep.id, r#"{"code":"0000","msg":"成功","data":{"ok":true}}"#)
        .unwrap();
    let scene = svc
        .get_scene(&ep.id, SceneKind::Success)
        .unwrap()
        .expect("scene");
    let body: Value = serde_json::from_str(&scene.body_json).unwrap();
    assert_eq!(body["data"]["ok"], json!(true));
}

#[test]
fn clear_logs_via_service() {
    let (_dir, store, svc) = new_service(Noop);
    let project = svc
        .create_project("phone", 19004, "0000", "9999", Vec::new())
        .unwrap();
    store
        .lock()
        .unwrap()
        .append_log(&RequestLog {
            id: 0,
            project_id: project.id.clone(),
            at: "t".into(),
            method: "POST".into(),
            url: "/x".into(),
            req_headers: "{}".into(),
            req_body: String::new(),
            hit: false,
            endpoint_id: None,
            scene: None,
            status: 404,
            res_body: String::new(),
            elapsed_ms: 0,
            missing_default_headers: Vec::new(),
        })
        .unwrap();
    assert_eq!(svc.logs(&project.id).unwrap().len(), 1);
    svc.clear_logs(&project.id).unwrap();
    assert!(svc.logs(&project.id).unwrap().is_empty());
}
