use std::sync::{Arc, Mutex};

use mocker::domain::{Endpoint, HeaderKv, Project, Scene, SceneKind};
use mocker::runtime::RuntimeHub;
use mocker::scenes::encode_code;
use mocker::store::{self, Store};
use serde_json::{json, Value};
use tempfile::TempDir;

const SUCCESS_BODY: &str = r#"{"code":"0000","msg":"成功","data":{"ok":true}}"#;
const EMPTY_BODY: &str = r#"{"code":"0000","msg":"成功","data":{}}"#;

struct Harness {
    hub: RuntimeHub,
    store: Arc<Mutex<Store>>,
    project_id: String,
    endpoint_id: String,
    port: u16,
    _dir: TempDir,
}

impl Harness {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let store = Arc::new(Mutex::new(
            Store::open(dir.path().join("mocker.db")).unwrap(),
        ));
        let port = free_port();
        let project = Project {
            id: store::new_id(),
            name: "phone".into(),
            port,
            success_code: "0000".into(),
            fail_code: "9999".into(),
            default_headers: vec![HeaderKv {
                key: "sn".into(),
                value: "device".into(),
            }],
        };
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

        {
            let db = store.lock().unwrap();
            db.upsert_project(&project).unwrap();
            db.upsert_endpoint(&endpoint).unwrap();
            db.upsert_scene(&Scene {
                endpoint_id: endpoint.id.clone(),
                kind: SceneKind::Success,
                http_status: 200,
                body_json: SUCCESS_BODY.into(),
            })
            .unwrap();
            db.upsert_scene(&Scene {
                endpoint_id: endpoint.id.clone(),
                kind: SceneKind::Empty,
                http_status: 200,
                body_json: EMPTY_BODY.into(),
            })
            .unwrap();
        }

        let hub = RuntimeHub::new(store.clone());
        hub.start(&project.id).unwrap();
        assert!(hub.is_running(&project.id));

        Self {
            hub,
            store,
            project_id: project.id,
            endpoint_id: endpoint.id,
            port,
            _dir: dir,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

#[tokio::test]
async fn post_exact_path_returns_success_json() {
    let h = Harness::new();
    let resp = reqwest::Client::new()
        .post(h.url("/api/queryPhoneHomeData"))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body, serde_json::from_str::<Value>(SUCCESS_BODY).unwrap());
}

#[tokio::test]
async fn refresh_after_scene_change_updates_body_without_restart() {
    let h = Harness::new();
    let client = reqwest::Client::new();
    let first = client
        .post(h.url("/api/queryPhoneHomeData"))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), 200);
    let first_body: Value = first.json().await.unwrap();
    assert_eq!(
        first_body,
        serde_json::from_str::<Value>(SUCCESS_BODY).unwrap()
    );

    {
        let db = h.store.lock().unwrap();
        db.set_current_scene(&h.endpoint_id, SceneKind::Empty)
            .unwrap();
    }
    h.hub.refresh(&h.project_id).unwrap();
    assert!(h.hub.is_running(&h.project_id));

    let second = client
        .post(h.url("/api/queryPhoneHomeData"))
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), 200);
    let second_body: Value = second.json().await.unwrap();
    assert_eq!(
        second_body,
        serde_json::from_str::<Value>(EMPTY_BODY).unwrap()
    );
}

#[tokio::test]
async fn unknown_path_returns_404_envelope_with_fail_code() {
    let h = Harness::new();
    let resp = reqwest::Client::new()
        .post(h.url("/no/such"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["code"], encode_code("9999"));
    assert_eq!(body["msg"], "未找到 mock 接口");
    assert_eq!(body["data"], Value::Null);
}

#[tokio::test]
async fn get_on_post_path_returns_405() {
    let h = Harness::new();
    let resp = reqwest::Client::new()
        .get(h.url("/api/queryPhoneHomeData"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 405);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["code"], encode_code("9999"));
    assert_eq!(body["msg"], "方法不允许");
    assert_eq!(body["data"], Value::Null);
}

#[tokio::test]
async fn options_returns_204_with_cors() {
    let h = Harness::new();
    let resp = reqwest::Client::new()
        .request(reqwest::Method::OPTIONS, h.url("/api/queryPhoneHomeData"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);
    assert_eq!(
        resp.headers().get("access-control-allow-origin").unwrap(),
        "*"
    );
    assert_eq!(
        resp.headers().get("access-control-allow-headers").unwrap(),
        "*"
    );
    assert_eq!(
        resp.headers().get("access-control-allow-methods").unwrap(),
        "GET, POST, PUT, PATCH, DELETE, OPTIONS"
    );

    let logs = h.store.lock().unwrap().list_logs(&h.project_id).unwrap();
    assert!(logs.is_empty());
}

#[tokio::test]
async fn missing_default_header_still_200_and_is_logged() {
    let h = Harness::new();
    let resp = reqwest::Client::new()
        .post(h.url("/api/queryPhoneHomeData"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let logs = h.store.lock().unwrap().list_logs(&h.project_id).unwrap();
    assert_eq!(logs.len(), 1);
    assert!(
        logs[0].missing_default_headers.iter().any(|k| k == "sn"),
        "{:?}",
        logs[0].missing_default_headers
    );
}

#[tokio::test]
async fn start_fails_when_port_occupied() {
    let dir = TempDir::new().unwrap();
    let store = Arc::new(Mutex::new(
        Store::open(dir.path().join("mocker.db")).unwrap(),
    ));
    let occupier = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = occupier.local_addr().unwrap().port();
    let project = Project {
        id: store::new_id(),
        name: "busy".into(),
        port,
        success_code: "0000".into(),
        fail_code: "9999".into(),
        default_headers: Vec::new(),
    };
    store.lock().unwrap().upsert_project(&project).unwrap();

    let hub = RuntimeHub::new(store);
    let err = hub.start(&project.id).unwrap_err();
    assert!(
        err.to_string().contains(&port.to_string())
            || err
                .chain()
                .any(|e| e.to_string().contains("Address already in use")
                    || e.to_string().contains("address already in use")),
        "{err:#}"
    );
    assert!(!hub.is_running(&project.id));
    drop(occupier);
}
