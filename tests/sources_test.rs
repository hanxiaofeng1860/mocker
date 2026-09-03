use std::fs;
use std::path::Path;

use mocker::domain::GlobalSettings;
use mocker::sources::{
    home_dir, scan_sources, CredentialFrom, CredentialKind, ModelSource, Protocol,
};
use tempfile::TempDir;

const CLAUDE_TOKEN: &str = "sk-ant-fixture-secret-TOKEN";
const PI_KEY: &str = "sk-pi-fixture-key";
const GROK_KEY: &str = "xai-fixture-key";

fn write_file(root: &Path, rel: &str, contents: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn by_id<'a>(sources: &'a [ModelSource], id: &str) -> &'a ModelSource {
    sources
        .iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| panic!("missing source {id} in {sources:?}"))
}

#[test]
fn claude_settings_with_base_url_and_token_is_available() {
    let home = TempDir::new().unwrap();
    write_file(
        home.path(),
        ".claude/settings.json",
        &format!(
            r#"{{
                "env": {{
                    "ANTHROPIC_BASE_URL": "https://claude.example.com",
                    "ANTHROPIC_AUTH_TOKEN": "{CLAUDE_TOKEN}",
                    "ANTHROPIC_MODEL": "claude-sonnet-4"
                }}
            }}"#
        ),
    );

    let sources = scan_sources(home.path());
    let src = by_id(&sources, "claude-code");
    assert!(src.available);
    assert_eq!(src.protocol, Protocol::AnthropicMessages);
    assert_eq!(src.base_url, "https://claude.example.com");
    assert_eq!(src.model, "claude-sonnet-4");
    assert_eq!(src.label, "Claude Code");
    match &src.credential_from {
        CredentialFrom::File { path, kind } => {
            assert!(path.ends_with("settings.json"));
            assert_eq!(*kind, CredentialKind::ClaudeEnv);
        }
    }
    let dumped = serde_json::to_string(src).unwrap();
    assert!(!dumped.contains(CLAUDE_TOKEN));
}

#[test]
fn pi_provider_with_base_url_and_api_key() {
    let home = TempDir::new().unwrap();
    write_file(
        home.path(),
        ".pi/agent/models.json",
        &format!(
            r#"{{
                "providers": {{
                    "xsy-llm": {{
                        "baseUrl": "https://pi.example.com/v1",
                        "api": "openai-completions",
                        "apiKey": "{PI_KEY}",
                        "models": [{{ "id": "pi-model" }}]
                    }}
                }}
            }}"#
        ),
    );

    let sources = scan_sources(home.path());
    let src = by_id(&sources, "pi:xsy-llm");
    assert!(src.available);
    assert_eq!(src.protocol, Protocol::OpenAIChat);
    assert_eq!(src.base_url, "https://pi.example.com/v1");
    assert_eq!(src.model, "pi-model");
    match &src.credential_from {
        CredentialFrom::File { path, kind } => {
            assert!(path.ends_with("models.json"));
            assert_eq!(
                *kind,
                CredentialKind::PiProvider {
                    name: "xsy-llm".into()
                }
            );
        }
    }
    assert!(!serde_json::to_string(src).unwrap().contains(PI_KEY));
}

#[test]
fn codex_local_base_url_on_closed_port_is_unavailable() {
    let home = TempDir::new().unwrap();
    write_file(
        home.path(),
        ".codex/config.toml",
        r#"
model = "gpt-4o"
model_provider = "x"

[model_providers.x]
name = "local"
base_url = "http://127.0.0.1:9"
"#,
    );

    let sources = scan_sources(home.path());
    let src = by_id(&sources, "codex:x");
    assert!(!src.available, "closed loopback port must be unavailable");
    assert_eq!(src.reason, "本地端口未开");
    assert_eq!(src.protocol, Protocol::OpenAIChat);
    assert_eq!(src.base_url, "http://127.0.0.1:9");
    assert_eq!(src.model, "gpt-4o");
}

#[test]
fn grok_model_with_base_url_and_api_key_is_available() {
    let home = TempDir::new().unwrap();
    write_file(
        home.path(),
        ".grok/config.toml",
        &format!(
            r#"
[model."grok-4.5"]
model = "grok-4.5"
base_url = "https://api.x.ai/v1"
api_key = "{GROK_KEY}"
name = "Grok 4.5"
"#
        ),
    );

    let sources = scan_sources(home.path());
    let src = by_id(&sources, "grok:grok-4.5");
    assert!(src.available);
    assert_eq!(src.protocol, Protocol::OpenAIChat);
    assert_eq!(src.base_url, "https://api.x.ai/v1");
    assert_eq!(src.model, "grok-4.5");
    assert_eq!(src.label, "Grok 4.5");
    match &src.credential_from {
        CredentialFrom::File { path, kind } => {
            assert!(path.ends_with("config.toml"));
            assert_eq!(
                *kind,
                CredentialKind::GrokModel {
                    id: "grok-4.5".into()
                }
            );
        }
    }
    assert!(!serde_json::to_string(src).unwrap().contains(GROK_KEY));
}

#[test]
fn selected_claude_code_settings_json_does_not_contain_token() {
    let home = TempDir::new().unwrap();
    write_file(
        home.path(),
        ".claude/settings.json",
        &format!(
            r#"{{
                "env": {{
                    "ANTHROPIC_BASE_URL": "https://claude.example.com",
                    "ANTHROPIC_AUTH_TOKEN": "{CLAUDE_TOKEN}",
                    "ANTHROPIC_MODEL": "claude-sonnet-4"
                }}
            }}"#
        ),
    );

    let sources = scan_sources(home.path());
    let src = by_id(&sources, "claude-code");
    assert!(src.available);

    let settings = GlobalSettings {
        selected_source_id: src.id.clone(),
        selected_model: src.model.clone(),
        ..Default::default()
    };
    let json = serde_json::to_string(&settings).unwrap();
    assert!(json.contains("claude-code"));
    assert!(!json.contains(CLAUDE_TOKEN));
}

#[test]
fn home_dir_resolves() {
    let home = home_dir();
    assert!(!home.as_os_str().is_empty());
}
