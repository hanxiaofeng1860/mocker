use serde_json::{json, Value};

use crate::domain::SceneKind;

pub fn encode_code(code: &str) -> Value {
    if regex_simple_int(code) {
        match code.parse::<i64>() {
            Ok(n) => Value::Number(n.into()),
            // Spec regex allows digits past i64; keep as string rather than panic or f64.
            Err(_) => Value::String(code.to_string()),
        }
    } else {
        Value::String(code.to_string())
    }
}

/// JSON number iff `^-?(0|[1-9][0-9]*)$` (no leading zeros).
fn regex_simple_int(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() {
        return false;
    }
    let mut i = 0;
    if b[0] == b'-' {
        i = 1;
        if b.len() == 1 {
            return false;
        }
    }
    if b[i] == b'0' {
        return b.len() == i + 1;
    }
    b[i..].iter().all(|c| c.is_ascii_digit())
}

pub fn scene_http_status(kind: SceneKind) -> u16 {
    match kind {
        SceneKind::ParamError => 400,
        _ => 200,
    }
}

pub fn default_success_envelope(success_code: &str) -> Value {
    json!({ "code": encode_code(success_code), "msg": "成功", "data": {} })
}

pub fn generate_non_success(
    kind: SceneKind,
    success_code: &str,
    fail_code: &str,
    success_body: &Value,
) -> Value {
    match kind {
        SceneKind::Success => success_body.clone(),
        SceneKind::ParamError => json!({
            "code": encode_code(fail_code), "msg": "参数错误", "data": Value::Null
        }),
        SceneKind::BusinessError => json!({
            "code": encode_code(fail_code), "msg": "失败", "data": Value::Null
        }),
        SceneKind::Empty => empty_from_success(success_code, success_body),
    }
}

fn empty_from_success(success_code: &str, success_body: &Value) -> Value {
    let data = success_body.get("data").cloned().unwrap_or(json!({}));
    let data = match data {
        Value::Array(_) => json!([]),
        Value::Object(mut m) => {
            let has_list = m.contains_key("list");
            let has_total = m.contains_key("total");
            if !has_list && !has_total {
                json!({})
            } else {
                if has_list {
                    m.insert("list".into(), json!([]));
                }
                if has_total {
                    m.insert("total".into(), json!(0));
                }
                Value::Object(m)
            }
        }
        _ => json!({}),
    };
    json!({ "code": encode_code(success_code), "msg": "成功", "data": data })
}
