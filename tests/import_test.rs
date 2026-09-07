use mocker::domain::{Envelope, Field, FieldLoc, SceneKind};
use mocker::import::{
    attach_scenes, run_import, semantic_values_prompt, validate_import, ImportError,
};
use mocker::llm::{LlmError, ModelClient};
use serde_json::{json, Value};

const ONE_ENDPOINT: &str = r#"{
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
      "response_fields": [],
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
    fn complete_json(&self, prompt: &str) -> Result<String, LlmError> {
        assert!(
            prompt.contains("only return JSON object"),
            "prompt must require JSON-only output: {prompt}"
        );
        assert!(prompt.contains("\"endpoints\""), "{prompt}");
        assert!(prompt.contains("success_body"), "{prompt}");
        assert!(prompt.contains("PASTE-FIXTURE"), "{prompt}");
        Ok(self.json.clone())
    }
}

#[test]
fn valid_one_endpoint_yields_one_draft() {
    let drafts = validate_import(ONE_ENDPOINT).unwrap();
    assert_eq!(drafts.len(), 1);
    let draft = &drafts[0];
    assert_eq!(draft.name, "首页数据概览");
    assert_eq!(draft.path, "/hl/pub/phone/v1/queryPhoneHomeData");
    assert_eq!(draft.method, "POST");
    assert!(!draft.deprecated);
    assert_eq!(draft.notes, "首页");
    assert_eq!(draft.request_headers.len(), 1);
    assert_eq!(draft.request_headers[0].name, "sn");
    assert_eq!(draft.request_headers[0].location, FieldLoc::Header);
    assert!(draft.success_body.is_object());
    assert_eq!(draft.success_body["code"], json!("0000"));
}

#[test]
fn missing_path_is_error() {
    let err = validate_import(r#"{"endpoints":[{"name":"x","method":"POST","success_body":{}}]}"#)
        .unwrap_err();
    assert!(matches!(err, ImportError::MissingPath { index: 0 }));
}

#[test]
fn empty_endpoints_is_error() {
    let err = validate_import(r#"{"endpoints":[]}"#).unwrap_err();
    assert!(matches!(err, ImportError::EmptyEndpoints));
}

#[test]
fn non_json_is_error() {
    let err = validate_import("not json").unwrap_err();
    assert!(matches!(err, ImportError::NotJson(_)));
}

#[test]
fn attach_scenes_four_kinds_and_empty_list_rule() {
    let draft = validate_import(ONE_ENDPOINT).unwrap().pop().unwrap();
    let scenes = attach_scenes(&draft, "0000", "9999", &Envelope::default());
    assert_eq!(scenes[0].kind, SceneKind::Success);
    assert_eq!(scenes[1].kind, SceneKind::Empty);
    assert_eq!(scenes[2].kind, SceneKind::ParamError);
    assert_eq!(scenes[3].kind, SceneKind::BusinessError);
    assert_eq!(scenes[0].http_status, 200);
    assert_eq!(scenes[1].http_status, 200);
    assert_eq!(scenes[2].http_status, 400);
    assert_eq!(scenes[3].http_status, 200);

    let success: Value = serde_json::from_str(&scenes[0].body_json).unwrap();
    assert_eq!(success, draft.success_body);

    let empty: Value = serde_json::from_str(&scenes[1].body_json).unwrap();
    assert_eq!(empty["code"], json!("0000"));
    assert_eq!(empty["msg"], "成功");
    assert_eq!(empty["data"]["list"], json!([]));
    assert_eq!(empty["data"]["total"], 0);
    assert_eq!(empty["data"]["extra"], 1);

    let param: Value = serde_json::from_str(&scenes[2].body_json).unwrap();
    assert_eq!(param["code"], json!(9999));
    assert_eq!(param["msg"], "参数错误");
    assert_eq!(param["data"], json!(null));

    let biz: Value = serde_json::from_str(&scenes[3].body_json).unwrap();
    assert_eq!(biz["code"], json!(9999));
    assert_eq!(biz["msg"], "失败");
    assert_eq!(biz["data"], json!(null));
}

#[test]
fn run_import_uses_complete_json_then_validate() {
    let fake = Fake {
        json: format!("```json\n{ONE_ENDPOINT}\n```"),
    };
    let drafts = run_import(&fake, "PASTE-FIXTURE", "0000", "9999", &Envelope::default()).unwrap();
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].path, "/hl/pub/phone/v1/queryPhoneHomeData");
}

#[test]
fn semantic_values_prompt_asks_for_realistic_values_and_json_only() {
    let prompt = semantic_values_prompt(
        "0000",
        &Envelope::default(),
        "查询设备",
        "POST",
        "/hl/pub/phone/v1/queryPhoneBasicInfo",
        &[Field {
            id: "1".into(),
            endpoint_id: "e".into(),
            location: FieldLoc::Response,
            name: "city".into(),
            name_zh: "城市".into(),
            type_name: "String".into(),
            required: false,
            comment: String::new(),
            enum_values: Vec::new(),
            parent_id: None,
        }],
        r#"{"code":"0000","msg":"成功","data":{"city":""}}"#,
    );
    assert!(prompt.contains("only return JSON object"));
    assert!(prompt.contains("city"));
    assert!(prompt.contains("城市"));
    assert!(prompt.contains("国内城市名"));
    assert!(prompt.contains("0000"));
    assert!(!prompt.contains("endpoints"));
}
