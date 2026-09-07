use mocker::domain::{Envelope, SceneKind};
use mocker::scenes::{
    default_success_envelope, encode_code, generate_non_success, relabel_envelope,
    scene_http_status,
};
use serde_json::json;

fn default_env() -> Envelope {
    Envelope::default()
}

#[test]
fn encode_leading_zeros_as_string() {
    assert_eq!(encode_code("0000"), json!("0000"));
    assert_eq!(encode_code("200"), json!(200));
    assert_eq!(encode_code("0"), json!(0));
}

#[test]
fn encode_negative_int_as_number() {
    assert_eq!(encode_code("-3"), json!(-3));
}

#[test]
fn empty_list_scene_uses_success_code() {
    let success =
        json!({"code":"0000","msg":"成功","data":{"list":[{"id":"1"}],"total":2,"extra":1}});
    let empty = generate_non_success(SceneKind::Empty, "0000", "9999", &success, &default_env());
    assert_eq!(empty["code"], "0000");
    assert_eq!(empty["data"]["list"], json!([]));
    assert_eq!(empty["data"]["total"], 0);
    assert_eq!(empty["data"]["extra"], 1);
}

#[test]
fn param_error_is_object_with_fail_code() {
    let body = generate_non_success(
        SceneKind::ParamError,
        "0000",
        "9999",
        &json!({}),
        &default_env(),
    );
    assert_eq!(body["code"], json!(9999));
    assert_eq!(body["msg"], "参数错误");
    assert_eq!(body["data"], json!(null));
}

#[test]
fn encode_overflow_int_falls_back_to_string() {
    assert_eq!(
        encode_code("9223372036854775808"),
        json!("9223372036854775808")
    );
}

#[test]
fn scene_http_status_param_error_is_400_others_200() {
    assert_eq!(scene_http_status(SceneKind::ParamError), 400);
    assert_eq!(scene_http_status(SceneKind::Success), 200);
    assert_eq!(scene_http_status(SceneKind::Empty), 200);
    assert_eq!(scene_http_status(SceneKind::BusinessError), 200);
}

#[test]
fn default_success_envelope_uses_encoded_code() {
    let body = default_success_envelope("0000", &default_env());
    assert_eq!(body["code"], json!("0000"));
    assert_eq!(body["msg"], "成功");
    assert_eq!(body["data"], json!({}));
}

#[test]
fn empty_scene_array_data_becomes_empty_array() {
    let success = json!({"code":"0000","msg":"成功","data":[{"id":"1"}]});
    let empty = generate_non_success(SceneKind::Empty, "0000", "9999", &success, &default_env());
    assert_eq!(empty["code"], "0000");
    assert_eq!(empty["msg"], "成功");
    assert_eq!(empty["data"], json!([]));
}

#[test]
fn empty_scene_non_list_object_data_becomes_empty_object() {
    let success = json!({"code":"0000","msg":"成功","data":{"user":{"id":1}}});
    let empty = generate_non_success(SceneKind::Empty, "0000", "9999", &success, &default_env());
    assert_eq!(empty["code"], "0000");
    assert_eq!(empty["msg"], "成功");
    assert_eq!(empty["data"], json!({}));
}

#[test]
fn custom_envelope_keys_used_for_empty_and_errors() {
    let env = Envelope {
        code_key: "errno".into(),
        msg_key: "message".into(),
        data_key: "result".into(),
    };
    let success = json!({"errno":"0000","message":"成功","result":{"list":[1],"total":1}});
    let empty = generate_non_success(SceneKind::Empty, "0000", "9999", &success, &env);
    assert_eq!(empty["errno"], "0000");
    assert_eq!(empty["message"], "成功");
    assert_eq!(empty["result"]["list"], json!([]));
    assert_eq!(empty["result"]["total"], 0);
    assert!(empty.get("code").is_none());
    assert!(empty.get("data").is_none());

    let param = generate_non_success(SceneKind::ParamError, "0000", "9999", &json!({}), &env);
    assert_eq!(param["errno"], json!(9999));
    assert_eq!(param["message"], "参数错误");
    assert_eq!(param["result"], json!(null));

    let body = default_success_envelope("0000", &env);
    assert_eq!(body["errno"], json!("0000"));
    assert_eq!(body["message"], "成功");
    assert_eq!(body["result"], json!({}));
}

#[test]
fn empty_envelope_keys_fall_back_to_defaults() {
    let env = Envelope {
        code_key: " ".into(),
        msg_key: "".into(),
        data_key: "  ".into(),
    };
    let body = default_success_envelope("0000", &env);
    assert_eq!(body["code"], json!("0000"));
    assert_eq!(body["msg"], "成功");
    assert_eq!(body["data"], json!({}));
}

#[test]
fn relabel_envelope_keeps_values_and_swaps_keys() {
    let old = Envelope::default();
    let new = Envelope {
        code_key: "errno".into(),
        msg_key: "errmsg".into(),
        data_key: "result".into(),
    };
    let body = json!({"code":"0000","msg":"成功","data":{"city":"杭州"}});
    let next = relabel_envelope(&body, &old, &new);
    assert_eq!(next["errno"], "0000");
    assert_eq!(next["errmsg"], "成功");
    assert_eq!(next["result"]["city"], "杭州");
    assert!(next.get("code").is_none());
    assert!(next.get("msg").is_none());
    assert!(next.get("data").is_none());

    let swapped = Envelope {
        code_key: "msg".into(),
        msg_key: "code".into(),
        data_key: "data".into(),
    };
    let swapped_body = relabel_envelope(&body, &old, &swapped);
    assert_eq!(swapped_body["msg"], "0000");
    assert_eq!(swapped_body["code"], "成功");
    assert_eq!(swapped_body["data"]["city"], "杭州");
}

#[test]
fn relabel_envelope_leaves_non_object_unchanged() {
    let body = json!([1, 2, 3]);
    let next = relabel_envelope(
        &body,
        &Envelope::default(),
        &Envelope {
            code_key: "errno".into(),
            msg_key: "errmsg".into(),
            data_key: "result".into(),
        },
    );
    assert_eq!(next, body);
}
