# Mocker Desktop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a macOS GPUI Kit desktop app that turns pasted company API docs into a local mock server: projects on their own ports, editable endpoints, switchable scenes, live hot-reload.

**Architecture:** One Rust process. SQLite for projects/endpoints/scenes/logs. In-process axum listeners (one per running project) read an `ArcSwap` route snapshot. UI is scheme C (project home → workbench). GPUI Kit (`gpui-component`) draws the window; Monet Paper tokens override the theme. LLM import and local-source scanning sit behind a `ModelClient` trait so tests inject fakes.

**Tech Stack:** Rust 2021, `gpui-component` 0.5.1 (https://gpui-kit.com/zh-CN/), gpui (resolved by that crate), axum, tokio, rusqlite (bundled), serde_json, reqwest, toml, uuid, chrono, arc-swap, thiserror.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-09-03-mocker-desktop-design.md`
- UI: scheme C in `docs/superpowers/ui/index.html` — not Dock, not scheme A/B
- Bind mock servers to `127.0.0.1` only; never auto-change a busy port
- Success/fail codes are project-level strings; JSON number iff `^-?(0|[1-9][0-9]*)$`
- Four scenes per endpoint: success / empty / param_error / business_error
- Scan-source secrets never written to SQLite (`manual` key is the exception)
- Paper theme defaults: background `#F2EBDC`, primary `#3A5A40`, accent `#A4453B`
- No PDF import, no request-body branching, no auth, no delay simulation
- Tests for store/runtime/import/sources/scenes; no GPUI click automation
- macOS first; data file `~/Library/Application Support/Mocker/mocker.db`

## File map

- Create: `Cargo.toml`, `.gitignore`
- Create: `src/main.rs` — process entry, gpui init
- Create: `src/lib.rs` — `mocker` lib so tests import domain/store/runtime
- Create: `src/domain.rs` — types
- Create: `src/store.rs` — SQLite
- Create: `src/scenes.rs` — envelope + empty/error generation + code encoding
- Create: `src/runtime.rs` — axum + snapshot
- Create: `src/sources.rs` — local model source scan
- Create: `src/llm.rs` — Anthropic / OpenAI-compatible HTTP
- Create: `src/import.rs` — validate model JSON, attach scenes
- Create: `src/service.rs` — use cases (create project, start, edit, import)
- Create: `src/ui/mod.rs`, `theme.rs`, `app.rs`, `home.rs`, `new_project.rs`, `settings.rs`, `workbench.rs`, `import_view.rs`
- Create: `tests/scenes_test.rs`, `tests/store_test.rs`, `tests/runtime_test.rs`, `tests/sources_test.rs`, `tests/import_test.rs`

---

### Task 1: Crate skeleton that compiles a GPUI Kit window

**Files:**
- Create: `Cargo.toml`
- Create: `.gitignore`
- Create: `src/main.rs`
- Create: `src/lib.rs`

**Interfaces:**
- Consumes: nothing
- Produces: binary `mocker` that opens a window; lib crate name `mocker`

- [ ] **Step 1: Write Cargo.toml and gitignore**

```toml
[package]
name = "mocker"
version = "0.1.0"
edition = "2021"
rust-version = "1.87"

[dependencies]
gpui-component = "0.5.1"
gpui = "0.2"
anyhow = "1"
arc-swap = "1"
axum = "0.8"
chrono = { version = "0.4", features = ["clock", "serde"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
rusqlite = { version = "0.32", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "sync"] }
toml = "0.8"
uuid = { version = "1", features = ["v4"] }
directories = "5"

[dev-dependencies]
tempfile = "3"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

If `gpui = "0.2"` fails to resolve against `gpui-component 0.5.1`, drop the explicit `gpui` line and `use` whatever `gpui-component` re-exports. If the compiler asks for `gpui_platform`, add:

```toml
gpui_platform = { version = "0.2", features = ["font-kit"] }
```

`.gitignore`:

```
/target
.DS_Store
```

- [ ] **Step 2: Write lib.rs and a Root-wrapped window**

`src/lib.rs`:

```rust
pub mod domain;
pub mod import;
pub mod llm;
pub mod runtime;
pub mod scenes;
pub mod service;
pub mod sources;
pub mod store;
```

Leave those modules as empty `pub struct _Placeholder;` until later tasks, **or** comment them out and only declare modules that exist. For this task only create:

```rust
// src/lib.rs
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
```

`src/main.rs` — follow GPUI Kit getting started. After `cargo fetch`, match the crate's current `Root::new` / `init` signatures. Expected shape:

```rust
use gpui::*;
use gpui_component::{Root, button::Button, v_flex};

struct Hello;

impl Render for Hello {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .child("Mocker")
            .child(Button::new("ok").primary().label("Let's Go!"))
    }
}

fn main() {
    Application::new().run(|cx| {
        gpui_component::init(cx);
        cx.spawn(async move |cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let view = cx.new(|_| Hello);
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("open window");
        })
        .detach();
    });
}
```

If `Application::new` is `gpui_platform::application()`, use that and `.with_assets` only if the crate requires it.

- [ ] **Step 3: Compile**

Run: `cargo test --lib`  
Expected: PASS (no tests yet, lib compiles)

Run: `cargo build`  
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock .gitignore src/main.rs src/lib.rs
git commit -m "chore: bootstrap GPUI Kit desktop crate"
```

---

### Task 2: Domain types and scene JSON helpers

**Files:**
- Create: `src/domain.rs`
- Create: `src/scenes.rs`
- Create: `tests/scenes_test.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: nothing
- Produces: `domain::{Project, Endpoint, Field, Scene, SceneKind, FieldLoc, GlobalSettings, RequestLog, HeaderKv}`; `scenes::{encode_code, scene_http_status, generate_non_success, default_success_envelope}`

- [ ] **Step 1: Write failing tests**

```rust
// tests/scenes_test.rs
use mocker::scenes::{encode_code, generate_non_success};
use mocker::domain::SceneKind;
use serde_json::json;

#[test]
fn encode_leading_zeros_as_string() {
    assert_eq!(encode_code("0000"), json!("0000"));
    assert_eq!(encode_code("200"), json!(200));
    assert_eq!(encode_code("0"), json!(0));
}

#[test]
fn empty_list_scene_uses_success_code() {
    let success = json!({"code":"0000","msg":"成功","data":{"list":[{"id":"1"}],"total":2,"extra":1}});
    let empty = generate_non_success(SceneKind::Empty, "0000", "9999", &success);
    assert_eq!(empty["code"], "0000");
    assert_eq!(empty["data"]["list"], json!([]));
    assert_eq!(empty["data"]["total"], 0);
}

#[test]
fn param_error_is_object_with_fail_code() {
    let body = generate_non_success(SceneKind::ParamError, "0000", "9999", &json!({}));
    assert_eq!(body["code"], "9999");
    assert_eq!(body["msg"], "参数错误");
    assert_eq!(body["data"], json!(null));
}
```

- [ ] **Step 2: Run tests — expect FAIL** (modules missing)

Run: `cargo test --test scenes_test`  
Expected: compile fail `could not find scenes in crate mocker`

- [ ] **Step 3: Implement domain + scenes**

`src/domain.rs` — include at least:

```rust
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HeaderKv { pub key: String, pub value: String }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub port: u16,
    pub success_code: String,
    pub fail_code: String,
    pub default_headers: Vec<HeaderKv>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneKind { Success, Empty, ParamError, BusinessError }

impl SceneKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Empty => "empty",
            Self::ParamError => "param_error",
            Self::BusinessError => "business_error",
        }
    }
    pub fn all() -> [SceneKind; 4] {
        [Self::Success, Self::Empty, Self::ParamError, Self::BusinessError]
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Endpoint {
    pub id: String,
    pub project_id: String,
    pub method: String,
    pub path: String,
    pub name: String,
    pub notes: String,
    pub source_text: String,
    pub deprecated: bool,
    pub enabled: bool,
    pub current_scene: SceneKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldLoc { Header, Body, Response }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Field {
    pub id: String,
    pub endpoint_id: String,
    pub location: FieldLoc,
    pub name: String,
    pub name_zh: String,
    pub type_name: String,
    pub required: bool,
    pub comment: String,
    pub enum_values: Vec<String>,
    pub parent_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scene {
    pub endpoint_id: String,
    pub kind: SceneKind,
    pub http_status: u16,
    pub body_json: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct GlobalSettings {
    pub selected_source_id: String,
    pub selected_model: String,
    pub manual_base_url: String,
    pub manual_api_key: String,
    pub manual_protocol: String,
    pub manual_model: String,
}

#[derive(Clone, Debug)]
pub struct RequestLog {
    pub id: i64,
    pub project_id: String,
    pub at: String,
    pub method: String,
    pub url: String,
    pub req_headers: String,
    pub req_body: String,
    pub hit: bool,
    pub endpoint_id: Option<String>,
    pub scene: Option<String>,
    pub status: u16,
    pub res_body: String,
    pub elapsed_ms: u128,
    pub missing_default_headers: Vec<String>,
}
```

`src/scenes.rs`:

```rust
use serde_json::{json, Value};
use crate::domain::SceneKind;

pub fn encode_code(code: &str) -> Value {
    if regex_simple_int(code) {
        Value::Number(code.parse::<i64>().unwrap().into())
    } else {
        Value::String(code.to_string())
    }
}

fn regex_simple_int(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() { return false; }
    let mut i = 0;
    if b[0] == b'-' { i = 1; if b.len() == 1 { return false; } }
    if b[i] == b'0' { return b.len() == i + 1; }
    b[i..].iter().all(|c| c.is_ascii_digit())
}

pub fn scene_http_status(kind: SceneKind) -> u16 {
    match kind {
        SceneKind::ParamError => 400,
        _ => 200,
    }
}

pub fn default_success_envelope(success_code: &str) -> Value {
    json!({ "code": encode_code(success_code), "msg": "成功", "data": {} })
}

pub fn generate_non_success(kind: SceneKind, success_code: &str, fail_code: &str, success_body: &Value) -> Value {
    match kind {
        SceneKind::Success => success_body.clone(),
        SceneKind::ParamError => json!({
            "code": encode_code(fail_code), "msg": "参数错误", "data": Value::Null
        }),
        SceneKind::BusinessError => json!({
            "code": encode_code(fail_code), "msg": "失败", "data": Value::Null
        }),
        SceneKind::Empty => empty_from_success(success_code, success_body),
    }
}

fn empty_from_success(success_code: &str, success_body: &Value) -> Value {
    let data = success_body.get("data").cloned().unwrap_or(json!({}));
    let data = match data {
        Value::Array(_) => json!([]),
        Value::Object(mut m) => {
            if m.contains_key("list") { m.insert("list".into(), json!([])); }
            if m.contains_key("total") { m.insert("total".into(), json!(0)); }
            Value::Object(m)
        }
        _ => json!({}),
    };
    json!({ "code": encode_code(success_code), "msg": "成功", "data": data })
}
```

Export modules from `src/lib.rs`.

- [ ] **Step 4: Run tests — expect PASS**

Run: `cargo test --test scenes_test`  
Expected: 3 passed

- [ ] **Step 5: Commit**

```bash
git add src/lib.rs src/domain.rs src/scenes.rs tests/scenes_test.rs
git commit -m "feat: domain types and scene JSON helpers"
```

---

### Task 3: SQLite store

**Files:**
- Create: `src/store.rs`
- Create: `tests/store_test.rs`

**Interfaces:**
- Consumes: `domain::*`
- Produces: `Store::{open, open_in_memory, upsert_project, list_projects, get_project, delete_project, upsert_endpoint, list_endpoints, get_endpoint, delete_endpoint, replace_fields, list_fields, upsert_scene, get_scene, set_current_scene, load_settings, save_settings, append_log, list_logs, trim_logs}`

- [ ] **Step 1: Write failing tests** using `tempfile` and `Store::open(path)`

Cover: insert project + endpoint + 4 scenes; list; change `current_scene`; settings round-trip does **not** store scan secrets (only `manual_api_key`); logs cap at 500 by deleting oldest.

- [ ] **Step 2: Run — FAIL** missing `store`

- [ ] **Step 3: Implement `store.rs`**

- Schema: `projects`, `endpoints` unique `(project_id, method, path)`, `fields`, `scenes` unique `(endpoint_id, kind)`, `settings` single row id=1, `request_logs`
- IDs: `uuid::Uuid::new_v4().to_string()`
- `default_headers` stored as JSON text
- `trim_logs(project_id)` after append if count > 500

- [ ] **Step 4: `cargo test --test store_test` PASS**

- [ ] **Step 5: Commit** `feat: sqlite persistence for projects and mocks`

---

### Task 4: Mock HTTP runtime

**Files:**
- Create: `src/runtime.rs`
- Create: `tests/runtime_test.rs`

**Interfaces:**
- Consumes: `Store`, `domain`, `scenes::encode_code`
- Produces: `RuntimeHub::{new, start(project_id), stop(project_id), refresh(project_id), is_running}`  
  Snapshot type `RouteSnapshot { fail_code, default_header_keys, routes: HashMap<(String,String), Route> }` where `Route { status, body, scene, endpoint_id, enabled }`

- [ ] **Step 1: Tests with `tokio::test` + `reqwest` against `127.0.0.1:0` or an ephemeral port from `TcpListener::bind("127.0.0.1:0")`**

Cases:
1. POST exact path → success JSON
2. switch scene in store + `refresh` → next request new body (no restart)
3. unknown path → 404 envelope with fail_code
4. GET on POST path → 405
5. OPTIONS → 204, CORS headers `Access-Control-Allow-Origin: *`, `Allow-Headers: *`, `Allow-Methods: GET, POST, PUT, PATCH, DELETE, OPTIONS`
6. missing `sn` header still 200; log `missing_default_headers` contains `sn`
7. second `start` on same port while occupied → error, port unchanged

- [ ] **Step 2: FAIL missing runtime**

- [ ] **Step 3: Implement**

- Tokio runtime: either a global `OnceLock<tokio::runtime::Handle>` started in `main` before gpui, or `tokio::runtime::Runtime` owned by `RuntimeHub`
- Axum router: fallback handler that looks up method+path
- CORS on every response
- Bind `127.0.0.1:{port}` only
- On each request append `RequestLog` via `Store` (mutex around rusqlite)
- `refresh` rebuilds `ArcSwap<RouteSnapshot>` from store (enabled endpoints only)

- [ ] **Step 4: tests PASS**

- [ ] **Step 5: Commit** `feat: in-process mock HTTP runtime with hot reload`

---

### Task 5: Local model source scan

**Files:**
- Create: `src/sources.rs`
- Create: `tests/sources_test.rs`

**Interfaces:**
- Consumes: filesystem paths (injectable home dir)
- Produces: `scan_sources(home: &Path) -> Vec<ModelSource>`  
  `ModelSource { id, label, protocol: AnthropicMessages | OpenAIChat, base_url, model, available: bool, reason: String, credential_from: CredentialFrom }`  
  `CredentialFrom::File { path, kind }` — never put the secret in `ModelSource`

- [ ] **Step 1: Tests with a temp HOME**

Fixtures:
- `settings.json` with `env.ANTHROPIC_BASE_URL` + `ANTHROPIC_AUTH_TOKEN` → available `claude-code`
- Pi `models.json` provider with `baseUrl`+`apiKey` → `pi:{name}`
- Codex `config.toml` `[model_providers.x] base_url = "http://127.0.0.1:9"` and port closed → listed unavailable
- Grok `config.toml` model with `base_url`+`api_key` → available
- Serialize `GlobalSettings` after selecting claude-code → no token string in JSON

- [ ] **Step 2: FAIL**

- [ ] **Step 3: Implement parsers** (serde_json / toml). TCP probe only for localhost URLs.

- [ ] **Step 4: PASS**

- [ ] **Step 5: Commit** `feat: scan Claude Code, Pi, Codex, Grok model configs`

---

### Task 6: LLM client + import validation

**Files:**
- Create: `src/llm.rs`
- Create: `src/import.rs`
- Create: `tests/import_test.rs`

**Interfaces:**
- Consumes: `ModelSource` + live credential read; `scenes::generate_non_success`
- Produces:  
  `trait ModelClient { fn complete_json(&self, prompt: &str) -> Result<String, LlmError>; }`  
  `validate_import(raw: &str) -> Result<Vec<ImportDraft>, ImportError>`  
  `ImportDraft { name, path, method, deprecated, notes, request_headers: Vec<Field>, request_body_fields, response_fields, success_body: Value }`  
  `fn attach_scenes(draft: &ImportDraft, success_code, fail_code) -> [Scene; 4]`  
  `fn import_prompt(paste: &str, success_code: &str, fail_code: &str) -> String`

- [ ] **Step 1: Tests**

- Valid model JSON with one endpoint and `success_body` object → one draft
- Missing path / empty endpoints / non-JSON → error, no drafts
- `attach_scenes` produces 4 kinds and empty list rule
- Fake `ModelClient` returns fixture JSON; `run_import(paste)` uses it

- [ ] **Step 2: FAIL**

- [ ] **Step 3: Implement**

Prompt (hard-coded, Chinese+JSON schema from spec §6). Strip markdown fences if the model wraps ` ```json `.

`llm.rs`: POST `{base}/v1/messages` for Anthropic (header `x-api-key`, `anthropic-version: 2023-06-01`); POST `{base}/chat/completions` for OpenAI. Normalize trailing `/v1`. Read credential from file at call time.

- [ ] **Step 4: PASS**

- [ ] **Step 5: Commit** `feat: import validation and LLM JSON client`

---

### Task 7: Application service

**Files:**
- Create: `src/service.rs`
- Create: `tests/service_test.rs`

**Interfaces:**
- Consumes: `Store`, `RuntimeHub`, `ModelClient`
- Produces: `AppService` methods used by UI:

```rust
impl AppService {
    pub fn list_projects(&self) -> Result<Vec<Project>>;
    pub fn create_project(&self, name, port, success_code, fail_code, headers) -> Result<Project>;
    pub fn start(&self, project_id: &str) -> Result<()>;
    pub fn stop(&self, project_id: &str) -> Result<()>;
    pub fn save_endpoint(&self, ep: Endpoint) -> Result<()>; // also refresh if running
    pub fn save_scene_body(&self, endpoint_id, kind, body: String) -> Result<()>;
    pub fn set_scene(&self, endpoint_id, kind) -> Result<()>;
    pub fn save_fields(&self, endpoint_id, fields: Vec<Field>) -> Result<()>;
    pub fn import_paste(&self, project_id, paste: &str) -> Result<Vec<ImportDraft>>; // LLM
    pub fn commit_import(&self, project_id, drafts: Vec<ImportDraft>) -> Result<()>;
    pub fn logs(&self, project_id) -> Result<Vec<RequestLog>>;
}
```

Every mutating call that affects routes must `runtime.refresh` when the project is running.

- [ ] **Step 1: Test create → start → reqwest → change JSON → reqwest sees new body**

- [ ] **Step 2: FAIL**

- [ ] **Step 3: Implement** (hold `Store` in `Arc<Mutex<Store>>` because rusqlite is not Sync)

- [ ] **Step 4: PASS**

- [ ] **Step 5: Commit** `feat: app service wiring store, runtime, and import`

---

### Task 8: Paper theme + app shell + navigation

**Files:**
- Create: `src/ui/mod.rs`, `src/ui/theme.rs`, `src/ui/app.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `AppService`
- Produces: `Screen` enum and `AppView` that switches screens; `apply_paper_theme(cx)`

```rust
pub enum Screen {
    Empty,
    Home,
    NewProject,
    Work { project_id: String },
    Import { project_id: String },
    Settings { back: Box<Screen> },
}
```

- [ ] **Step 1: No automated UI test.** Manually `cargo run` must show a Paper-colored window (beige, not default white/blue).

- [ ] **Step 2: Implement `apply_paper_theme`** after `gpui_component::init`

Map at least: background `#F2EBDC`, foreground `#2A2620`, primary `#3A5A40`, accent `#A4453B`, border a warm gray. Use whatever `Theme` / `cx.theme()` fields 0.5.1 exposes (`background`, `foreground`, `primary`, `border`, `accent` or equivalents). Ink mapping: background `#1e1e1e`, foreground `#d4d4d4`.

- [ ] **Step 3: `AppView` title bar:** `Mocker` + Settings button. Body: placeholder text per `Screen`. Settings button sets `Screen::Settings { back }`.

- [ ] **Step 4: `cargo run` shows window; `cargo test` still green**

- [ ] **Step 5: Commit** `feat: GPUI Kit shell with Monet paper theme`

---

### Task 9: Empty, home, new project screens

**Files:**
- Create: `src/ui/home.rs`, `src/ui/new_project.rs`
- Modify: `src/ui/app.rs`

Match `docs/superpowers/ui/index.html` screens `empty`, `home`, `new`.

- [ ] **Step 1: Home lists projects from `AppService::list_projects`.** Running section uses `runtime.is_running`. Cards show name, port, endpoint count, badge 运行中/已停止. Header button 「新建项目」 is a primary `Button`, not a dashed card.

- [ ] **Step 2: Empty state when list is empty:** copy 「还没有项目」 + primary 「新建项目」.

- [ ] **Step 3: New project form:** name, port, success_code default 0000, fail_code 9999, optional header key. Submit calls `create_project` then `Screen::Work`. Port parse errors via `window.push_notification` / dialog, do not bind.

- [ ] **Step 4: `cargo test` PASS; `cargo run` can create a project and see it on home**

- [ ] **Step 5: Commit** `feat: project home and new-project form`

---

### Task 10: Settings screen (model sources)

**Files:**
- Create: `src/ui/settings.rs`
- Modify: `src/ui/app.rs`

- [ ] **Step 1: Render scan results as radio rows** (available selectable, unavailable disabled with reason). Buttons: 刷新扫描, 测试连通, 手动填写 (shows base_url / key / protocol / model inputs).

- [ ] **Step 2: Persist `GlobalSettings.selected_source_id` on change.** Paper / Ink toggles `apply_paper_theme` / ink.

- [ ] **Step 3: 测试连通 uses `ModelClient` with a one-token prompt; toast success or error.**

- [ ] **Step 4: Commit** `feat: settings page for model sources and appearance`

---

### Task 11: Workbench editor, live apply, logs

**Files:**
- Create: `src/ui/workbench.rs`
- Modify: `src/ui/app.rs`

- [ ] **Step 1: Layout** left list + right editor + bottom log. Top: back, name, running badge, port, start/stop, import, project settings, live pill 「已写入 · 下一请求生效」.

- [ ] **Step 2: Bind InputState for name, path; Select for method; Switch for enabled.** On `InputEvent::Change` / click, call `save_endpoint` (service refreshes runtime).

- [ ] **Step 3: Scene chips call `set_scene`.** JSON `TextareaState` on change → `save_scene_body`. Invalid JSON still saves; show muted 「JSON 不合法，已按原文保存」.

- [ ] **Step 4: Header/Body/Response tables:** rows of inputs + delete + add. Saving fields does not by itself rewrite JSON unless user clicks 「按字段重新生成」 (confirm dialog). 「用 AI 重新生成成功数据」 calls LLM with `source_text` or field JSON.

- [ ] **Step 5: Log table from `service.logs`, refresh on a timer (e.g. 500ms) while screen is Work.** Empty endpoints: CTA 导入 + 手工新建.

- [ ] **Step 6: Manual check:** start, curl path, edit JSON, curl again without restart.

- [ ] **Step 7: Commit** `feat: workbench editing with live mock reload`

---

### Task 12: Import screen

**Files:**
- Create: `src/ui/import_view.rs`
- Modify: `src/ui/app.rs`

- [ ] **Step 1: Textarea + 解析 button.** If no source selected, dialog 「去设置里选择模型来源」 and jump to Settings with back = Import.

- [ ] **Step 2: Show drafts as checkbox table (method, path, name).** Duplicate/empty path rows marked, confirm disabled. 写入 calls `commit_import` then Work.

- [ ] **Step 3: Cancel in-flight: drop/abort reqwest via `tokio::select!` + CancellationToken.**

- [ ] **Step 4: Commit** `feat: paste import with preview`

---

### Task 13: Spec acceptance

**Files:** none new except optional `tests/acceptance_http.rs` using store+service without UI.

- [ ] **Step 1: Automated:** two projects two ports; import fixture JSON (no live LLM) for `queryPhoneHomeData` + a list endpoint; curl success/empty/404; header sn missing only logged.

- [ ] **Step 2: Manual:** paste text from `设计文档v1.0.0.0.pdf` (home overview, call record list, modify with header sn) through the UI with a real scanned source.

- [ ] **Step 3: Commit** `test: HTTP acceptance for two projects and scene switch`

---

## Self-review

| Spec section | Task |
|---|---|
| Project / codes / headers | 2, 3, 9 |
| Scenes + encode_code | 2 |
| Runtime match/CORS/404/405/hot reload/logs | 4, 7, 11 |
| Source scan + no secret persist | 5, 10 |
| LLM import + preview | 6, 12 |
| Scheme C pages | 8–12 |
| Paper theme | 8, 10 |
| GPUI Kit | 1, 8 |

No PDF, no Dock, no request-body branching.
