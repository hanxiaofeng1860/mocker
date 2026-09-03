# Task 10 implementation summary

Settings page for model sources and appearance (scheme C). Commit: `5561f9b` (`feat: settings page for model sources and appearance`).

## Files changed

Created:

- `src/ui/settings.rs` — settings screen, source radios, manual form, theme toggles
- `docs/superpowers/plans/task-10-summary.md`

Modified:

- `src/ui/mod.rs` — `mod settings;`
- `src/ui/app.rs` — `Screen::Settings` body; `SettingsState`; apply saved client on start
- `src/service.rs` — `load_settings` / `save_settings` / `set_model`; `model` behind `Mutex`
- `src/sources.rs` — `home_dir()`
- `tests/service_test.rs` — settings round-trip + `set_model` replaces client
- `tests/sources_test.rs` — `home_dir` resolves

Not created (deferred to Tasks 11–12): `workbench.rs`, `import_view.rs`. Work stays a navigation stub.

## Screen

**设置** (`Screen::Settings { back }`):

- 「← 项目」 restores `Settings.back` (Home / Empty / Work / …), not a hard-coded Home
- Lead: 「低频配置放这里。所有项目共用一套模型来源。」
- Section 「模型来源」: radio rows from `scan_sources(home_dir())`
  - available: selectable; badge 可用; subtitle `anthropic · {model}` or host:port
  - unavailable: disabled + opacity; badge 不可用; subtitle is `reason`
- Buttons: 「刷新扫描」, 「测试连通」, 「手动填写接口」
- 「手动填写接口」 shows Base URL / API Key (masked) / 协议 / 模型; `selected_source_id = "manual"`
- Protocol radios: `anthropic-messages` | `openai-chat` (default `openai-chat`)
- Section 「外观」: Paper / Ink → `apply_paper_theme` / `apply_ink_theme`

Title bar 「设置」 unchanged (no nested Settings).

## Persistence and model client

On source change (and on manual blur / close):

- `GlobalSettings.selected_source_id` (+ `selected_model`; manual fields if `manual`)
- `AppService::save_settings` → Store (scan secrets still not stored)
- scanned available → `AppService::set_model(HttpModelClient::from_source)`
- `manual` → `HttpModelClient::with_inline_key`

`AppView::new` calls `apply_saved_client` so a later import uses the last saved source.

「测试连通」 builds `HttpModelClient` from the current selection, `complete_json("ping")` on the background executor, then `Notification::success("连通成功 · {model}")` or `Notification::error` with status/text.

## AppService

```
load_settings() -> GlobalSettings
save_settings(&GlobalSettings)
set_model(impl ModelClient + Send + Sync + 'static)
```

`model` is `Mutex<Box<dyn ModelClient + Send + Sync>>` so `set_model` is `&self` (same as other use cases). `import_paste` locks around `complete_json`.

## Cargo results

Toolchain: `rustc 1.96.1`, `cargo 1.96.1`. No new crates. Locked (`cargo tree -p mocker --depth 1`) unchanged from Task 9 (`gpui-component 0.5.1`, `gpui 0.2.2`, `tokio 1.53.1`, `directories 5.0.1`, …).

### `cargo test --offline`

```
lib 7 (parse_port + settings helpers); main 0; import_test 6; runtime_test 7; scenes_test 9; service_test 4; sources_test 6; store_test 3
test result: ok
```

### `cargo build --offline`

```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.85s
```

`cargo fmt --check` PASS.

`cargo clippy --offline --all-targets -- -D warnings` PASS.

The `block v0.1.6` future-incompat warning is unchanged from Task 1 (GPUI/macOS dep).

`cargo run` was not executed (window would hang).

## API deviations from the plan

Plan snippet (`docs/superpowers/plans/2026-09-03-mocker-desktop.md` Task 10) vs what shipped:

1. **Standalone `Radio` rows, not `RadioGroup`.** Kit `RadioGroup` applies one `disabled` to every child, so unavailable sources could not be greyed individually.
2. **Manual form is hidden until 「手动填写接口」** (or saved id is already `manual`). HTML mock does not show the fields until that action.
3. **Protocol is a two-option radio**, values `anthropic-messages` / `openai-chat` from spec §4.1, not a free-text Input.
4. **`complete_json("ping")` is background-spawned.** The trait is sync and can block up to the reqwest timeout; toast still lands on the UI thread.
5. **`home_dir()` lives in `sources.rs`** (`directories::UserDirs`, then `HOME` / `USERPROFILE`). UI calls `scan_sources(&home_dir())`.
6. **Cursor / Gemini OAuth rows are not listed.** Spec §5.2 is “show reason, do not request”; Task 5 scan never emitted them. Settings renders scan results only.
7. **Appearance is session-only.** `GlobalSettings` has no theme field; Paper/Ink is not written to SQLite.
8. **Back copy is 「← 项目」** (HTML mock) while the target is `Settings.back` (spec 「返回上一页」).
9. **Did not implement the workbench editor.**
