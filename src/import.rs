use serde_json::Value;

use crate::domain::{DataKind, Envelope, Field, FieldLoc, Scene, SceneKind};
use crate::llm::{LlmError, ModelClient};
use crate::scenes::{generate_non_success, scene_http_status};

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("模型输出不是 JSON: {0}")]
    NotJson(#[source] serde_json::Error),
    #[error("endpoints 必须是非空数组")]
    EmptyEndpoints,
    #[error("第 {index} 个接口缺少 path")]
    MissingPath { index: usize },
    #[error("第 {index} 个接口 path 必须以 / 开头")]
    BadPath { index: usize },
    #[error("第 {index} 个接口 method 必须是 GET/POST/PUT/PATCH/DELETE")]
    BadMethod { index: usize },
    #[error("第 {index} 个接口 success_body 必须是 JSON 对象")]
    BadSuccessBody { index: usize },
    #[error("{0}")]
    Llm(#[from] LlmError),
}

#[derive(Clone, Debug)]
pub struct ImportDraft {
    pub name: String,
    pub path: String,
    pub method: String,
    pub deprecated: bool,
    pub notes: String,
    pub request_headers: Vec<Field>,
    pub request_body_fields: Vec<Field>,
    pub response_fields: Vec<Field>,
    pub success_body: Value,
}

pub fn import_prompt(
    paste: &str,
    success_code: &str,
    fail_code: &str,
    envelope: &Envelope,
) -> String {
    let env = envelope.sanitized();
    let code_key = &env.code_key;
    let msg_key = &env.msg_key;
    let data_key = &env.data_key;
    format!(
        r#"你是接口文档解析器。根据用户粘贴的公司接口说明，提取一个或多个 HTTP 接口。

only return JSON object
只返回一个 JSON 对象，不要 markdown 围栏，不要解释性文字。

项目成功码: {success_code}
项目失败码: {fail_code}
响应信封字段: {code_key} / {msg_key} / {data_key}

JSON 形状：
{{
  "endpoints": [
    {{
      "name": "首页数据概览",
      "path": "/hl/pub/phone/v1/queryPhoneHomeData",
      "method": "POST",
      "deprecated": false,
      "notes": "接口逻辑原文，可空",
      "request_headers": [
        {{
          "name": "sn",
          "name_zh": "设备sn号",
          "type": "String",
          "required": true,
          "comment": "header",
          "enum_values": []
        }}
      ],
      "request_body_fields": [],
      "response_fields": [],
      "envelope": true,
      "success_body": {{}}
    }}
  ]
}}

约束：
- 按语义填成功数据：手机号像手机号、枚举用文档里的合法值、列表 2～3 条、时间用 ISO 或文档格式。
- 不编文档里没有的字段。
- 多接口一次切分。
- sn 等写在描述里的 header 必须进 request_headers。
- 不要返回解释性文字。
- method 只能是 GET、POST、PUT、PATCH、DELETE，缺省 POST。
- path 必须是完整路径且以 / 开头，不含 host、不含 query。
- 标题或描述含「废弃」时 deprecated 为 true。
- 若文档出参是 {code_key} / {msg_key} / {data_key} 信封：envelope 为 true，success_body 必须是完整信封，且 {code_key} 使用项目成功码 {success_code}。
- response_fields 与 body 字段使用同一字段对象，嵌套放在 children。

接口说明：
{paste}
"#
    )
}

pub fn semantic_values_prompt(
    success_code: &str,
    envelope: &Envelope,
    endpoint_name: &str,
    method: &str,
    path: &str,
    fields: &[Field],
    current_json: &str,
    data_kind: DataKind,
) -> String {
    let env = envelope.sanitized();
    let data_wrapper = fields.iter().find(|field| {
        field.location == FieldLoc::Response
            && field.parent_id.is_none()
            && field.name.trim() == env.data_key
            && fields.iter().any(|child| {
                child.location == FieldLoc::Response
                    && child.parent_id.as_deref() == Some(field.id.as_str())
            })
    });
    let mut field_lines = String::new();
    for field in fields.iter().filter(|f| f.location == FieldLoc::Response) {
        let name = field.name.trim();
        if name.is_empty() {
            continue;
        }
        if let Some(parent) = data_wrapper {
            if field.id == parent.id
                || (field.parent_id.is_none() && (name == env.code_key || name == env.msg_key))
            {
                continue;
            }
        } else if field.parent_id.is_none()
            && (name == env.code_key || name == env.msg_key || name == env.data_key)
        {
            continue;
        }
        let zh = field.name_zh.trim();
        let ty = if field.type_name.trim().is_empty() {
            "String"
        } else {
            field.type_name.trim()
        };
        let comment = field.comment.trim();
        field_lines.push_str(&format!("- {name}"));
        if !zh.is_empty() {
            field_lines.push_str(&format!("（{zh}）"));
        }
        field_lines.push_str(&format!(" 类型:{ty}"));
        if !comment.is_empty() {
            field_lines.push_str(&format!(" {comment}"));
        }
        field_lines.push('\n');
    }
    if field_lines.is_empty() {
        field_lines.push_str("（无字段表，请按当前 JSON 的键名推断语义）\n");
    }
    let skeleton = current_json.trim();
    let code_key = &env.code_key;
    let msg_key = &env.msg_key;
    let data_key = &env.data_key;
    let nest_rule =
        format!("- 顶层已有 {code_key}/{msg_key}/{data_key}，{data_key} 内不要再套一层信封。\n");
    let array_rule = if data_kind == DataKind::Array {
        format!(
            "- {data_key} 必须是业务对象数组。每个元素只含业务字段，不要再套一层 {code_key}/{msg_key}/{data_key}。\n"
        )
    } else {
        String::new()
    };
    format!(
        r#"你是 mock 数据生成器。根据接口字段的英文名、中文名和类型，给 JSON 填入符合真实业务语义的示例值。

only return JSON object
只返回一个 JSON 对象，不要 markdown 围栏，不要解释性文字。

接口：{method} {path} {endpoint_name}
项目成功码: {success_code}

字段：
{field_lines}
当前 JSON：
{skeleton}

约束：
- 保持字段名和 JSON 结构不变，只改 value。
- {code_key} 必须使用项目成功码 {success_code}，{msg_key} 用「成功」。
{nest_rule}{array_rule}- 按语义填值：city/城市→国内城市名（如杭州、成都）；mobile/phone/手机→1 开头 11 位；name/姓名→中文姓名；org/机构→中文机构名；email→合法邮箱；id/sn→非空字符串；金额→合理数字；时间→ISO 或常见日期；枚举用字段说明里的合法值；列表 2～3 条。
- 不要编造字段表或当前 JSON 里没有的键。
- 不要返回解释性文字。
"#
    )
}

pub fn run_import(
    client: &impl ModelClient,
    paste: &str,
    success_code: &str,
    fail_code: &str,
    envelope: &Envelope,
) -> Result<Vec<ImportDraft>, ImportError> {
    let prompt = import_prompt(paste, success_code, fail_code, envelope);
    let raw = client.complete_json(&prompt)?;
    validate_import(&raw)
}

pub fn attach_scenes(
    draft: &ImportDraft,
    success_code: &str,
    fail_code: &str,
    envelope: &Envelope,
) -> [Scene; 4] {
    SceneKind::all().map(|kind| Scene {
        // Filled when the draft is committed to an endpoint.
        endpoint_id: String::new(),
        kind,
        http_status: scene_http_status(kind),
        body_json: generate_non_success(
            kind,
            success_code,
            fail_code,
            &draft.success_body,
            envelope,
        )
        .to_string(),
    })
}

pub fn validate_import(raw: &str) -> Result<Vec<ImportDraft>, ImportError> {
    let stripped = strip_markdown_fences(raw);
    let value: Value = serde_json::from_str(stripped).map_err(ImportError::NotJson)?;
    let Some(endpoints) = value.get("endpoints").and_then(Value::as_array) else {
        return Err(ImportError::EmptyEndpoints);
    };
    if endpoints.is_empty() {
        return Err(ImportError::EmptyEndpoints);
    }
    let mut drafts = Vec::with_capacity(endpoints.len());
    for (index, item) in endpoints.iter().enumerate() {
        drafts.push(parse_draft(index, item)?);
    }
    Ok(drafts)
}

fn parse_draft(index: usize, item: &Value) -> Result<ImportDraft, ImportError> {
    let path = item
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if path.is_empty() {
        return Err(ImportError::MissingPath { index });
    }
    if !path.starts_with('/') {
        return Err(ImportError::BadPath { index });
    }
    let method_raw = item.get("method").and_then(Value::as_str).unwrap_or("POST");
    let method = normalize_method(method_raw).ok_or(ImportError::BadMethod { index })?;
    let success_body = item.get("success_body").cloned().unwrap_or(Value::Null);
    if !success_body.is_object() {
        return Err(ImportError::BadSuccessBody { index });
    }
    Ok(ImportDraft {
        name: item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        path: path.to_string(),
        method,
        deprecated: item
            .get("deprecated")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        notes: item
            .get("notes")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        request_headers: parse_fields(item.get("request_headers"), FieldLoc::Header),
        request_body_fields: parse_fields(item.get("request_body_fields"), FieldLoc::Body),
        response_fields: parse_fields(item.get("response_fields"), FieldLoc::Response),
        success_body,
    })
}

fn normalize_method(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let upper = if trimmed.is_empty() {
        "POST".to_string()
    } else {
        trimmed.to_ascii_uppercase()
    };
    match upper.as_str() {
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" => Some(upper),
        _ => None,
    }
}

fn parse_fields(value: Option<&Value>, loc: FieldLoc) -> Vec<Field> {
    let mut out = Vec::new();
    let Some(arr) = value.and_then(Value::as_array) else {
        return out;
    };
    for item in arr {
        push_field(item, loc, None, &mut out);
    }
    out
}

fn push_field(item: &Value, loc: FieldLoc, parent_id: Option<String>, out: &mut Vec<Field>) {
    if !item.is_object() {
        return;
    }
    let id = uuid::Uuid::new_v4().to_string();
    let enum_values = item
        .get("enum_values")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let type_name = item
        .get("type")
        .or_else(|| item.get("type_name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    out.push(Field {
        id: id.clone(),
        endpoint_id: String::new(),
        location: loc,
        name: item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        name_zh: item
            .get("name_zh")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        type_name,
        required: item
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        comment: item
            .get("comment")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        enum_values,
        parent_id,
    });
    if let Some(children) = item.get("children").and_then(Value::as_array) {
        for child in children {
            push_field(child, loc, Some(id.clone()), out);
        }
    }
}

pub(crate) fn strip_markdown_fences(raw: &str) -> &str {
    let mut s = raw.trim();
    if let Some(rest) = s.strip_prefix("```") {
        let rest = rest
            .strip_prefix("json")
            .or_else(|| rest.strip_prefix("JSON"))
            .unwrap_or(rest);
        let rest = rest
            .strip_prefix("\r\n")
            .or_else(|| rest.strip_prefix('\n'))
            .unwrap_or(rest);
        s = rest.trim_start();
    }
    if let Some(rest) = s.strip_suffix("```") {
        s = rest.trim_end();
    }
    s.trim()
}
