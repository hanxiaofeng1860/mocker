# Task 4 implementation summary

In-process mock HTTP runtime with ArcSwap hot reload. Commit: `15388bc` (`feat: in-process mock HTTP runtime with hot reload`).

## Files changed

Created:

- `src/runtime.rs`
- `tests/runtime_test.rs`

Modified:

- `src/lib.rs` — `pub mod runtime;`
- `Cargo.toml` — `axum` 0.8, `tokio` (rt-multi-thread, macros, net, sync), `reqwest` (json, rustls-tls, default-features false), `arc-swap`, `chrono` (clock)
- `Cargo.lock` — locked those crates

Not created (deferred): UI / LLM / import / service / sources.

## Runtime API

`RuntimeHub::{new, start, stop, refresh, is_running}`

- `new(store: Arc<Mutex<Store>>)` — `std::sync::Mutex` because rusqlite is not Sync
- `start(project_id)` binds `127.0.0.1:{project.port}` only. Busy port → error; does not pick another port
- `stop(project_id)` sends graceful shutdown
- `refresh(project_id)` rebuilds `ArcSwap<RouteSnapshot>` from store (enabled endpoints only). No-op if the project is not running
- `is_running(project_id)`

Snapshot types:

- `RouteSnapshot { fail_code, default_header_keys, routes: HashMap<(METHOD, path), Route> }`
- `Route { status, body, scene, endpoint_id, enabled }`

Behavior:

- Match: uppercase METHOD + exact path
- Hit: scene HTTP status + `body_json` as stored
- Unknown path: 404 `{code: encode_code(fail_code), msg: "未找到 mock 接口", data: null}`
- Known path, wrong method: 405, `msg` `"方法不允许"`
- Handler error: 500, `msg` `"mock 内部错误"`
- OPTIONS: 204 + CORS, no route match, no log
- Every JSON response: `Access-Control-Allow-Origin: *`, `Access-Control-Allow-Methods: GET, POST, PUT, PATCH, DELETE, OPTIONS`, `Access-Control-Allow-Headers: *`, `Content-Type: application/json; charset=utf-8`
- Missing project default headers: still return current scene; log `missing_default_headers`
- `append_log` on each non-OPTIONS request

Tokio: `Handle::try_current()` when already inside a runtime (`#[tokio::test]`); otherwise an owned multi-thread `Runtime`. Tests do not use gpui.

## Cargo results

Toolchain: `rustc 1.96.1`, `cargo 1.96.1`.

Locked (`cargo tree -p mocker --depth 1`):

| Crate | Locked version |
|---|---|
| axum | **0.8.9** |
| tokio | **1.53.1** (features: rt-multi-thread, macros, net, sync) |
| reqwest | **0.12.28** (features: json, rustls-tls; default-features off) |
| arc-swap | **1.9.2** |
| chrono | **0.4.45** (features: clock) |

### `cargo test --test runtime_test`

```
running 7 tests
test start_fails_when_port_occupied ... ok
test options_returns_204_with_cors ... ok
test unknown_path_returns_404_envelope_with_fail_code ... ok
test missing_default_header_still_200_and_is_logged ... ok
test post_exact_path_returns_success_json ... ok
test get_on_post_path_returns_405 ... ok
test refresh_after_scene_change_updates_body_without_restart ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### `cargo test --lib --offline`

```
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

`cargo test --test store_test --offline` PASS (3). `cargo test --test scenes_test --offline` PASS (9).

`cargo fmt --check` PASS.

`cargo clippy --offline --all-targets -- -D warnings` PASS.

## Tests covered

1. POST exact path → 200 success JSON
2. `set_current_scene(empty)` + `refresh` → next POST is empty body, still running (no restart)
3. Unknown path → 404 envelope; `code` is `encode_code("9999")` (JSON number)
4. GET on POST path → 405, `msg` `"方法不允许"`
5. OPTIONS → 204 + CORS headers; no log row
6. Missing `sn` still 200; `missing_default_headers` contains `sn`
7. `start` while that port is held by another `TcpListener` → error; `is_running` false

Uses `#[tokio::test]` + reqwest against an ephemeral `127.0.0.1` port from `TcpListener::bind("127.0.0.1:0")`. Store is tempfile SQLite.

## API deviations from the plan

Plan snippet (`docs/superpowers/plans/2026-09-03-mocker-desktop.md` Task 4) vs what shipped:

1. **Tokio handle vs owned runtime.** Plan allowed either a global handle started in `main` or a `Runtime` owned by `RuntimeHub`. `new()` uses `Handle::try_current()` so tests drive the server on the test runtime; outside tokio it builds an owned multi-thread runtime (`enable_io` only — no `time` feature).
2. **`refresh` is a no-op when not running.** Start always rebuilds the snapshot. Avoids a dangling swap with no listener.
3. **Enabled-only snapshot.** Disabled endpoints are omitted (unregistered). `Route.enabled` is stored as `true` for inserted routes; lookup still checks the flag.
4. **Missing current scene at snapshot build skips that endpoint.** Tests always insert the scene in use.
5. **chrono `clock` only**, not plan Cargo.toml `serde`. Runtime only needs `Utc::now()` for log `at`.
6. **reqwest is a normal dependency** (plan Task 1 Cargo.toml; later LLM uses it). Tests consume it; no extra dev-dep.
7. **Drop stops listeners.** `RuntimeHub` shutdowns every project on drop so test ports are released.

The `block v0.1.6` future-incompat warning is unchanged from Task 1 (GPUI/macOS dep, not mocker sources).
