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
        let model = self.model.clone();
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
                .header("anthropic-version", "2023-06-01"),
            json!({
                "model": model,
                "max_tokens": 8192,
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

fn extract_text(protocol: Protocol, text: &str) -> Result<String, LlmError> {
    let v: Value =
        serde_json::from_str(text).map_err(|e| LlmError::Request(format!("响应不是 JSON: {e}")))?;
    let content = match protocol {
        Protocol::AnthropicMessages => anthropic_text(&v),
        Protocol::OpenAIChat => openai_text(&v),
    };
    match content {
        Some(s) if !s.trim().is_empty() => Ok(s),
        _ => Err(LlmError::EmptyResponse),
    }
}

fn anthropic_text(v: &Value) -> Option<String> {
    let blocks = v.get("content")?.as_array()?;
    let mut out = String::new();
    for block in blocks {
        let is_text = block.get("type").and_then(Value::as_str).unwrap_or("text") == "text";
        if is_text {
            if let Some(t) = block.get("text").and_then(Value::as_str) {
                out.push_str(t);
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn openai_text(v: &Value) -> Option<String> {
    v.get("choices")?
        .as_array()?
        .first()?
        .get("message")?
        .get("content")?
        .as_str()
        .map(str::to_string)
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
