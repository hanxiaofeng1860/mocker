use mocker::domain::SceneKind;
use mocker::scenes::{
    default_success_envelope, encode_code, generate_non_success, scene_http_status,
};
use serde_json::json;

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
    let empty = generate_non_success(SceneKind::Empty, "0000", "9999", &success);
    assert_eq!(empty["code"], "0000");
    assert_eq!(empty["data"]["list"], json!([]));
    assert_eq!(empty["data"]["total"], 0);
    assert_eq!(empty["data"]["extra"], 1);
}

#[test]
fn param_error_is_object_with_fail_code() {
    let body = generate_non_success(SceneKind::ParamError, "0000", "9999", &json!({}));
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
    let body = default_success_envelope("0000");
    assert_eq!(body["code"], json!("0000"));
    assert_eq!(body["msg"], "成功");
    assert_eq!(body["data"], json!({}));
}

#[test]
fn empty_scene_array_data_becomes_empty_array() {
    let success = json!({"code":"0000","msg":"成功","data":[{"id":"1"}]});
    let empty = generate_non_success(SceneKind::Empty, "0000", "9999", &success);
    assert_eq!(empty["code"], "0000");
    assert_eq!(empty["msg"], "成功");
    assert_eq!(empty["data"], json!([]));
}

#[test]
fn empty_scene_non_list_object_data_becomes_empty_object() {
    let success = json!({"code":"0000","msg":"成功","data":{"user":{"id":1}}});
    let empty = generate_non_success(SceneKind::Empty, "0000", "9999", &success);
    assert_eq!(empty["code"], "0000");
    assert_eq!(empty["msg"], "成功");
    assert_eq!(empty["data"], json!({}));
}
