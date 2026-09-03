# Task 9 implementation summary

Empty, home, and new-project screens (scheme C). Commit: `2fabbcd` (`feat: project home and new-project form`).

## Files changed

Created:

- `src/ui/home.rs` — Empty + Home
- `src/ui/new_project.rs` — form + `InputState` + submit
- `docs/superpowers/plans/task-9-summary.md`

Modified:

- `src/ui/mod.rs` — `mod home; mod new_project;`
- `src/ui/app.rs` — body switches on `Screen`; navigation helpers
- `src/ui/theme.rs` — popover / muted tokens for cards and labels
- `src/service.rs` — `list_endpoints`, `is_running`
- `tests/service_test.rs` — assert those two methods around start/stop

Not created (deferred to Tasks 10–12): `settings.rs`, `workbench.rs`, `import_view.rs`. Settings stays a placeholder. Work is a stub (`← 项目` + `Work {id}`) so a created project can be seen on Home without restarting.

## Screens

**Empty** (initial when `list_projects` is empty, or after cancel/back with no rows):

- Title 「还没有项目」
- Muted lead from the HTML mock
- Primary 「新建项目」 → `Screen::NewProject`

**Home** (title 「项目」, not a dashed new-project card):

- Right primary 「新建项目」
- Summary `{n} 个 · {m} 个运行中`
- Section 「运行中」 then 「全部项目」
- Cards: name, badge 运行中/`Tag::success` or 已停止/`Tag::secondary`, `:{port}` (mono), `{n} 个接口`
- Click card → `Screen::Work { project_id }`
- Running section uses `AppService::is_running`; counts use `list_endpoints`

**New project** (`gpui-component` `Input` + `InputState`, `v_form` 2 columns):

| Field | Default / placeholder |
|---|---|
| 项目名称 | placeholder `例如：智能话机` |
| 端口 | placeholder `7788` |
| 成功码 | `0000` |
| 失败码 | `9999` |
| 默认请求头（可选，如 sn） | placeholder `sn` |

- 「创建并打开」 → `create_project` then `Screen::Work`
- Invalid port (`parse_port`: empty / 0 / non-u16): `window.push_notification(Notification::error("端口无效"))`, no insert
- Optional header: trimmed key → `HeaderKv { key, value: "" }`; empty → no headers
- Empty success/fail codes fall back to `0000` / `9999`
- 「← 项目」 / 「取消」 → Home if any projects, else Empty
- Global title bar 「设置」 unchanged

`InputState` is created on enter (`cx.new(|cx| InputState::new(window, cx)…)`), not in `AppView::new` (no `Window` there).

## AppService

```
list_endpoints(project_id) -> Vec<Endpoint>  // Store::list_endpoints
is_running(project_id) -> bool               // RuntimeHub::is_running
```

## Theme

Cards/form sheet use `cx.theme().popover`; labels use `muted_foreground`. Task 8 did not set those, so Light defaults would be white-on-beige.

Paper: popover `#F8F3E8`, muted `#E8DFD0`, muted_foreground `#7A6F64`.  
Ink: popover `#252525`, muted `#2A2A2A`, muted_foreground `#999999`.

## Cargo results

Toolchain: `rustc 1.96.1`, `cargo 1.96.1`. No new crates. Locked (`cargo tree -p mocker --depth 1`) unchanged from Task 8 (`gpui-component 0.5.1`, `gpui 0.2.2`, `tokio 1.53.1`, …).

### `cargo test --offline`

```
lib 2 (parse_port); main 0; import_test 6; runtime_test 7; scenes_test 9; service_test 2; sources_test 5; store_test 3
test result: ok
```

### `cargo build --offline`

```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 8.65s
```

`cargo fmt --check` PASS.

`cargo clippy --offline --all-targets -- -D warnings` PASS.

The `block v0.1.6` future-incompat warning is unchanged from Task 1 (GPUI/macOS dep).

`cargo run` was not executed (window would hang).

## API deviations from the plan

Plan snippet (`docs/superpowers/plans/2026-09-03-mocker-desktop.md` Task 9) vs what shipped:

1. **Work is a navigation stub, not the editor.** Task 11 owns the workbench. Stub has 「← 项目」 so create → Work → Home can show the new card (plan: “cargo run can create a project and see it on home”).
2. **`list_endpoints` rather than a dedicated `count_endpoints`.** Home counts `Vec` length. Same Store call Task 11 needs.
3. **Port errors use `Notification::error`, not a dialog.** Plan allowed either.
4. **Popover/muted theme tokens added** so cards match scheme C paper, not default Light white.
5. **`parse_port` unit tests** in `new_project.rs` (no GPUI click automation, per spec §10).
6. **Empty lead sentence** from `docs/superpowers/ui/index.html` is included, not title+button only.
