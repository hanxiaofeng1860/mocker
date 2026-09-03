# Task 2 review — Domain types and scene JSON helpers

**Verdict:** Pass with open issues. Domain types and `encode_code` match the task; `9999` is a JSON number and `0000` is a JSON string. Empty-scene generation does not fully follow spec §4.5.

**Scope:** Task 2 only (`src/domain.rs`, `src/scenes.rs`, `tests/scenes_test.rs`, `src/lib.rs`). Later-task modules and crates are out of scope.

## Verification

| Check | Result |
|---|---|
| Types: `Project`, `Endpoint`, `Field`, `Scene`, `SceneKind`, `FieldLoc`, `GlobalSettings`, `RequestLog`, `HeaderKv` | Yes — match plan snippet (`SceneKind` / `FieldLoc` `snake_case`, `SceneKind::as_str` + `all()`, `RequestLog` without serde) |
| Fns: `encode_code`, `scene_http_status`, `generate_non_success`, `default_success_envelope` | Yes — present and public |
| `encode_code`: JSON number iff `^-?(0|[1-9][0-9]*)$` | Yes — `9999`/`200`/`0`/`-3` → number; `0000` → string |
| `scene_http_status(ParamError)` = 400, else 200 | Yes — implemented, **not tested** |
| `src/lib.rs` exports `domain` + `scenes` + `version()` | Yes |
| Commit | `c40eff7` `feat: domain types and scene JSON helpers` |
| `cargo test --test scenes_test --offline` | PASS (4 tests) |
| `cargo test --lib --offline` | PASS (0 tests) |
| `cargo fmt --check` | PASS |
| `cargo clippy --offline --all-targets -- -D warnings` | PASS |
| Toolchain | `rustc 1.96.1` |

`regex_simple_int` was checked against the spec regex for leading zeros, `-0`, `+1`, decimals, hex, and overflow-length digit strings. Matcher and regex agree; only the post-match `parse::<i64>().unwrap()` is unsafe.

Allowed deviations (not issues for this task):

- Plan test `assert_eq!(body["code"], "9999")` is wrong vs spec: `9999` matches `^-?(0|[1-9][0-9]*)$`, so the body field must be number `9999`. `Value`’s `PartialEq<str>` only matches strings. Test correctly uses `assert_eq!(body["code"], json!(9999))`.
- Extra empty-scene assert `empty["data"]["extra"] == 1` (plan fixture had `extra` but did not assert it).
- Extra test `encode_code("-3")` → JSON number `-3` (allowed by the regex).
- `src/lib.rs` does not declare Task 3+ modules.
- Only `serde` + `serde_json` added; no later-task crates.
- Rustfmt on plan snippets; comment on `regex_simple_int` documenting the spec regex.

## Issues

### Issue 1 -- Severity: bug
- **File**: src/scenes.rs:65
- **Description**: Spec §4.5 empty `data` is a three-way split: (1) object containing `list`/`total` → zero those keys; (2) array → `[]`; (3) **otherwise → `{}`**. `empty_from_success` treats every JSON object as case (1): it only rewrites `list`/`total` when present and otherwise returns the success object unchanged. So `data: { "user": { "id": 1 } }` (homepage-style payload, no list) stays fully populated on the empty scene instead of `{}`. That is the wrong product meaning of 空数据 for non-list endpoints, and Task 6 `attach_scenes` will reuse this helper. Behavior matches the plan snippet; existing tests only cover the list/`total` object. Sibling keys on list objects (`extra: 1`) are also cloned from success rather than reset to spec 「类型默认值」 (number → `0`, string → `""`, …); that part is plan-locked by `tests/scenes_test.rs:25`.
- **Suggestion**: In the `Value::Object` arm, if the object has neither `list` nor `total`, return `json!({})`. Optionally, for remaining sibling keys, write type defaults instead of cloning success values; if so, drop or change the `extra == 1` assertion.
- **Status**: fixed
- **Response**: `empty_from_success` now returns `{}` when `data` is an object with neither `list` nor `total`. Objects that have `list` and/or `total` still zero those keys and keep sibling values (`extra == 1` test unchanged). Array `data` still becomes `[]`. Covered by `empty_scene_non_list_object_data_becomes_empty_object` and the existing list test.

### Issue 2 -- Severity: suggestion
- **File**: src/scenes.rs:7
- **Description**: After `regex_simple_int` succeeds, `encode_code` does `code.parse::<i64>().unwrap()`. The spec regex allows arbitrarily long integers. `"9223372036854775808"` (and any 19+ digit code starting high enough) matches `^-?(0|[1-9][0-9]*)$` but overflows `i64`, so scene generation panics. `success_code` / `fail_code` are user-editable project strings, and this path is on the library hot path (empty/error envelopes, later 404/405). Typical defaults `0000`/`9999` are fine.
- **Suggestion**: If `parse::<i64>()` fails, fall back to `Value::String` (or try `u64` then string) instead of `unwrap`. Do not encode overflowed inputs as a JSON number via `f64` (lossy).
- **Status**: fixed
- **Response**: `encode_code` matches `parse::<i64>()`; on `Err` it returns `Value::String` (no `unwrap`, no `u64`/`f64`). Test `encode_overflow_int_falls_back_to_string` uses `9223372036854775808`.

### Issue 3 -- Severity: suggestion
- **File**: tests/scenes_test.rs:28
- **Description**: Spec §10.3 requires scene-generation tests to cover 信封, `0000` as string, `200` as number, empty `list`/`total`, and **参数错误 400**. `param_error_is_object_with_fail_code` checks body `code`/`msg`/`data` only. `scene_http_status` and `default_success_envelope` are produced by this task and never called. Also untested: `SceneKind::BusinessError`, empty scene when `data` is a JSON array.
- **Suggestion**: Assert `scene_http_status(SceneKind::ParamError) == 400` and `scene_http_status` of the other three kinds == 200. Add a `default_success_envelope("0000")` envelope check (`code` string, `msg` `"成功"`, `data` `{}`). Array-`data` empty scene → `[]` is the other spec §4.5 branch.
- **Status**: fixed
- **Response**: Added `scene_http_status_param_error_is_400_others_200` (ParamError 400, Success/Empty/BusinessError 200), `default_success_envelope_uses_encoded_code` (`code` `"0000"`, `msg` `"成功"`, `data` `{}`), and `empty_scene_array_data_becomes_empty_array` (`data` → `[]`). Non-list object empty scene is covered under Issue 1.

## Implementation Summary

Addressed all three open review issues in `src/scenes.rs` and `tests/scenes_test.rs`.

- **Empty scene (spec §4.5):** `Value::Object` with neither `list` nor `total` → `data: {}`. List payloads still zero `list`/`total` and keep siblings (`extra: 1`). Array `data` → `[]`.
- **`encode_code`:** `parse::<i64>()` success → JSON number; overflow / parse error → JSON string. No `unwrap`, no `f64`.
- **Tests:** `scene_http_status` 400/200 for all four kinds; `default_success_envelope("0000")`; empty array `data`; empty non-list object `data`; i64 overflow string fallback. Existing list/extra and `param_error` 9999-as-number tests unchanged.

`cargo fmt` then `cargo test --test scenes_test`: 9 passed. Commit: `fix: empty-scene object rule and safer encode_code`.
