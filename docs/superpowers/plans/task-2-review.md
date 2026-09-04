# Task 2 review — Domain types and scene JSON helpers (re-review)

**Verdict:** Pass. 0 open issues.

**Scope:** Task 2 follow-up (`src/scenes.rs`, `tests/scenes_test.rs`). Prior issues 1–3 were re-checked against the current tree.

## Verification

| Check | Result |
|---|---|
| Empty object without `list`/`total` → `data: {}` | Yes — `src/scenes.rs:72-73`; test `empty_scene_non_list_object_data_becomes_empty_object` |
| List/`total` object still zeros those keys and keeps siblings | Yes — `extra == 1` still asserted |
| Array `data` → `[]` | Yes — `empty_scene_array_data_becomes_empty_array` |
| `encode_code` no `unwrap`; i64 overflow → string | Yes — `src/scenes.rs:7-11`; test uses `9223372036854775808` |
| `encode_code`: `9999` number, `0000` string | Yes |
| `scene_http_status`: ParamError 400, other three 200 | Yes — tested |
| `default_success_envelope("0000")` envelope | Yes — tested |
| Commit | `63f033d` `fix: empty-scene object rule and safer encode_code` |
| `cargo test --test scenes_test --offline` | PASS (9 tests) |
| `cargo test --lib --offline` | PASS (0 tests) |
| `cargo fmt --check` | PASS |
| `cargo clippy --offline --all-targets -- -D warnings` | PASS |

## Previous issues

### Issue 1 -- Severity: bug
- **File**: src/scenes.rs:65
- **Description**: Empty `data` objects without `list`/`total` were left populated; spec §4.5 otherwise `{}`.
- **Suggestion**: Return `json!({})` when the object has neither key.
- **Status**: fixed

### Issue 2 -- Severity: suggestion
- **File**: src/scenes.rs:7
- **Description**: `parse::<i64>().unwrap()` panics on spec-regex integers outside `i64`.
- **Suggestion**: On parse `Err`, return `Value::String` (no `f64`).
- **Status**: fixed

### Issue 3 -- Severity: suggestion
- **File**: tests/scenes_test.rs:28
- **Description**: Spec §10.3 参数错误 400 / envelope / array empty `data` untested.
- **Suggestion**: Cover `scene_http_status`, `default_success_envelope`, array empty scene.
- **Status**: fixed

## Issues

0 open issues.
