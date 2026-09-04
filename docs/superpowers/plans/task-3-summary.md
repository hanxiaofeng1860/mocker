# Task 3 implementation summary

SQLite persistence for projects, endpoints, scenes, fields, settings, and logs. Commit: `aad7bf8` (`feat: sqlite persistence for projects and mocks`).

## Files changed

Created:

- `src/store.rs`
- `tests/store_test.rs`

Modified:

- `src/lib.rs` — `pub mod store;`
- `Cargo.toml` — `rusqlite` (bundled), `uuid` (v4); `tempfile` as a dev-dependency
- `Cargo.lock` — locked those crates

Not created (deferred): runtime / UI / import / llm / service / sources.

## Store API

`Store::{open, open_in_memory, upsert_project, list_projects, get_project, delete_project, upsert_endpoint, list_endpoints, get_endpoint, delete_endpoint, replace_fields, list_fields, upsert_scene, get_scene, set_current_scene, load_settings, save_settings, append_log, list_logs, trim_logs}` plus helper `store::new_id()` (uuid v4).

Schema:

- `projects` — `default_headers` JSON text
- `endpoints` — unique `(project_id, method, path)`
- `fields` — `enum_values` JSON text
- `scenes` — unique `(endpoint_id, kind)`
- `settings` — single row `id=1`; columns match `GlobalSettings` only (no scan-token column)
- `request_logs` — `missing_default_headers` JSON text; cap 500 newest per project

Foreign keys: deleting a project cascades endpoints/fields/scenes/logs; deleting an endpoint cascades fields/scenes. `PRAGMA foreign_keys = ON`.

`append_log` calls `trim_logs` so a project never keeps more than 500 rows. `list_logs` is newest-first (`ORDER BY id DESC`).

## Cargo results

Toolchain: `rustc 1.96.1`, `cargo 1.96.1`.

Locked (`cargo tree -p mocker --depth 1`):

| Crate | Locked version |
|---|---|
| rusqlite | **0.32.1** (features: bundled) |
| uuid | **1.26.0** (features: v4) |
| tempfile (dev) | **3.27.0** |

### `cargo test --test store_test`

```
running 3 tests
test settings_round_trip_persists_manual_key_not_scan_token ... ok
test insert_project_endpoint_and_scenes_then_switch_current ... ok
test trim_logs_keeps_newest_500_and_drops_oldest ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### `cargo test --lib --offline`

```
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

`cargo fmt --check` PASS.

`cargo clippy --offline --all-targets -- -D warnings` PASS.

## Tests covered

1. Insert project + endpoint + 4 scenes; list; `set_current_scene` to `empty`. Also checks `default_headers` JSON round-trip (`sn`).
2. Settings round-trip: `selected_source_id` and `manual_api_key` persist. `PRAGMA table_info(settings)` is exactly `id` + the six `GlobalSettings` fields — no token column.
3. Append 505 logs then `trim_logs`: 500 remain, `/n/0` gone, newest `/n/504`, oldest remaining `/n/5`.

Uses `tempfile` + `Store::open(path)` as specified. `open_in_memory` is implemented but unused by these tests.

## API deviations from the plan

Plan snippet (`docs/superpowers/plans/2026-09-03-mocker-desktop.md` Task 3) vs what shipped:

1. **`store::new_id()` helper.** Plan said IDs are `uuid::Uuid::new_v4().to_string()`. Exposed as a free function so the lib actually uses `uuid` and tests share one constructor. Callers still pass ids into upserts (empty ids are stored as-is).
2. **`replace_fields(endpoint_id, fields)` takes `&[Field]`.** The `endpoint_id` argument is written on insert so a mixed slice cannot attach to another endpoint.
3. **`append_log` auto-trims.** Plan: `trim_logs(project_id)` after append if count > 500. `trim_logs` is still public; the log test calls it after 505 appends.
4. **`load_settings` returns `GlobalSettings::default()` when no row.** Avoids inventing a missing-row error.
5. **Did not add later-task crates.** Only `rusqlite` + `uuid` + `tempfile` as required. No axum / tokio / runtime / UI.

The `block v0.1.6` future-incompat warning is unchanged from Task 1 (GPUI/macOS dep, not mocker sources).
