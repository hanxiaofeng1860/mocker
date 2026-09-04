# Task 4 review — Mock HTTP runtime

**Verdict:** Pass. 0 open issues.

**Scope:** Task 4 only (`src/runtime.rs`, `tests/runtime_test.rs`, `src/lib.rs` `pub mod runtime`, `Cargo.toml` axum/tokio/reqwest/arc-swap/chrono). No code changes in this review.

## Verification

| Check | Result |
|---|---|
| Bind `127.0.0.1` only | Yes — `TcpListener::bind((Ipv4Addr::LOCALHOST, port))` (`src/runtime.rs:105-106`). Not `0.0.0.0`, not `[::]`. |
| Never change port on conflict | Yes — single bind, no retry / `:0` fallback / store write. Busy port → `Err` with `bind 127.0.0.1:{port}`. `inner` insert only after bind succeeds, so `is_running` stays false. Test `start_fails_when_port_occupied`. |
| OPTIONS 204, no log | Yes — uppercase `OPTIONS` returns before lookup / `append_log` (`src/runtime.rs:232-235`). 204 + CORS, empty body, no `Content-Type`. Test `options_returns_204_with_cors` asserts empty `list_logs`. |
| 404/405 envelopes use project `fail_code` via `encode_code` | Yes — `envelope` is `json!({ "code": encode_code(fail_code), "msg", "data": null })`. 404 `"未找到 mock 接口"`, 405 `"方法不允许"`. `fail_code` comes from `RouteSnapshot` (copied from `project.fail_code` at snapshot build). Tests assert `body["code"] == encode_code("9999")` (JSON number). |
| `refresh` without restart | Yes — rebuilds snapshot, `ArcSwap::store`, does not stop/rebind. Test `refresh_after_scene_change_updates_body_without_restart`: `set_current_scene(Empty)` + `refresh` → new body, `is_running` still true. |
| CORS headers | Yes — `apply_cors` on OPTIONS and every `json_response` (200/404/405/500): `Origin: *`, `Methods: GET, POST, PUT, PATCH, DELETE, OPTIONS`, `Headers: *`. JSON responses also set `Content-Type: application/json; charset=utf-8`. |
| Missing default headers still 200 + logged | Yes — keys compared case-insensitively; no reject. Hit still returns current scene. `RequestLog.missing_default_headers` records the missing keys. Test omits `sn`, asserts 200 and log contains `sn`. |
| No deadlock: store mutex vs async | Yes — `std::sync::Mutex` is never held across `.await`. Handler awaits `to_bytes` first, then locks store only for `append_log`/`trim_logs`, then drops the guard before returning. `MutexGuard` is `!Send`; holding it across await would not compile against axum's `Send` connection future. Lock order never nests: `start` is store then inner (store released first); `refresh` is inner then store (inner released first); handler/stop each take one lock. `ArcSwap` load is dropped before the store lock. |
| Commit | `15388bc` `feat: in-process mock HTTP runtime with hot reload` |
| `cargo test --test runtime_test --offline` | PASS (7 tests) |
| `cargo test --lib --offline` | PASS (0 tests) |
| `cargo fmt --check` | PASS |
| `cargo clippy --offline --all-targets -- -D warnings` | PASS |
| Toolchain / lock | rustc **1.96.1**; axum **0.8.9**, tokio **1.53.1**, reqwest **0.12.28**, arc-swap **1.9.2**, chrono **0.4.45** |

Plan-required tests are present (`#[tokio::test]` + reqwest against `127.0.0.1` + ephemeral port from `TcpListener::bind("127.0.0.1:0")` + tempfile store): POST hit; scene change + `refresh` without restart; 404/405 envelopes; OPTIONS 204 + CORS and no log; missing `sn` still 200 and logged; occupied port → error, not running.

Allowed deviations (not issues; match `task-4-summary.md`): `Handle::try_current()` else owned multi-thread runtime (`enable_io` only); `refresh` no-op when not running; enabled-only snapshot; skip endpoint if current scene row is missing; chrono `clock` only; reqwest as a normal dependency; `Drop` signals shutdown for every project.

## Observations (not issues)

- `stop` sends the watch flag and removes the map entry but does not join the serve task. `is_running` is false immediately; the listener is dropped only after the tokio task observes shutdown. A tight stop→start can still see `EADDRINUSE`. Plan tests do not cover stop/restart.
- Callers must not hold `store` when calling `start` / `refresh` (those methods lock it). Same-thread re-lock of `std::sync::Mutex` would deadlock. Task 7 should drop the guard before refresh.
- Blocking `append_log` (plus trim) runs on the tokio worker. That can stall that worker under SQLite load; it is not a lock-order deadlock.
- Log `url` is `req.uri().to_string()` (origin-form path/query), not scheme+host. Spec §4.6 says 完整 URL.
- Snapshot is loaded after the body is read, so a `refresh` during a large upload can affect that in-flight request. Spec says in-flight uses the snapshot at entry.
- `handle_request` always returns `Ok`; oversized bodies become `""` via `unwrap_or_default` instead of the 500 envelope. The 500 path is defensive.
- Port `0` would bind an ephemeral port (`bind(127.0.0.1:0)`). UI/service should reject it later; Task 4 has no port validator.
- CORS on 200/404/405 is implemented but only asserted on OPTIONS.

## Issues

0 open issues.
