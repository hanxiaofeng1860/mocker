use std::fs;
use std::path::Path;
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

use crate::sources::{CredentialFrom, CredentialKind, ModelSource, Protocol};

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("读取凭证失败: {0}")]
    Credential(String),
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("请求失败: {0}")]
    Request(String),
    #[error("模型返回空内容")]
    EmptyResponse,
}

pub trait ModelClient {
    fn complete_json(&self, prompt: &str) -> Result<String, LlmError>;
}

enum Secret {
    File(CredentialFrom),
    Inline(String),
}

pub struct HttpModelClient {
    protocol: Protocol,
    base_url: String,
    model: String,
    secret: Secret,
}

impl HttpModelClient {
    pub fn from_source(source: &ModelSource) -> Self {
        Self {
            protocol: source.protocol,
            base_url: source.base_url.clone(),
            model: source.model.clone(),
            secret: Secret::File(source.credential_from.clone()),
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        let model = model.into();
        if !model.trim().is_empty() {
            self.model = model;
        }
        self
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn with_inline_key(
        protocol: Protocol,
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            protocol,
            base_url: base_url.into(),
            model: model.into(),
            secret: Secret::Inline(api_key.into()),
        }
    }

    fn api_key(&self) -> Result<String, LlmError> {
        match &self.secret {
            Secret::Inline(key) => nonempty_key(key),
            // Spec 5.3: scan secrets are read from the original file at call time.
            Secret::File(from) => read_credential(from),
        }
    }
}

impl ModelClient for HttpModelClient {
    fn complete_json(&self, prompt: &str) -> Result<String, LlmError> {
        let protocol = self.protocol;
        let url = request_url(&self.base_url, protocol);
        let model = normalize_model_name(&self.model).to_string();
        let key = self.api_key()?;
        let prompt = prompt.to_string();
        // Sync trait, async reqwest: a nested Runtime::block_on panics inside tokio.
        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| LlmError::Request(e.to_string()))?;
            rt.block_on(send_complete(protocol, &url, &model, &key, &prompt))
        })
        .join()
        .unwrap_or_else(|_| Err(LlmError::Request("llm thread panicked".into())))
    }
}

pub fn list_models(
    protocol: Protocol,
    base_url: &str,
    api_key: &str,
) -> Result<Vec<String>, LlmError> {
    let url = models_url(base_url, protocol);
    let key = api_key.trim().to_string();
    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| LlmError::Request(e.to_string()))?;
        rt.block_on(send_list_models(protocol, &url, &key))
    })
    .join()
    .unwrap_or_else(|_| Err(LlmError::Request("llm thread panicked".into())))
}

pub fn read_credential(from: &CredentialFrom) -> Result<String, LlmError> {
    let CredentialFrom::File { path, kind } = from;
    match kind {
        CredentialKind::ClaudeEnv => {
            let v = read_json(path)?;
            json_secret(v.get("env").and_then(|env| env.get("ANTHROPIC_AUTH_TOKEN")))
        }
        CredentialKind::PiProvider { name } => {
            let v = read_json(path)?;
            json_secret(
                v.get("providers")
                    .and_then(|p| p.get(name))
                    .and_then(|p| p.get("apiKey")),
            )
        }
        CredentialKind::GrokModel { id } => {
            let v = read_toml(path)?;
            toml_secret(
                v.get("model")
                    .and_then(|m| m.get(id.as_str()))
                    .and_then(|m| m.get("api_key")),
            )
        }
        CredentialKind::CodexProvider { id } => {
            let v = read_toml(path)?;
            let spec = v.get("model_providers").and_then(|m| m.get(id.as_str()));
            if let Ok(key) = toml_secret(spec.and_then(|s| s.get("api_key"))) {
                return Ok(key);
            }
            let env_key = spec
                .and_then(|s| s.get("env_key"))
                .and_then(|x| x.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("OPENAI_API_KEY");
            let val = std::env::var(env_key)
                .map_err(|_| LlmError::Credential(format!("环境变量 {env_key} 为空")))?;
            nonempty_key(&val)
        }
    }
}

/// Spec 5.3: strip a trailing `/`; if base already ends with `/v1` or `/v1/messages`, do not append another `/v1`.
fn request_url(base_url: &str, protocol: Protocol) -> String {
    let base = base_url.trim().trim_end_matches('/');
    match protocol {
        Protocol::AnthropicMessages => {
            if base.ends_with("/v1/messages") {
                base.to_string()
            } else if base.ends_with("/v1") {
                format!("{base}/messages")
            } else {
                format!("{base}/v1/messages")
            }
        }
        Protocol::OpenAIChat => {
            if base.ends_with("/chat/completions") {
                base.to_string()
            } else {
                format!("{base}/chat/completions")
            }
        }
    }
}

fn models_url(base_url: &str, protocol: Protocol) -> String {
    let base = base_url.trim().trim_end_matches('/');
    match protocol {
        Protocol::AnthropicMessages => {
            if let Some(prefix) = base.strip_suffix("/v1/messages") {
                format!("{prefix}/v1/models")
            } else if let Some(prefix) = base.strip_suffix("/messages") {
                format!("{prefix}/models")
            } else if base.ends_with("/v1") {
                format!("{base}/models")
            } else {
                format!("{base}/v1/models")
            }
        }
        Protocol::OpenAIChat => {
            if let Some(prefix) = base.strip_suffix("/chat/completions") {
                format!("{}/models", prefix.trim_end_matches('/'))
            } else {
                format!("{base}/models")
            }
        }
    }
}

async fn send_complete(
    protocol: Protocol,
    url: &str,
    model: &str,
    key: &str,
    prompt: &str,
) -> Result<String, LlmError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| LlmError::Request(e.to_string()))?;
    let builder = client.post(url);
    let (builder, body) = match protocol {
        Protocol::AnthropicMessages => (
            builder
                .header("x-api-key", key)
                .header("Authorization", format!("Bearer {key}"))
                .header("anthropic-version", "2023-06-01"),
            json!({
                "model": model,
                "max_tokens": 16384,
                "thinking": { "type": "disabled" },
                "messages": [{"role": "user", "content": prompt}]
            }),
        ),
        Protocol::OpenAIChat => (
            builder.bearer_auth(key),
            json!({
                "model": model,
                "messages": [{"role": "user", "content": prompt}]
            }),
        ),
    };
    let resp = builder
        .json(&body)
        .send()
        .await
        .map_err(|e| LlmError::Request(e.to_string()))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| LlmError::Request(e.to_string()))?;
    if !status.is_success() {
        return Err(LlmError::Http {
            status: status.as_u16(),
            body: truncate(&text, 2000),
        });
    }
    extract_text(protocol, &text)
}

async fn send_list_models(
    protocol: Protocol,
    url: &str,
    key: &str,
) -> Result<Vec<String>, LlmError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| LlmError::Request(e.to_string()))?;
    let mut builder = client.get(url);
    builder = match protocol {
        Protocol::AnthropicMessages => {
            let builder = builder.header("anthropic-version", "2023-06-01");
            if key.is_empty() {
                builder
            } else {
                builder.header("x-api-key", key)
            }
        }
        Protocol::OpenAIChat => {
            if key.is_empty() {
                builder
            } else {
                builder.bearer_auth(key)
            }
        }
    };
    let resp = builder
        .send()
        .await
        .map_err(|e| LlmError::Request(e.to_string()))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| LlmError::Request(e.to_string()))?;
    if !status.is_success() {
        return Err(LlmError::Http {
            status: status.as_u16(),
            body: truncate(&text, 2000),
        });
    }
    let v: Value = serde_json::from_str(&text)
        .map_err(|e| LlmError::Request(format!("响应不是 JSON: {e}")))?;
    Ok(parse_model_ids(&v))
}

fn parse_model_ids(v: &Value) -> Vec<String> {
    let fallback: Vec<Value> = Vec::new();
    let items = v
        .get("data")
        .or_else(|| v.get("models"))
        .and_then(Value::as_array)
        .or_else(|| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&fallback);
    let mut ids: Vec<String> = items
        .iter()
        .filter_map(|item| {
            item.get("id")
                .and_then(Value::as_str)
                .or_else(|| item.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

fn extract_text(_protocol: Protocol, text: &str) -> Result<String, LlmError> {
    let v: Value =
        serde_json::from_str(text).map_err(|e| LlmError::Request(format!("响应不是 JSON: {e}")))?;
    if let Some(msg) = api_error_message(&v) {
        return Err(LlmError::Request(msg));
    }
    match collect_text(&v) {
        Some(s) if !s.trim().is_empty() => Ok(s),
        _ if has_thinking_only(&v) => Err(LlmError::Request(
            "模型只返回了思考过程，没有 JSON 正文。请再点一次解析".into(),
        )),
        _ => Err(LlmError::Request(format!(
            "模型返回空内容。响应: {}",
            truncate(text, 400)
        ))),
    }
}

fn has_thinking_only(v: &Value) -> bool {
    let Some(blocks) = v.get("content").and_then(Value::as_array) else {
        return v
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .and_then(|c| c.get("message"))
            .is_some_and(|m| {
                m.get("reasoning_content")
                    .or_else(|| m.get("reasoning"))
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.trim().is_empty())
                    && collect_text(v).is_none()
            });
    };
    let mut thinking = false;
    let mut text = false;
    for block in blocks {
        let ty = block.get("type").and_then(Value::as_str).unwrap_or("");
        if ty == "thinking" || block.get("thinking").is_some() {
            thinking = true;
        }
        if block_text(block).is_some() {
            text = true;
        }
    }
    thinking && !text
}

fn api_error_message(v: &Value) -> Option<String> {
    let err = v.get("error")?;
    if let Some(msg) = err.as_str().map(str::trim).filter(|s| !s.is_empty()) {
        return Some(msg.to_string());
    }
    err.get("message")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn collect_text(v: &Value) -> Option<String> {
    anthropic_text(v)
        .or_else(|| openai_text(v))
        .or_else(|| value_text(v.get("output_text")?))
}

fn anthropic_text(v: &Value) -> Option<String> {
    let content = v.get("content")?;
    if let Some(s) = content.as_str() {
        return nonempty_text(s);
    }
    let blocks = content.as_array()?;
    let mut out = String::new();
    for block in blocks {
        if let Some(t) = block_text(block) {
            out.push_str(&t);
        }
    }
    nonempty_text(&out)
}

fn openai_text(v: &Value) -> Option<String> {
    let message = v.get("choices")?.as_array()?.first()?.get("message")?;
    value_text(message.get("content")?)
        .or_else(|| value_text(message.get("refusal")?))
}

fn value_text(v: &Value) -> Option<String> {
    if v.is_null() {
        return None;
    }
    if let Some(s) = v.as_str() {
        return nonempty_text(s);
    }
    let arr = v.as_array()?;
    let mut out = String::new();
    for part in arr {
        if let Some(t) = block_text(part).or_else(|| part.as_str().map(str::to_string)) {
            out.push_str(&t);
        }
    }
    nonempty_text(&out)
}

fn block_text(block: &Value) -> Option<String> {
    if let Some(s) = block.as_str() {
        return nonempty_text(s);
    }
    let ty = block.get("type").and_then(Value::as_str).unwrap_or("text");
    if matches!(ty, "text" | "output_text" | "input_text") {
        return block
            .get("text")
            .and_then(Value::as_str)
            .and_then(nonempty_text);
    }
    None
}

fn nonempty_text(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// Claude Code 会给模型名加 `[1M]` 这类窗口标记，网关通常不认。
fn normalize_model_name(model: &str) -> &str {
    let trimmed = model.trim();
    match trimmed.rsplit_once('[') {
        Some((name, rest)) if rest.ends_with(']') && !name.is_empty() => name.trim_end(),
        _ => trimmed,
    }
}

fn read_json(path: &Path) -> Result<Value, LlmError> {
    let text = fs::read_to_string(path).map_err(|e| LlmError::Credential(e.to_string()))?;
    serde_json::from_str(&text).map_err(|e| LlmError::Credential(e.to_string()))
}

fn read_toml(path: &Path) -> Result<toml::Value, LlmError> {
    let text = fs::read_to_string(path).map_err(|e| LlmError::Credential(e.to_string()))?;
    toml::from_str(&text).map_err(|e| LlmError::Credential(e.to_string()))
}

fn json_secret(v: Option<&Value>) -> Result<String, LlmError> {
    nonempty_key(v.and_then(Value::as_str).unwrap_or(""))
}

fn toml_secret(v: Option<&toml::Value>) -> Result<String, LlmError> {
    nonempty_key(v.and_then(|x| x.as_str()).unwrap_or(""))
}

fn nonempty_key(s: &str) -> Result<String, LlmError> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        Err(LlmError::Credential("密钥为空".into()))
    } else {
        Ok(trimmed.to_string())
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= max_chars {
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{extract_text, models_url, normalize_model_name, parse_model_ids};
    use crate::llm::LlmError;
    use crate::sources::Protocol;
    use serde_json::json;

    #[test]
    fn normalize_model_name_strips_context_tag() {
        assert_eq!(
            normalize_model_name("claude-haiku-xsy[1M]"),
            "claude-haiku-xsy"
        );
        assert_eq!(normalize_model_name("claude-haiku-xsy"), "claude-haiku-xsy");
        assert_eq!(normalize_model_name("  grok-4.5  "), "grok-4.5");
    }

    #[test]
    fn extract_anthropic_blocks_and_string_content() {
        let blocks = json!({
            "content": [
                {"type": "thinking", "thinking": "x"},
                {"type": "text", "text": "{\"ok\":true}"}
            ]
        })
        .to_string();
        assert_eq!(
            extract_text(Protocol::AnthropicMessages, &blocks).unwrap(),
            "{\"ok\":true}"
        );
        let as_string = json!({"content": "{\"ok\":true}"}).to_string();
        assert_eq!(
            extract_text(Protocol::AnthropicMessages, &as_string).unwrap(),
            "{\"ok\":true}"
        );
    }

    #[test]
    fn extract_openai_shape_even_on_anthropic_protocol() {
        let body = json!({
            "choices": [{"message": {"content": "{\"ok\":true}"}}]
        })
        .to_string();
        assert_eq!(
            extract_text(Protocol::AnthropicMessages, &body).unwrap(),
            "{\"ok\":true}"
        );
        let parts = json!({
            "choices": [{"message": {"content": [{"type": "text", "text": "{\"ok\":true}"}]}}]
        })
        .to_string();
        assert_eq!(
            extract_text(Protocol::OpenAIChat, &parts).unwrap(),
            "{\"ok\":true}"
        );
    }

    #[test]
    fn extract_thinking_only_is_clear_error() {
        let body = json!({
            "content": [{
                "type": "thinking",
                "signature": "",
                "thinking": "先分析信封字段……"
            }]
        })
        .to_string();
        let err = extract_text(Protocol::AnthropicMessages, &body).unwrap_err();
        assert!(
            matches!(err, LlmError::Request(ref msg) if msg.contains("思考过程")),
            "{err:?}"
        );
    }

    #[test]
    fn extract_http200_error_object() {
        let body = json!({"error": {"message": "Invalid model name"}}).to_string();
        let err = extract_text(Protocol::AnthropicMessages, &body).unwrap_err();
        assert!(matches!(err, LlmError::Request(msg) if msg.contains("Invalid model name")));
    }

    #[test]
    fn models_url_openai_matches_chat_base() {
        assert_eq!(
            models_url("https://api.deepseek.com", Protocol::OpenAIChat),
            "https://api.deepseek.com/models"
        );
        assert_eq!(
            models_url("https://api.openai.com/v1", Protocol::OpenAIChat),
            "https://api.openai.com/v1/models"
        );
        assert_eq!(
            models_url(
                "https://example.com/v1/chat/completions",
                Protocol::OpenAIChat
            ),
            "https://example.com/v1/models"
        );
    }

    #[test]
    fn models_url_anthropic_uses_v1_models() {
        assert_eq!(
            models_url("https://api.anthropic.com", Protocol::AnthropicMessages),
            "https://api.anthropic.com/v1/models"
        );
        assert_eq!(
            models_url("https://api.anthropic.com/v1", Protocol::AnthropicMessages),
            "https://api.anthropic.com/v1/models"
        );
        assert_eq!(
            models_url(
                "https://api.anthropic.com/v1/messages",
                Protocol::AnthropicMessages
            ),
            "https://api.anthropic.com/v1/models"
        );
    }

    #[test]
    fn parse_model_ids_from_openai_and_plain_list() {
        let openai = json!({"data":[{"id":"deepseek-chat"},{"id":"deepseek-v4-flash"}]});
        assert_eq!(
            parse_model_ids(&openai),
            vec!["deepseek-chat".to_string(), "deepseek-v4-flash".to_string()]
        );
        let plain = json!({"models":["b","a","a"]});
        assert_eq!(
            parse_model_ids(&plain),
            vec!["a".to_string(), "b".to_string()]
        );
    }
}
