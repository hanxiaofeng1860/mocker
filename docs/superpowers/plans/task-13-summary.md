# Task 13 implementation summary

Spec HTTP acceptance (store + service, no UI, no live LLM). Commit: `test: HTTP acceptance for two projects and scene switch`.

## Files changed

Created:

- `tests/acceptance_http.rs` — one `#[tokio::test]` covering the automated spec path
- `docs/superpowers/plans/task-13-summary.md`

No production code changes. No new crates.

## Automated coverage (`Store` + `AppService` + Fake `ModelClient`)

`two_projects_import_scene_switch_404_and_missing_sn`:

1. **Two projects, two ports.** Bind `127.0.0.1:0` twice, drop listeners, reuse those ports. Both `start` and both respond (`queryPhoneHomeData` on phone, `/untitled` on other).
2. **Import fixture JSON (no live LLM).** Fake `ModelClient::complete_json` returns two drafts: `POST /hl/pub/phone/v1/queryPhoneHomeData` (overview envelope) and `POST /hl/pub/phone/v1/queryCallRecordList` (`data.list` + `total`). `import_paste` + `commit_import`.
3. **Success body structure** via reqwest: home `code` `"0000"`, `todayInbound` 12 / `yesterdayInbound` 9 / `recent7DayInbound` 47; list two rows and `total` 2.
4. **Switch to empty scene** on the list endpoint (`set_scene` → `refresh`): next POST returns `list: []`, `total: 0`, still HTTP 200 and success code.
5. **Unknown path** `POST /no/such` → 404 `{code: 9999, msg: "未找到 mock 接口", data: null}`.
6. **Missing `sn`.** Phone project default header `sn`. First home POST has no `sn` → still 200; `logs().missing_default_headers` contains `"sn"`.

Phone project uses default header `sn` (runtime does not inject or reject). Other project is a second listener only (`create_endpoint` `/untitled`).

## Manual (not run)

Plan Step 2: paste text from `设计文档v1.0.0.0.pdf` (home overview, call record list, modify with header `sn`) through the UI with a real scanned source. Out of scope here (no UI, no live LLM).

## Cargo results

Toolchain: `rustc 1.96.1`, `cargo 1.96.1`. Locked deps unchanged from Task 12.

### `cargo test --offline`

```
lib 15; main 0; acceptance_http 1; import_test 6; runtime_test 7; scenes_test 9; service_test 8; sources_test 6; store_test 4
test result: ok
```

`cargo fmt --check` PASS.

No existing tests broken.
