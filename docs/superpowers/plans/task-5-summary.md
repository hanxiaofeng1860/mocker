# Task 5 implementation summary

Local model source scan for Claude Code, Pi, Codex, and Grok. Commit: `9910ec3` (`feat: scan Claude Code, Pi, Codex, Grok model configs`).

## Files changed

Created:

- `src/sources.rs`
- `tests/sources_test.rs`

Modified:

- `src/lib.rs` — `pub mod sources;`
- `Cargo.toml` — `toml = "0.8"`
- `Cargo.lock` — mocker now depends on locked `toml 0.8.23` (crate was already in the lockfile via other packages)

Not created (deferred): LLM HTTP client / import / service / UI.

## Sources API

`scan_sources(home: &Path) -> Vec<ModelSource>`

- `ModelSource { id, label, protocol, base_url, model, available, reason, credential_from }`
- `Protocol::{AnthropicMessages, OpenAIChat}`
- `CredentialFrom::File { path, kind: CredentialKind }` — never a secret field
- `CredentialKind::{ClaudeEnv, PiProvider { name }, CodexProvider { id }, GrokModel { id }}` so a later live read can find the right key in a multi-entry file

Scan order: Claude Code, Pi, Codex, Grok. Missing or unreadable files are skipped.

| Source | Path under `home` | id | available when |
|---|---|---|---|
| Claude Code | `.claude/settings.json` `env.ANTHROPIC_BASE_URL` + `ANTHROPIC_AUTH_TOKEN` + `ANTHROPIC_MODEL` (or `model`) | `claude-code` | base URL and token both non-empty. Protocol AnthropicMessages |
| Pi | `.pi/agent/models.json` `providers.*` | `pi:{name}` | `baseUrl` + `apiKey`. `api` `anthropic-messages` → AnthropicMessages; `openai-completions` / `openai-chat` / missing → OpenAIChat; other `api` values listed unavailable |
| Codex | `.codex/config.toml` `model`, `model_provider`, `[model_providers.<id>]` | `codex:{id}` | `base_url` present. Protocol OpenAIChat. Host `127.0.0.1`/`localhost` TCP probe; closed port → unavailable `"本地端口未开"` |
| Grok | `.grok/config.toml` `[model.<id>]` with `base_url` + `api_key` | `grok:{id}` | both non-empty. Protocol OpenAIChat |

TCP probe is only used for Codex loopback URLs (`connect_timeout` 250ms). Remote URLs are not HTTP-probed.

Unavailable reasons: `"缺 Base URL"`, `"缺 Token"`, `"本地端口未开"`, `"不支持的协议"`. Available sources have an empty `reason`.

## Cargo results

Toolchain: `rustc 1.96.1`, `cargo 1.96.1`.

Locked (`cargo tree -p mocker --depth 1`):

| Crate | Locked version |
|---|---|
| toml | **0.8.23** |

### `cargo test --test sources_test --offline`

```
running 5 tests
test pi_provider_with_base_url_and_api_key ... ok
test selected_claude_code_settings_json_does_not_contain_token ... ok
test claude_settings_with_base_url_and_token_is_available ... ok
test grok_model_with_base_url_and_api_key_is_available ... ok
test codex_local_base_url_on_closed_port_is_unavailable ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### `cargo test --lib --offline`

```
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

`cargo test --test store_test --offline` PASS (3). `cargo test --test scenes_test --offline` PASS (9). `cargo test --test runtime_test --offline` PASS (7).

`cargo fmt --check` PASS.

`cargo clippy --offline --all-targets -- -D warnings` PASS.

## Tests covered

Temp `HOME` fixtures via `tempfile::TempDir` (not the process `HOME`):

1. Claude `settings.json` with base URL + token → available `claude-code`. Serialized `ModelSource` JSON does not contain the fixture token.
2. Pi `models.json` provider with `baseUrl`+`apiKey` → `pi:xsy-llm`, OpenAIChat, available.
3. Codex `[model_providers.x] base_url = "http://127.0.0.1:9"` with port closed → listed, `available == false`, reason `"本地端口未开"`.
4. Grok `[model."grok-4.5"]` with `base_url`+`api_key` → available `grok:grok-4.5`.
5. After selecting `claude-code`, `serde_json::to_string(&GlobalSettings)` contains the source id and not the fixture token.

## API deviations from the plan

Plan snippet (`docs/superpowers/plans/2026-09-03-mocker-desktop.md` Task 5) vs what shipped:

1. **`CredentialKind` enum.** Plan only named `CredentialFrom::File { path, kind }`. Kind is an enum with provider/model ids so Task 6 can re-read the correct key from a shared file. The secret is never stored on `ModelSource`.
2. **TCP probe is Codex-only.** Spec table 5.1 requires a loopback probe for Codex. Plan step 3 says probe only localhost URLs; Claude/Pi/Grok do not probe even if the URL is loopback.
3. **Missing files are omitted**, not listed unavailable. A present Claude file with empty token is listed unavailable (`缺 Token`).
4. **Pi `api` filter.** Spec 5.1 allows `anthropic-messages` or OpenAI-compatible `openai-completions` / `openai-chat`. Other values stay in the list as unavailable (`不支持的协议`).
5. **Codex does not require a file secret.** Availability is `base_url` plus the loopback probe. `model` is copied onto the selected `model_provider` only.
6. **Did not add later-task crates.** No `reqwest` usage here, no LLM client, no UI.

The `block v0.1.6` future-incompat warning is unchanged from Task 1 (GPUI/macOS dep, not mocker sources).
