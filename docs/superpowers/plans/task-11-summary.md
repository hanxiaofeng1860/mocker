# Task 11 implementation summary

Workbench editor with live mock reload (scheme C). Commit: `1cc3787` (`feat: workbench editing with live mock reload`).

## Files changed

Created:

- `src/ui/workbench.rs` — work screen: list, editor, field tables, JSON, logs
- `docs/superpowers/plans/task-11-summary.md`

Modified:

- `src/ui/mod.rs` — `mod workbench;`
- `src/ui/app.rs` — `Screen::Work` body; `WorkbenchState`; drop the Work stub
- `src/service.rs` — get/save project, fields, scenes; create endpoint; regenerate; apply AI JSON; clear logs
- `src/store.rs` — `clear_logs`
- `src/import.rs` — `strip_markdown_fences` is `pub(crate)`
- `tests/service_test.rs` — create endpoint, regenerate from fields, apply generated success, clear logs
- `tests/store_test.rs` — `clear_logs` is per-project

Not created (deferred to Task 12): `import_view.rs`. 「导入」 navigates to `Screen::Import { project_id }` (still a placeholder).

## Screen

**工作台** (`Screen::Work { project_id }`):

Top: 「← 项目」, name, 运行中/已停止, `:{port}`, 启动/停止, live pill 「已写入 · 下一请求生效」, 导入, 项目设置.

- 启动/停止: `service.start` / `stop`. Port-in-use (and other bind errors) → `Notification::error` with the anyhow text (`bind 127.0.0.1:{port}: Address already in use`). Port is not changed.
- 项目设置: inline form (name, port, success/fail codes, comma-separated default header keys). 保存 → `save_project` (codes do not rewrite existing JSON). Invalid port → 「端口无效」.

Left: search, endpoint list (method badge + name + path last segment), 「+」 and 「+ 手工新建接口」.

Right: name `Input`, 启用 `Switch`, method `Select` (GET/POST/PUT/PATCH/DELETE), path `Input`; scene chips 成功/空数据/参数错误/业务失败 → `set_scene` then load that scene’s JSON.

- Header table: 英文名 / 中文名 / 类型 / 必填 / 说明 / 删 + 「+ 添加 header」
- Body table: same + 「+ 添加 body 字段」
- Response table: 路径 / 英文名 / 中文名 / 类型 / 删 + 「+ 添加字段」 + 「按字段重新生成」
- JSON textarea (`InputState::multi_line`) for the current scene; `save_scene_body` on `InputEvent::Change`. Invalid JSON is still saved; muted 「JSON 不合法，已按原文保存」 if `serde_json::from_str` fails.

Empty endpoints: CTA 「导入」 + 「手工新建」.

Bottom: request logs from `service.logs` (newest first), 500ms `Timer` while `WorkbenchState` lives. Columns: 时间 / 方法 / 路径 / 状态 / 场景. 「清空」 → `clear_logs`.

## Live apply

On edit, UI calls `save_endpoint` / `save_scene_body` / `set_scene` / `save_fields`. Store writes SQLite; `save_endpoint` / `save_scene_body` / `set_scene` / `save_project` refresh the running snapshot (no port restart). `save_fields` does not rewrite JSON or refresh routes (snapshot reads scenes).

Duplicate method+path → 「同一项目下 method + path 不能重复」.

## Regenerators

「按字段重新生成该场景」: confirm dialog, then `regenerate_scene_from_fields`.

- success: envelope + `data` from top-level response fields (`Integer`→0, `Boolean`→false, `Array`→[], else `""`)
- empty / param_error / business_error: `generate_non_success` from the current success body

「用 AI 重新生成成功数据」: `HttpModelClient` from settings (same as 测试连通), `import_prompt(source_text or field paste)` on `background_spawn` (does not block the UI). Result → `apply_generated_success` (import JSON first draft, or a raw JSON object). LLM / parse errors → `Notification::error`. No source selected → 「去设置里选择模型来源」.

## AppService

```
get_project / get_endpoint / list_fields / get_scene
save_project(Project)           // upsert + refresh if running
clear_logs(project_id)
create_endpoint(project_id)     // POST /untitled, 新接口, 4 scenes
apply_generated_success(id, raw)
regenerate_scene_from_fields(id, kind)
```

`create_endpoint` paths: `/untitled`, `/untitled-2`, …

## Cargo results

Toolchain: `rustc 1.96.1`, `cargo 1.96.1`. No new crates. Locked (`cargo tree -p mocker --depth 1`) unchanged from Task 10 (`gpui-component 0.5.1`, `gpui 0.2.2`, `tokio 1.53.1`, `directories 5.0.1`, …).

### `cargo test --offline`

```
lib 11 (parse_port + settings helpers + workbench helpers); main 0; import_test 6; runtime_test 7; scenes_test 9; service_test 8; sources_test 6; store_test 4
test result: ok
```

### `cargo build --offline`

```
Finished `dev` profile [unoptimized + debuginfo] target(s) in ~9s
```

`cargo fmt --check` PASS.

Manual check (start → curl → edit JSON → curl without restart) not run here (`cargo run` was out of scope).
