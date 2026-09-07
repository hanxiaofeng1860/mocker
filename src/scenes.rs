use serde_json::{json, Value};

use crate::domain::{Envelope, SceneKind};

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

pub fn default_success_envelope(success_code: &str, envelope: &Envelope) -> Value {
    envelope.pack(encode_code(success_code), "成功", json!({}))
}

pub fn generate_non_success(
    kind: SceneKind,
    success_code: &str,
    fail_code: &str,
    success_body: &Value,
    envelope: &Envelope,
) -> Value {
    match kind {
        SceneKind::Success => success_body.clone(),
        SceneKind::ParamError => envelope.pack(encode_code(fail_code), "参数错误", Value::Null),
        SceneKind::BusinessError => envelope.pack(encode_code(fail_code), "失败", Value::Null),
        SceneKind::Empty => empty_from_success(success_code, success_body, envelope),
    }
}

/// 只改顶层信封键名，保留原来的值。非对象 JSON 原样返回。
pub fn relabel_envelope(body: &Value, old: &Envelope, new: &Envelope) -> Value {
    let old = old.sanitized();
    let new = new.sanitized();
    if old == new {
        return body.clone();
    }
    let Value::Object(mut map) = body.clone() else {
        return body.clone();
    };
    let code = map.remove(&old.code_key);
    let msg = map.remove(&old.msg_key);
    let data = map.remove(&old.data_key);
    if let Some(v) = code {
        map.insert(new.code_key, v);
    }
    if let Some(v) = msg {
        map.insert(new.msg_key, v);
    }
    if let Some(v) = data {
        map.insert(new.data_key, v);
    }
    Value::Object(map)
}

fn empty_from_success(success_code: &str, success_body: &Value, envelope: &Envelope) -> Value {
    let data_key = envelope.sanitized().data_key;
    let data = success_body.get(&data_key).cloned().unwrap_or(json!({}));
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
    envelope.pack(encode_code(success_code), "成功", data)
}
