# Task 3 review — SQLite store

**Verdict:** Pass. 0 open issues.

**Scope:** Task 3 only (`src/store.rs`, `tests/store_test.rs`, `src/lib.rs` `pub mod store`, `Cargo.toml` rusqlite/uuid/tempfile). No code changes in this review.

## Verification

| Check | Result |
|---|---|
| Unique `(project_id, method, path)` | Yes — `src/store.rs:34` `UNIQUE (project_id, method, path)` plus PK on `id`. SQLite autoindex `sqlite_autoindex_endpoints_2` (unique). Duplicate insert is `UNIQUE constraint failed: endpoints.project_id, endpoints.method, endpoints.path`. Same path in another project or another method is allowed. `upsert_endpoint` `ON CONFLICT(id)` does not bypass the triple unique. |
| Logs capped at 500 newest | Yes — `LOG_CAP = 500`; `append_log` always calls `trim_logs`. Trim takes the 500th-newest `id` (`ORDER BY id DESC LIMIT 1 OFFSET 499`) and `DELETE … AND id < min_keep`. `list_logs` is newest-first (`ORDER BY id DESC`). Test `trim_logs_keeps_newest_500_and_drops_oldest`: 505 appends → 500 rows, `/n/0` gone, newest `/n/504`, oldest remaining `/n/5`. Cap is per `project_id`. |
| No scan-token column in settings | Yes — `settings` is `id` + the six `GlobalSettings` fields only (`src/store.rs:61-69`). Single row `id=1` (`PRIMARY KEY CHECK (id = 1)`). Test `settings_round_trip_persists_manual_key_not_scan_token` asserts `PRAGMA table_info(settings)` exact column list and no name containing `token`; `manual_api_key` round-trips. |
| SQL injection (use params) | Yes — schema is a static `&str`. All DML/DQL bind with `?1…` and `params![]` / `query_map(params![…])`. `LOG_CAP - 1` is a bound param, not interpolated. No `format!` / concat into SQL (only into anyhow context for paths). `SceneKind` / `FieldLoc` go in as bound text, not identifiers. |
| Delete project cascades (no orphans) | Yes — FKs `ON DELETE CASCADE`: endpoints → projects; fields/scenes → endpoints; request_logs → projects. `from_conn` runs `PRAGMA foreign_keys = ON` on every open (SQLite defaults off). Deleting a project removes that project's endpoints, fields, scenes, and logs; other projects' rows stay. `delete_endpoint` cascades fields/scenes only; logs stay (no FK on `request_logs.endpoint_id`) — project-scoped history, not leftover project children. |
| Commit | `aad7bf8` `feat: sqlite persistence for projects and mocks` |
| `cargo test --test store_test --offline` | PASS (3 tests) |
| `cargo fmt --check` | PASS |
| `cargo clippy --offline --all-targets -- -D warnings` | PASS |
| Toolchain / lock | rusqlite **0.32.1** (bundled), uuid **1.26.0** (v4), tempfile **3.27.0** |

Plan-required tests are present (`tempfile` + `Store::open(path)`): project+endpoint+4 scenes+`current_scene`; settings without scan secrets; 500-log cap.

Allowed deviations (not issues; match `task-3-summary.md`): `store::new_id()`; `replace_fields(endpoint_id, &[Field])` forces `endpoint_id`; `append_log` always trims (equivalent to “if count > 500”); `load_settings` returns `Default` when no row; later-task crates not added.

Observations (not issues):

- Unique/cascade are schema-enforced but not asserted in `store_test` (plan Step 1 did not require those cases).
- Triple unique is SQLite BINARY: `POST /a` and `post /a` both fit. Spec matching uppercases method later; store does not normalize.
- `delete_endpoint` leaves `request_logs.endpoint_id` pointing at a removed endpoint. Deleting the **project** still removes those log rows.

## Issues

0 open issues.
