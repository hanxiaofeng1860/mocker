use std::fs;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    AnthropicMessages,
    OpenAIChat,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSource {
    pub id: String,
    pub label: String,
    pub protocol: Protocol,
    pub base_url: String,
    pub model: String,
    pub available: bool,
    pub reason: String,
    pub credential_from: CredentialFrom,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CredentialFrom {
    File { path: PathBuf, kind: CredentialKind },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CredentialKind {
    ClaudeEnv,
    PiProvider { name: String },
    CodexProvider { id: String },
    GrokModel { id: String },
}

pub fn scan_sources(home: &Path) -> Vec<ModelSource> {
    let mut out = Vec::new();
    scan_claude(home, &mut out);
    scan_pi(home, &mut out);
    scan_codex(home, &mut out);
    scan_grok(home, &mut out);
    out
}

fn scan_claude(home: &Path, out: &mut Vec<ModelSource>) {
    let path = home.join(".claude").join("settings.json");
    let Some(root) = read_json(&path) else {
        return;
    };
    let env = root.get("env").and_then(Value::as_object);
    let base_url = env
        .and_then(|e| map_str(e, "ANTHROPIC_BASE_URL"))
        .unwrap_or("")
        .to_string();
    let token_ok = env
        .and_then(|e| map_str(e, "ANTHROPIC_AUTH_TOKEN"))
        .is_some();
    let model = env
        .and_then(|e| map_str(e, "ANTHROPIC_MODEL").or_else(|| map_str(e, "model")))
        .or_else(|| json_str(&root, "model"))
        .unwrap_or("")
        .to_string();
    let (available, reason) = availability(&base_url, token_ok, false);
    out.push(ModelSource {
        id: "claude-code".into(),
        label: "Claude Code".into(),
        protocol: Protocol::AnthropicMessages,
        base_url,
        model,
        available,
        reason,
        credential_from: CredentialFrom::File {
            path,
            kind: CredentialKind::ClaudeEnv,
        },
    });
}

fn scan_pi(home: &Path, out: &mut Vec<ModelSource>) {
    let path = home.join(".pi").join("agent").join("models.json");
    let Some(root) = read_json(&path) else {
        return;
    };
    let Some(providers) = root.get("providers").and_then(Value::as_object) else {
        return;
    };
    for (name, spec) in providers {
        let Some(spec) = spec.as_object() else {
            continue;
        };
        let base_url = map_str(spec, "baseUrl").unwrap_or("").to_string();
        let token_ok = map_str(spec, "apiKey").is_some();
        if base_url.is_empty() && !token_ok {
            continue;
        }
        let api = spec
            .get("api")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        let (protocol, proto_ok) = match api {
            "" | "openai-completions" | "openai-chat" => (Protocol::OpenAIChat, true),
            "anthropic-messages" => (Protocol::AnthropicMessages, true),
            _ => (Protocol::OpenAIChat, false),
        };
        let model = first_pi_model(spec);
        let label = map_str(spec, "name")
            .map(str::to_string)
            .unwrap_or_else(|| format!("Pi ({name})"));
        let (mut available, mut reason) = availability(&base_url, token_ok, false);
        if available && !proto_ok {
            available = false;
            reason = "不支持的协议".to_string();
        }
        out.push(ModelSource {
            id: format!("pi:{name}"),
            label,
            protocol,
            base_url,
            model,
            available,
            reason,
            credential_from: CredentialFrom::File {
                path: path.clone(),
                kind: CredentialKind::PiProvider { name: name.clone() },
            },
        });
    }
}

fn scan_codex(home: &Path, out: &mut Vec<ModelSource>) {
    let path = home.join(".codex").join("config.toml");
    let Some(root) = read_toml(&path) else {
        return;
    };
    let selected = toml_str(&root, "model_provider");
    let default_model = toml_str(&root, "model").unwrap_or("");
    let Some(providers) = root.get("model_providers").and_then(|v| v.as_table()) else {
        return;
    };
    for (id, spec) in providers {
        let Some(spec) = spec.as_table() else {
            continue;
        };
        let base_url = toml_table_str(spec, "base_url").unwrap_or("").to_string();
        if base_url.is_empty() {
            continue;
        }
        let label = toml_table_str(spec, "name")
            .map(str::to_string)
            .unwrap_or_else(|| format!("Codex ({id})"));
        let model = if selected == Some(id.as_str()) {
            default_model.to_string()
        } else {
            String::new()
        };
        // Codex keys live in env; availability is base_url plus a loopback TCP probe.
        let (available, reason) = availability(&base_url, true, true);
        out.push(ModelSource {
            id: format!("codex:{id}"),
            label,
            protocol: Protocol::OpenAIChat,
            base_url,
            model,
            available,
            reason,
            credential_from: CredentialFrom::File {
                path: path.clone(),
                kind: CredentialKind::CodexProvider { id: id.clone() },
            },
        });
    }
}

fn scan_grok(home: &Path, out: &mut Vec<ModelSource>) {
    let path = home.join(".grok").join("config.toml");
    let Some(root) = read_toml(&path) else {
        return;
    };
    let Some(models) = root.get("model").and_then(|v| v.as_table()) else {
        return;
    };
    for (id, spec) in models {
        let Some(spec) = spec.as_table() else {
            continue;
        };
        let base_url = toml_table_str(spec, "base_url").unwrap_or("").to_string();
        let token_ok = toml_table_str(spec, "api_key").is_some();
        if base_url.is_empty() && !token_ok {
            continue;
        }
        let model = toml_table_str(spec, "model").unwrap_or(id).to_string();
        let label = toml_table_str(spec, "name")
            .map(str::to_string)
            .unwrap_or_else(|| format!("Grok ({id})"));
        let (available, reason) = availability(&base_url, token_ok, false);
        out.push(ModelSource {
            id: format!("grok:{id}"),
            label,
            protocol: Protocol::OpenAIChat,
            base_url,
            model,
            available,
            reason,
            credential_from: CredentialFrom::File {
                path: path.clone(),
                kind: CredentialKind::GrokModel { id: id.clone() },
            },
        });
    }
}

fn availability(base_url: &str, secret_ok: bool, probe_loopback: bool) -> (bool, String) {
    if base_url.trim().is_empty() {
        return (false, "缺 Base URL".to_string());
    }
    if !secret_ok {
        return (false, "缺 Token".to_string());
    }
    if probe_loopback {
        if let Some((host, port)) = loopback_host_port(base_url) {
            if !tcp_open(&host, port) {
                return (false, "本地端口未开".to_string());
            }
        }
    }
    (true, String::new())
}

fn loopback_host_port(base_url: &str) -> Option<(String, u16)> {
    let url = base_url.trim();
    let (rest, default_port): (&str, u16) = if let Some(rest) = url.strip_prefix("https://") {
        (rest, 443)
    } else if let Some(rest) = url.strip_prefix("http://") {
        (rest, 80)
    } else {
        return None;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let hostport = authority
        .rsplit_once('@')
        .map(|(_, h)| h)
        .unwrap_or(authority);
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) if p.bytes().all(|b| b.is_ascii_digit()) => (h, p.parse().ok()?),
        _ => (hostport, default_port),
    };
    if host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" {
        Some((host.to_string(), port))
    } else {
        None
    }
}

fn tcp_open(host: &str, port: u16) -> bool {
    let Ok(addrs) = (host, port).to_socket_addrs() else {
        return false;
    };
    addrs
        .take(8)
        .any(|addr| TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok())
}

fn first_pi_model(spec: &serde_json::Map<String, Value>) -> String {
    let Some(models) = spec.get("models") else {
        return String::new();
    };
    if let Some(arr) = models.as_array() {
        for item in arr {
            if let Some(id) = json_str(item, "id").or_else(|| nonempty_str(item.as_str())) {
                return id.to_string();
            }
        }
    }
    String::new()
}

fn read_json(path: &Path) -> Option<Value> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn read_toml(path: &Path) -> Option<toml::Value> {
    let text = fs::read_to_string(path).ok()?;
    toml::from_str(&text).ok()
}

fn map_str<'a>(m: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a str> {
    nonempty_str(m.get(key).and_then(Value::as_str))
}

fn json_str<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    nonempty_str(v.get(key).and_then(Value::as_str))
}

fn toml_str<'a>(v: &'a toml::Value, key: &str) -> Option<&'a str> {
    nonempty_str(v.get(key).and_then(|x| x.as_str()))
}

fn toml_table_str<'a>(t: &'a toml::Table, key: &str) -> Option<&'a str> {
    nonempty_str(t.get(key).and_then(|x| x.as_str()))
}

fn nonempty_str(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|s| !s.is_empty())
}
