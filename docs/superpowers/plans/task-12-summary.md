# Task 12 implementation summary

Paste import with preview (scheme C). Commit: `feat: paste import with preview`.

## Files changed

Created:

- `src/ui/import_view.rs` — import screen: paste, parse, preview, commit
- `docs/superpowers/plans/task-12-summary.md`

Modified:

- `src/ui/mod.rs` — `mod import_view;`
- `src/ui/app.rs` — `Screen::Import` body; `ImportState`; drop the Import stub
- `src/service.rs` — `model` is `Arc<Mutex<…>>` so parse can run off the UI thread; `import_paste_job` / `remember_import_paste`

## Screen

**导入接口** (`Screen::Import { project_id }`):

- 「← 工作台」 and 「取消」 → `Screen::Work { same project_id }` without writing. In-flight parse is ignored.
- Large multiline textarea (placeholder from the HTML mock).
- Muted source line: 「将使用设置里的模型来源（当前 {label}）。」 or 「尚未选择模型来源…」.

「解析」:

- Empty paste → error area 「请先粘贴接口说明」, no LLM call.
- No source (`selected_source_id` empty / unavailable / `Unconfigured`) → dialog title 「去设置里选择模型来源」, OK 「去设置」 → `Screen::Settings { back: Import }`. Paste is kept.
- Else `AppService::import_paste_job` on `background_spawn` (does not block the UI). Ticket `AtomicU64`: 「取消解析」, leave page, or a newer parse ignores late results. `last_paste` is written on the UI thread only after the ticket still matches.

Preview: checkbox table (方法 / 路径 / 名称). Empty path, `(method, path)` already in the project, or two **checked** rows with the same pair are marked red. Default check skips those. 「写入」 disabled while any checked row is marked, or none are checked.

「写入」: `commit_import` of the checked valid drafts, then `Screen::Work` (workbench reloaded). LLM / validate errors stay in the preview error area; paste is kept; nothing is written.

## AppService

```
import_paste(project_id, paste) -> Vec<ImportDraft>   // tests: parse + remember
import_paste_job(project_id, paste)                   // UI background: parse only
remember_import_paste(project_id, paste)              // UI applies a accepted parse
commit_import(project_id, drafts)
```

## Cargo results

Toolchain: `rustc 1.96.1`, `cargo 1.96.1`. No new crates. Locked (`cargo tree -p mocker --depth 1`) unchanged from Task 11 (`gpui-component 0.5.1`, `gpui 0.2.2`, `tokio 1.53.1`, `directories 5.0.1`, …).

### `cargo test --offline`

```
lib 15 (previous helpers + import row/commit); main 0; import_test 6; runtime_test 7; scenes_test 9; service_test 8; sources_test 6; store_test 4
test result: ok
```

### `cargo build --offline`

```
Finished `dev` profile [unoptimized + debuginfo] target(s) in ~1s
```

`cargo fmt --check` PASS.

Manual paste-through-UI with a live scanned source not run here (`cargo run` was out of scope).
