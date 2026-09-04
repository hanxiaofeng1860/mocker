# Task 7 implementation summary

Application service wiring Store, RuntimeHub, and ModelClient. Commit: `2a01dfc` (`feat: app service wiring store, runtime, and import`).

## Files changed

Created:

- `src/service.rs`
- `tests/service_test.rs`

Modified:

- `src/lib.rs` — `pub mod service;`

Not created (deferred): UI / GPUI screens.

## API

`AppService` holds `Arc<Mutex<Store>>`, `RuntimeHub` (built from that store), and `Box<dyn ModelClient + Send + Sync>`. Constructor takes `impl ModelClient + Send + Sync + 'static` so tests inject fakes without boxing at the call site.

```
list_projects()
create_project(name, port, success_code, fail_code, headers) -> Project
start(project_id) / stop(project_id)
save_endpoint(ep)                 // persist + refresh
save_scene_body(endpoint_id, kind, body)
set_scene(endpoint_id, kind)
save_fields(endpoint_id, fields)  // persist only; snapshot does not include fields
import_paste(project_id, paste) -> Vec<ImportDraft>
commit_import(project_id, drafts)
logs(project_id)
```

Mutations that change routes (`save_endpoint`, `save_scene_body`, `set_scene`, `commit_import`) call `runtime.refresh`. `RuntimeHub::refresh` is already a no-op when the project is not running.

`import_paste` loads project codes, calls `ModelClient::complete_json(import_prompt(...))`, then `validate_import`. The paste is remembered per project so `commit_import` can set `source_text` to the original full paste (preview and confirm are separate UI steps).

`commit_import` writes each draft as: endpoint (`enabled = !deprecated`, `current_scene = success`, `source_text = paste`) + header/body/response fields + 4 scenes from `attach_scenes`.

## Cargo results

Toolchain: `rustc 1.96.1`, `cargo 1.96.1`.

No new crates. Locked (`cargo tree -p mocker --depth 1`) unchanged from Task 6 (anyhow 1.0.104, axum 0.8.9, tokio 1.53.1, reqwest 0.12.28, rusqlite 0.32.1, tempfile 3.27.0, …).

### `cargo test --test service_test --offline`

```
running 2 tests
test import_paste_then_commit_writes_endpoints_fields_scenes_and_source_text ... ok
test create_start_then_save_scene_body_hot_reloads ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### `cargo test --offline`

```
lib 0; main 0; import_test 6; runtime_test 7; scenes_test 9; service_test 2; sources_test 5; store_test 3
test result: ok
```

`cargo fmt --check` PASS.

`cargo clippy --offline --all-targets -- -D warnings` PASS.

The `block v0.1.6` future-incompat warning is unchanged from Task 1 (GPUI/macOS dep).

## Tests covered

1. **Hot reload:** bind `127.0.0.1:0` for an ephemeral port, `create_project` → `save_endpoint` + `save_scene_body` → `start` → reqwest success JSON → `save_scene_body` with a new body → next reqwest returns the new JSON without restart. `logs()` has two hits.
2. **Import commit:** Fake `ModelClient` returns fixture JSON. `import_paste` then `commit_import` writes one endpoint, header+response fields, four scenes (empty-list rule + param_error HTTP 400), and `source_text` equal to the full paste.

## API deviations from the plan

Plan snippet (`docs/superpowers/plans/2026-09-03-mocker-desktop.md` Task 7) vs what shipped:

1. **`ModelClient` is `Box<dyn ModelClient + Send + Sync>`**, not a type parameter. One concrete `AppService` for later GPUI views. `new` still accepts `impl ModelClient + Send + Sync + 'static`.
2. **`import_paste` calls `complete_json` + `validate_import`**, not `run_import`. `run_import` takes `&impl ModelClient` (Sized); a trait object is `?Sized`. Behavior matches: same prompt, same validation.
3. **`commit_import(project_id, drafts)` does not take the paste.** The original paste is stored from `import_paste` so `source_text` is the full paste used at parse time.
4. **`save_fields` does not `refresh`.** Spec/Task 11: saving the field tree does not rewrite scene JSON; the route snapshot only reads scenes.
5. **Deprecated drafts commit with `enabled = false`** (spec §6.1).
6. **Did not add UI.**
