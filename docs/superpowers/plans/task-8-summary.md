# Task 8 implementation summary

GPUI Kit app shell with Monet Paper theme and screen navigation. Commit: `7c8a6f4` (`feat: GPUI Kit shell with Monet paper theme`).

## Files changed

Created:

- `src/ui/mod.rs`
- `src/ui/theme.rs`
- `src/ui/app.rs`

Modified:

- `src/lib.rs` — `pub mod ui;`
- `src/main.rs` — tokio OnceLock, `apply_paper_theme` after `gpui_component::init`, `Root::new(AppView, …)`
- `Cargo.toml` — `directories = "5"`
- `Cargo.lock` — locked `directories 5.0.1`

Not created (deferred to Tasks 9–12): `home.rs`, `new_project.rs`, `settings.rs`, `workbench.rs`, `import_view.rs`. Body is a placeholder label per `Screen`.

## Theme

`apply_paper_theme(cx)` / `apply_ink_theme(cx)` — call after `gpui_component::init`.

Both `Theme::change` to Light/Dark first so leftover `ThemeColor` tokens are not the opposite appearance, then `Theme::global_mut(cx)` overlays Monet tokens via `gpui::rgb(0x…).into()` as `Hsla`.

Paper:

| Field | Hex |
|---|---|
| background | `#F2EBDC` |
| foreground | `#2A2620` |
| primary | `#3A5A40` |
| primary_foreground | `#FBF6EA` |
| primary_hover | primary `.darken(0.08)` |
| accent | `#A4453B` |
| border / input | `#D4C8B0` |
| list | background |
| list_active | primary `.opacity(0.16)` |
| danger | accent |
| title_bar / title_bar_border | background / border |

Ink: background `#1E1E1E`, foreground `#D4D4D4`, same primary/accent, border/input `#3A3A3A`, primary_hover `.lighten(0.08)`.

`title_bar*` is set because `TitleBar` paints those, not `background`.

## Shell

`Screen`: `Empty | Home | NewProject | Work { project_id } | Import { project_id } | Settings { back: Box<Screen> }`.

`AppView` holds `AppService` + current `Screen`.

- Store: `directories::ProjectDirs::from("", "", "Mocker")` → `~/Library/Application Support/Mocker/mocker.db` (`Store::open` creates the directory).
- Model: `Unconfigured` `ModelClient` returns `LlmError::Request("not configured")`.
- Initial screen: `Empty` if `list_projects` is empty, else `Home`.
- Title bar (`gpui-component` `TitleBar` + `Button`): `"Mocker"` + 「设置」. Click → `Screen::Settings { back: current }` (no nested Settings).
- Body: placeholder string for the current variant (`Empty`, `Home`, `Work {id}`, …).
- Window: `TitleBar::title_bar_options()` so the custom bar sits under macOS traffic lights.

Tokio: `OnceLock<Runtime>` in `main`, `.enter()` before `Application::new().run`. `RuntimeHub::new` then takes `Handle::try_current()` instead of owning a second runtime. `AppView::new` runs on that entered thread, then the entity is moved into `open_window`.

## Cargo results

Toolchain: `rustc 1.96.1`, `cargo 1.96.1`.

Locked (`cargo tree -p mocker --depth 1`):

| Crate | Locked version |
|---|---|
| directories | **5.0.1** |
| gpui-component | 0.5.1 |
| gpui | 0.2.2 |
| tokio | 1.53.1 |

### `cargo test --offline`

```
lib 0; main 0; import_test 6; runtime_test 7; scenes_test 9; service_test 2; sources_test 5; store_test 3
test result: ok
```

### `cargo build --offline`

```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.87s
```

`cargo fmt --check` PASS.

`cargo clippy --offline --all-targets -- -D warnings` PASS.

The `block v0.1.6` future-incompat warning is unchanged from Task 1 (GPUI/macOS dep).

`cargo run` was not executed (window would hang).

## API deviations from the plan

Plan snippet (`docs/superpowers/plans/2026-09-03-mocker-desktop.md` Task 8) vs what shipped:

1. **`apply_ink_theme` is implemented now** (plan listed paper; Task 10 toggles Paper/Ink). Same field set as paper with ink bg/fg.
2. **Settings button copy is 「设置」**, matching scheme C (`docs/superpowers/ui/index.html`), not the English word "Settings".
3. **Initial screen is Empty vs Home from `list_projects`**, not a hard-coded Home. Forms themselves are still placeholders.
4. **`directories = "5"`** added as specified; empty qualifier/org so macOS data dir is `Application Support/Mocker`, not a reverse-DNS bundle id.
5. **`TitleBar::title_bar_options()`** on the window so the Kit title bar is compatible with traffic lights. Task 1 used `WindowOptions::default()`.
6. **Did not implement home/workbench/import forms.**
