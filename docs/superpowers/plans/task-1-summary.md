# Task 1 implementation summary

Crate skeleton that compiles a GPUI Kit window. Commit: `8ac61b0` (`chore: bootstrap GPUI Kit desktop crate`).

## Files changed

Created (and committed):

- `Cargo.toml`
- `Cargo.lock`
- `.gitignore`
- `src/lib.rs`
- `src/main.rs`

Not created (deferred to later tasks): domain/store/runtime/import/llm/scenes/service/sources modules.

## Versions

Declared in `Cargo.toml`:

```toml
gpui-component = "0.5.1"
gpui = "0.2"
anyhow = "1"
```

Locked (`cargo tree -p mocker --depth 1` / `Cargo.lock`):

| Crate | Locked version |
|---|---|
| gpui-component | **0.5.1** |
| gpui | **0.2.2** |
| anyhow | 1.0.104 |

`gpui-component 0.5.1` depends on `gpui = "0.2.2"`. `gpui = "0.2"` resolved cleanly to that version. `gpui_platform` was **not** required (`Application::new()` exists on `gpui` 0.2.2).

## API deviations from the plan

Plan snippet (`docs/superpowers/plans/2026-09-03-mocker-desktop.md` Task 1) vs what shipped:

1. **Cargo.toml deps are minimal.** The plan listed later-task crates (`axum`, `rusqlite`, `tokio`, `serde`, …) and `rust-version = "1.87"`. Task 1 instruction was to start with `gpui-component` + `anyhow`, then resolve `gpui`. Only those three runtime deps are present.

2. **`src/lib.rs` is version-only.** The plan first showed `pub mod domain;` etc., then said this task should only export `version()`. No placeholder modules.

3. **`Button::primary()` needs `ButtonVariants` in scope.** Plan `main.rs` used `Button::new("ok").primary().label("Let's Go!")` with `use gpui_component::{Root, button::Button, v_flex};`. That failed:

   ```
   error[E0599]: no method named `primary` found for struct `Button`
   help: trait `ButtonVariants` which provides `primary` is implemented but not in scope
   ```

   Fix: `use gpui_component::{button::{Button, ButtonVariants}, v_flex, Root};`.

4. **`Root::new` / `Application::new` matched the plan.** Registry `gpui-component` 0.5.1 README and `root.rs`:

   ```rust
   pub fn new(view: impl Into<AnyView>, window: &mut Window, cx: &mut Context<Self>) -> Self
   ```

   `gpui` 0.2.2: `Application::new()` then `.run(|cx| { ... })`. Did **not** switch to `gpui_platform::application()`.

5. **`.gitignore` extras.** Plan listed `/target` and `.DS_Store`. Also added `.playwright-mcp/` and `*.png` as allowed by the task instruction.

`cargo run` was not executed (window would hang).

## Cargo results

Toolchain: `rustc 1.96.1`, `cargo 1.96.1`. First GPUI compile ~4m 12s.

### `cargo test --lib`

First run (cold compile):

```
   Compiling mocker v0.1.0 (/Users/wangkeke/Desktop/mine/ai/grokwork/mocker)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 4m 12s
     Running unittests src/lib.rs (target/debug/deps/mocker-47516a607b965294)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

After the `ButtonVariants` import + `cargo fmt`:

```
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.07s
     Running unittests src/lib.rs (target/debug/deps/mocker-47516a607b965294)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### `cargo build`

First attempt failed on `Button::primary` (see deviation 3). After the import:

```
   Compiling mocker v0.1.0 (/Users/wangkeke/Desktop/mine/ai/grokwork/mocker)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.19s
```

Both commands printed:

```
warning: the following packages contain code that will be rejected by a future version of Rust: block v0.1.6
note: to see what the problems were, use the option `--future-incompat-report`, or run `cargo report future-incompatibilities --id 1`
```

This comes from a GPUI/macOS dependency, not from `mocker` sources.
