use mocker::domain::SceneKind;
use mocker::scenes::{encode_code, generate_non_success};
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
