# Task 2 implementation summary

Domain types and scene JSON helpers. Commit: `c40eff7` (`feat: domain types and scene JSON helpers`).

## Files changed

Created:

- `src/domain.rs`
- `src/scenes.rs`
- `tests/scenes_test.rs`

Modified:

- `src/lib.rs` — `pub mod domain;` + `pub mod scenes;` (kept `version()`)
- `Cargo.toml` — `serde` (derive) + `serde_json`
- `Cargo.lock` — `mocker` now depends on those two

Not created (deferred): store / runtime / UI / import / llm / service / sources.

## Types and functions

`domain::{Project, Endpoint, Field, Scene, SceneKind, FieldLoc, GlobalSettings, RequestLog, HeaderKv}` match the plan snippet (including `SceneKind` / `FieldLoc` `snake_case` serde, `SceneKind::as_str` + `all()`, `RequestLog` without serde).

`scenes::{encode_code, scene_http_status, generate_non_success, default_success_envelope}` match the plan snippet. `encode_code` uses the no-regex integer check `^-?(0|[1-9][0-9]*)$`.

## Cargo results

Toolchain: `rustc 1.96.1`, `cargo 1.96.1`.

Locked (`cargo tree -p mocker --depth 1`):

| Crate | Locked version |
|---|---|
| serde | **1.0.229** (features: derive) |
| serde_json | **1.0.151** |

### `cargo test --test scenes_test`

```
running 4 tests
test encode_leading_zeros_as_string ... ok
test param_error_is_object_with_fail_code ... ok
test encode_negative_int_as_number ... ok
test empty_list_scene_uses_success_code ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### `cargo test --lib --offline`

```
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`cargo fmt --check` PASS.

## API deviations from the plan

Plan snippet (`docs/superpowers/plans/2026-09-03-mocker-desktop.md` Task 2) vs what shipped:

1. **`param_error` code assertion is a JSON number.** Plan test used `assert_eq!(body["code"], "9999")`. `encode_code("9999")` matches `^-?(0|[1-9][0-9]*)$`, so the body field is `9999` (number), not `"9999"` (string). `serde_json::Value`’s `PartialEq<str>` only matches strings, so the plan assertion would fail against a correct encoder. Test uses `assert_eq!(body["code"], json!(9999))`. Fail code is still 9999.

2. **Empty scene also asserts extra keys.** Plan fixture already had `"extra": 1` but did not assert it. Added `assert_eq!(empty["data"]["extra"], 1)` so list/total zeroing does not drop sibling keys. `code` stays string `"0000"`.

3. **Optional negative-int test.** `encode_code("-3")` is JSON number `-3` (`encode_negative_int_as_number`). Allowed by the task instruction.

4. **`src/lib.rs` only exports modules that exist.** Plan Task 1 listed `import` / `llm` / `runtime` / `service` / `sources` / `store` as well. Those files are Task 3+; declaring them would not compile. Only `domain` and `scenes` plus `version()`.

5. **Rustfmt on plan snippets.** Multi-line struct/enum variants, `generate_non_success` signature, and a one-line comment on `regex_simple_int` documenting the spec regex. No behavior change.

6. **Did not add later-task crates.** Only `serde` + `serde_json` as required. No axum / rusqlite / tokio / store / runtime / UI.

The `block v0.1.6` future-incompat warning is unchanged from Task 1 (GPUI/macOS dep, not mocker sources).
