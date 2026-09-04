# Task 6 implementation summary

LLM JSON client + paste-import validation and scene attachment. Commit: `2c00e5b` (`feat: import validation and LLM JSON client`).

## Files changed

Created:

- `src/llm.rs`
- `src/import.rs`
- `tests/import_test.rs`

Modified:

- `src/lib.rs` — `pub mod import;` + `pub mod llm;`
- `Cargo.toml` — `thiserror = "2"`
- `Cargo.lock` — mocker now depends on locked `thiserror 2.0.20` (already present as a transitive crate)

Not created (deferred): `AppService`, UI, import preview screen.

## API

`trait ModelClient { fn complete_json(&self, prompt: &str) -> Result<String, LlmError>; }`

- Tests inject `Fake` that returns fixture JSON (optionally wrapped in ` ```json ` fences).
- `HttpModelClient::from_source(&ModelSource)` reads the key from the original file at call time (`read_credential`). `HttpModelClient::with_inline_key` covers the later manual source.
- Anthropic: POST `{base}/v1/messages`, headers `x-api-key` + `anthropic-version: 2023-06-01`.
- OpenAI-compatible: POST `{base}/chat/completions`, `Authorization: Bearer`.
- Spec 5.3 URL: strip trailing `/`; if base already ends with `/v1` or `/v1/messages`, do not append another `/v1`.
- Sync trait + async reqwest: work runs on a spawned current-thread tokio runtime so we never `Runtime::block_on` inside the caller's runtime.

`validate_import(raw: &str) -> Result<Vec<ImportDraft>, ImportError>`

- Strip optional markdown fences, then parse JSON.
- `endpoints` must be a non-empty array.
- Each `path` non-empty and starting with `/`; `method` GET/POST/PUT/PATCH/DELETE (empty/missing → POST); `success_body` must be a JSON object.
- Nested `children` flatten into `Vec<Field>` with `parent_id`. Field `id`s are new UUIDs; `endpoint_id` is empty until commit.

`ImportDraft { name, path, method, deprecated, notes, request_headers, request_body_fields, response_fields, success_body }`

`attach_scenes(draft, success_code, fail_code) -> [Scene; 4]` uses `generate_non_success` + `scene_http_status` (success / empty / param_error / business_error). Empty-list rule is the scenes helper (`list`→`[]`, `total`→`0`, other keys kept).

`import_prompt(paste, success_code, fail_code)` is hard-coded Chinese + the spec §6 JSON schema and the phrase `only return JSON object`.

`run_import(client, paste, success_code, fail_code)` = `complete_json(import_prompt(...))` then `validate_import`.

## Cargo results

Toolchain: `rustc 1.96.1`, `cargo 1.96.1`.

Locked (`cargo tree -p mocker --depth 1`):

| Crate | Locked version |
|---|---|
| thiserror | **2.0.20** |
| reqwest | **0.12.28** (features: json, rustls-tls; default-features off; already present) |

### `cargo test --test import_test --offline`

```
running 6 tests
test empty_endpoints_is_error ... ok
test non_json_is_error ... ok
test missing_path_is_error ... ok
test run_import_uses_complete_json_then_validate ... ok
test valid_one_endpoint_yields_one_draft ... ok
test attach_scenes_four_kinds_and_empty_list_rule ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### `cargo test --lib --offline`

```
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

`cargo test --test scenes_test --offline` PASS (9). `cargo test --test store_test --offline` PASS (3). `cargo test --test runtime_test --offline` PASS (7). `cargo test --test sources_test --offline` PASS (5).

`cargo fmt --check` PASS.

`cargo clippy --offline --all-targets -- -D warnings` PASS.

## Tests covered

1. Valid model JSON with one endpoint and object `success_body` → one `ImportDraft` (path/method/name/header `sn`).
2. Missing path / empty `endpoints` / non-JSON → `ImportError`, no drafts.
3. `attach_scenes` emits 4 kinds; empty scene follows list/total rule; param_error HTTP 400 and fail code; business_error HTTP 200.
4. `Fake` `ModelClient`: `run_import(paste)` calls `complete_json` with a prompt that includes `only return JSON object` + schema, then `validate_import` (fenced fixture is accepted).

## API deviations from the plan

Plan snippet (`docs/superpowers/plans/2026-09-03-mocker-desktop.md` Task 6) vs what shipped:

1. **`run_import` takes `(client, paste, success_code, fail_code)`.** The plan’s test line is `run_import(paste)`. Codes are required by `import_prompt` (spec §6.1).
2. **`HttpModelClient` is real but untested against the network.** Tests inject `Fake`. Credential read is live from the scan file (Claude `ANTHROPIC_AUTH_TOKEN`, Pi `apiKey`, Grok `api_key`, Codex `api_key` or `env_key` / `OPENAI_API_KEY`).
3. **Missing/empty method defaults to POST** (domain default). Invalid verbs still error.
4. **Field `id` assigned at validate time** so `children` can set `parent_id`. `endpoint_id` on fields and attached scenes stays empty until Task 7 commit.
5. **Did not add `AppService` or UI.**

The `block v0.1.6` future-incompat warning is unchanged from Task 1 (GPUI/macOS dep, not mocker import/llm).
