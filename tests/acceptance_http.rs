use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use mocker::domain::{HeaderKv, SceneKind};
use mocker::llm::{LlmError, ModelClient};
use mocker::scenes::encode_code;
use mocker::service::AppService;
use mocker::store::Store;
use serde_json::{json, Value};
use tempfile::TempDir;

/// Fixture JSON a Fake ModelClient returns — no live LLM.
/// queryPhoneHomeData-style overview + a list endpoint (`data.list` + `total`).
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
          "name": "todayInbound",
          "name_zh": "今日呼入",
          "type": "Integer",
          "required": true,
          "comment": "",
          "enum_values": []
        }
      ],
      "envelope": true,
      "success_body": {
        "code": "0000",
        "msg": "成功",
        "data": {
          "todayInbound": 12,
          "yesterdayInbound": 9,
          "recent7DayInbound": 47
        }
      }
    },
    {
      "name": "通话记录列表",
      "path": "/hl/pub/phone/v1/queryCallRecordList",
      "method": "POST",
      "deprecated": false,
      "notes": "列表",
      "request_headers": [],
      "request_body_fields": [],
      "response_fields": [],
      "envelope": true,
      "success_body": {
        "code": "0000",
        "msg": "成功",
        "data": {
          "list": [
            {"id": "1", "callType": "in"},
            {"id": "2", "callType": "out"}
          ],
          "total": 2
        }
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

/// Bind `127.0.0.1:0`, read the ports, drop the listeners, then reuse.
fn ephemeral_ports(n: usize) -> Vec<u16> {
    let listeners: Vec<TcpListener> = (0..n)
        .map(|_| TcpListener::bind("127.0.0.1:0").unwrap())
        .collect();
    listeners
        .iter()
        .map(|l| l.local_addr().unwrap().port())
        .collect()
}

fn new_service() -> (TempDir, AppService) {
    let dir = TempDir::new().unwrap();
    let store = Arc::new(Mutex::new(
        Store::open(dir.path().join("mocker.db")).unwrap(),
    ));
    let svc = AppService::new(
        store,
        Fake {
            json: IMPORT_JSON.into(),
        },
    );
    (dir, svc)
}

fn url(port: u16, path: &str) -> String {
    format!("http://127.0.0.1:{port}{path}")
}

#[tokio::test]
async fn two_projects_import_scene_switch_404_and_missing_sn() {
    let (_dir, svc) = new_service();
    let ports = ephemeral_ports(2);
    let phone_port = ports[0];
    let other_port = ports[1];
    assert_ne!(phone_port, other_port);

    let phone = svc
        .create_project(
            "phone",
            phone_port,
            "0000",
            "9999",
            vec![HeaderKv {
                key: "sn".into(),
                value: "device".into(),
            }],
        )
        .unwrap();
    let other = svc
        .create_project("other", other_port, "0000", "9999", Vec::new())
        .unwrap();
    assert_eq!(svc.list_projects().unwrap().len(), 2);

    let drafts = svc
        .import_paste(
            &phone.id,
            "PASTE-FIXTURE queryPhoneHomeData queryCallRecordList",
        )
        .unwrap();
    assert_eq!(drafts.len(), 2);
    assert!(drafts
        .iter()
        .any(|d| d.path == "/hl/pub/phone/v1/queryPhoneHomeData"));
    assert!(drafts
        .iter()
        .any(|d| d.path == "/hl/pub/phone/v1/queryCallRecordList"));
    svc.commit_import(&phone.id, drafts).unwrap();

    let other_ep = svc.create_endpoint(&other.id).unwrap();
    assert_eq!(other_ep.path, "/untitled");

    svc.start(&phone.id).unwrap();
    svc.start(&other.id).unwrap();
    assert!(svc.is_running(&phone.id));
    assert!(svc.is_running(&other.id));

    let client = reqwest::Client::new();
    let home_path = "/hl/pub/phone/v1/queryPhoneHomeData";
    let list_path = "/hl/pub/phone/v1/queryCallRecordList";

    let home = client
        .post(url(phone_port, home_path))
        .send()
        .await
        .unwrap();
    assert_eq!(home.status(), 200);
    let home_body: Value = home.json().await.unwrap();
    assert_eq!(home_body["code"], json!("0000"));
    assert_eq!(home_body["msg"], json!("成功"));
    assert_eq!(home_body["data"]["todayInbound"], json!(12));
    assert_eq!(home_body["data"]["yesterdayInbound"], json!(9));
    assert_eq!(home_body["data"]["recent7DayInbound"], json!(47));

    let logs = svc.logs(&phone.id).unwrap();
    assert_eq!(logs.len(), 1);
    assert!(logs[0].hit);
    assert!(
        logs[0].missing_default_headers.iter().any(|k| k == "sn"),
        "{:?}",
        logs[0].missing_default_headers
    );

    let list = client
        .post(url(phone_port, list_path))
        .header("sn", "device")
        .send()
        .await
        .unwrap();
    assert_eq!(list.status(), 200);
    let list_body: Value = list.json().await.unwrap();
    assert_eq!(list_body["code"], json!("0000"));
    assert_eq!(list_body["msg"], json!("成功"));
    assert_eq!(
        list_body["data"]["list"],
        json!([{"id": "1", "callType": "in"}, {"id": "2", "callType": "out"}])
    );
    assert_eq!(list_body["data"]["total"], json!(2));

    let list_ep = svc
        .list_endpoints(&phone.id)
        .unwrap()
        .into_iter()
        .find(|e| e.path == list_path)
        .expect("list endpoint");
    svc.set_scene(&list_ep.id, SceneKind::Empty).unwrap();

    let empty = client
        .post(url(phone_port, list_path))
        .header("sn", "device")
        .send()
        .await
        .unwrap();
    assert_eq!(empty.status(), 200);
    let empty_body: Value = empty.json().await.unwrap();
    assert_eq!(empty_body["code"], json!("0000"));
    assert_eq!(empty_body["msg"], json!("成功"));
    assert_eq!(empty_body["data"]["list"], json!([]));
    assert_eq!(empty_body["data"]["total"], json!(0));

    let missing = client
        .post(url(phone_port, "/no/such"))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 404);
    let missing_body: Value = missing.json().await.unwrap();
    assert_eq!(missing_body["code"], encode_code("9999"));
    assert_eq!(missing_body["msg"], json!("未找到 mock 接口"));
    assert_eq!(missing_body["data"], Value::Null);

    let other_hit = client
        .post(url(other_port, "/untitled"))
        .send()
        .await
        .unwrap();
    assert_eq!(other_hit.status(), 200);
    let other_body: Value = other_hit.json().await.unwrap();
    assert_eq!(other_body["code"], json!("0000"));
    assert_eq!(other_body["msg"], json!("成功"));

    svc.stop(&phone.id).unwrap();
    svc.stop(&other.id).unwrap();
}
