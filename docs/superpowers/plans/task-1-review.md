# Task 1 review — GPUI Kit crate skeleton

**Verdict:** Pass. Window is `Root`-wrapped, `gpui_component::init` is called before open, and the crate compiles.

**Scope:** Task 1 only (`Cargo.toml`, `src/main.rs`, `src/lib.rs`, `.gitignore`). Missing later-task crates (`axum`, `rusqlite`, …) and placeholder modules are in scope and accepted.

## Verification

| Check | Result |
|---|---|
| Binary crate `mocker` + lib crate `mocker` | Yes (`Cargo.toml` package name `mocker`, `src/main.rs` + `src/lib.rs`) |
| `gpui-component = "0.5.1"`, `gpui = "0.2"` → lock `0.2.2`, `anyhow = "1"` | Yes (`Cargo.lock`: gpui-component 0.5.1, gpui 0.2.2, anyhow 1.0.104) |
| `gpui_component::init(cx)` before window | Yes, `src/main.rs:22` |
| Window root is `Root::new(view, window, cx)` | Yes, `src/main.rs:26` |
| `ButtonVariants` in scope for `.primary()` | Yes, `src/main.rs:3` |
| `src/lib.rs` exports `version()` only | Yes |
| Commit | `8ac61b0` `chore: bootstrap GPUI Kit desktop crate` |
| `cargo test --lib --offline` | PASS (0 tests) |
| `cargo build --offline` | PASS |
| `cargo fmt --check` | PASS |
| Toolchain | `rustc 1.96.1` |

`src/main.rs` matches the GPUI Kit 0.5.1 getting-started shape (`Application::new().run`, `init`, async `open_window`, first-level `Root`). `gpui_platform` was not required.

Allowed deviations (not issues for this task):

- `Cargo.toml` omits later-task deps and `rust-version = "1.87"`.
- `.gitignore` also ignores `.playwright-mcp/` and `*.png`.
- `anyhow` is declared but unused in sources (plan/starter dep; official Kit example uses it for the spawn `Result`).

## Issues

No issues.
