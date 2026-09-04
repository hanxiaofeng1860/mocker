use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    dialog::DialogButtonProps,
    h_flex,
    input::{Input, InputState},
    notification::Notification,
    tag::Tag,
    v_flex, ActiveTheme as _, Disableable as _, Sizable as _, StyledExt as _, WindowExt as _,
};

use crate::import::ImportDraft;
use crate::service::AppService;
use crate::sources::{home_dir, scan_sources};

use super::app::AppView;
use super::settings::client_for_selection;
use super::style;

pub(super) struct ImportState {
    pub(super) project_id: String,
    paste: Entity<InputState>,
    drafts: Vec<ImportDraft>,
    selected: Vec<bool>,
    existing: Vec<(String, String)>,
    parsing: bool,
    error: Option<String>,
    ticket: Arc<AtomicU64>,
}

impl ImportState {
    pub(super) fn new(project_id: String, window: &mut Window, cx: &mut Context<AppView>) -> Self {
        Self {
            project_id,
            paste: cx.new(|cx| {
                InputState::new(window, cx)
                    .multi_line(true)
                    .rows(10)
                    .placeholder("粘贴公司设计文档风格的接口说明")
            }),
            drafts: Vec::new(),
            selected: Vec::new(),
            existing: Vec::new(),
            parsing: false,
            error: None,
            ticket: Arc::new(AtomicU64::new(0)),
        }
    }
}

pub(super) fn view(
    state: &ImportState,
    service: &AppService,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let parsing = state.parsing;
    let can_write = commit_enabled(&state.drafts, &state.selected, &state.existing);
    let preview_id = state.ticket.load(Ordering::SeqCst);
    v_flex()
        .size_full()
        .gap_3()
        .child(
            h_flex()
                .flex_shrink_0()
                .gap_3()
                .child(
                    Button::new("back-import")
                        .ghost()
                        .small()
                        .label("← 工作台")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.close_import();
                            cx.notify();
                        })),
                )
                .child(div().text_xl().font_semibold().child("导入接口")),
        )
        .child(
            Input::new(&state.paste)
                .h(px(160.))
                .w_full()
                .flex_shrink_0(),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(source_caption(service)),
        )
        .child(
            h_flex()
                .flex_shrink_0()
                .gap_2()
                .child(
                    Button::new("import-parse")
                        .primary()
                        .small()
                        .label("解析")
                        .loading(parsing)
                        .disabled(parsing)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.start_import_parse(window, cx);
                            cx.notify();
                        })),
                )
                .when(parsing, |this| {
                    this.child(
                        Button::new("import-abort")
                            .small()
                            .label("取消解析")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.abort_import_parse();
                                cx.notify();
                            })),
                    )
                })
                .child(
                    Button::new("import-cancel")
                        .ghost()
                        .small()
                        .label("取消")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.close_import();
                            cx.notify();
                        })),
                )
                .when(!state.drafts.is_empty(), |this| {
                    this.child(
                        Button::new("import-write")
                            .primary()
                            .small()
                            .label("写入")
                            .disabled(!can_write || parsing)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.commit_import_preview(window, cx);
                                cx.notify();
                            })),
                    )
                }),
        )
        .when(parsing, |this| {
            this.child(
                div()
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("解析中…"),
            )
        })
        .when_some(state.error.clone(), |this, err| {
            this.child(style::appear(
                "import-error",
                div()
                    .w_full()
                    .p_3()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(cx.theme().danger)
                    .bg(cx.theme().danger.opacity(0.08))
                    .text_color(cx.theme().danger)
                    .child(err),
            ))
        })
        .when(!state.drafts.is_empty(), |this| {
            this.child(div().flex_1().min_h_0().child(style::appear(
                format!("import-preview-{preview_id}"),
                preview_table(state, cx),
            )))
        })
        .into_any_element()
}

fn preview_table(state: &ImportState, cx: &mut Context<AppView>) -> impl IntoElement {
    style::card(cx)
        .w_full()
        .flex_1()
        .min_h_0()
        .child(
            h_flex()
                .w_full()
                .px_3()
                .py_2()
                .gap_3()
                .border_b_1()
                .border_color(cx.theme().border)
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(div().w(px(28.)))
                .child(div().w(px(72.)).child("方法"))
                .child(div().flex_1().child("路径"))
                .child(div().w(px(160.)).child("名称")),
        )
        .children(
            state
                .drafts
                .iter()
                .enumerate()
                .map(|(i, draft)| preview_row(i, draft, state, cx)),
        )
}

fn preview_row(
    index: usize,
    draft: &ImportDraft,
    state: &ImportState,
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    let issue = row_issue(index, &state.drafts, &state.selected, &state.existing);
    let checked = state.selected.get(index).copied().unwrap_or(false);
    let marked = issue.is_some();
    let name = if draft.name.trim().is_empty() {
        "未命名".to_string()
    } else {
        draft.name.clone()
    };
    h_flex()
        .w_full()
        .px_3()
        .py_2()
        .gap_3()
        .items_center()
        .border_b_1()
        .border_color(cx.theme().border)
        .when(marked, |this| this.bg(cx.theme().danger.opacity(0.08)))
        .child(
            Checkbox::new(SharedString::from(format!("import-row-{index}")))
                .checked(checked)
                .on_click(cx.listener(move |this, checked, _, cx| {
                    this.toggle_import_row(index, *checked);
                    cx.notify();
                })),
        )
        .child(div().w(px(72.)).child(method_badge(&draft.method)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .font_family(cx.theme().mono_font_family.clone())
                .when(marked, |this| this.text_color(cx.theme().danger))
                .child(draft.path.clone()),
        )
        .child(
            v_flex()
                .w(px(160.))
                .child(
                    div()
                        .when(marked, |this| this.text_color(cx.theme().danger))
                        .child(name),
                )
                .when_some(issue, |this, issue| {
                    this.child(div().text_xs().text_color(cx.theme().danger).child(issue))
                }),
        )
}

fn method_badge(method: &str) -> impl IntoElement {
    let tag = match method.to_ascii_uppercase().as_str() {
        "GET" => Tag::success(),
        "DELETE" => Tag::danger(),
        "PUT" | "PATCH" => Tag::warning(),
        _ => Tag::info(),
    };
    tag.small().child(method.to_ascii_uppercase())
}

fn source_caption(service: &AppService) -> String {
    let settings = service.load_settings().unwrap_or_default();
    if settings.selected_source_id.is_empty() {
        return "尚未选择模型来源，解析前请先到设置里选择。".into();
    }
    if settings.selected_source_id == "manual" {
        let name = if settings.manual_model.trim().is_empty() {
            "手动填写"
        } else {
            settings.manual_model.trim()
        };
        return format!("将使用设置里的模型来源（当前 {name}）。");
    }
    match scan_sources(&home_dir())
        .into_iter()
        .find(|s| s.id == settings.selected_source_id)
    {
        Some(source) => format!("将使用设置里的模型来源（当前 {}）。", source.label),
        None => "将使用设置里的模型来源。".into(),
    }
}

fn model_source_ready(service: &AppService) -> bool {
    let Ok(settings) = service.load_settings() else {
        return false;
    };
    if settings.selected_source_id.is_empty() {
        return false;
    }
    client_for_selection(&settings, &scan_sources(&home_dir())).is_some()
}

pub(crate) fn row_issue(
    index: usize,
    drafts: &[ImportDraft],
    selected: &[bool],
    existing: &[(String, String)],
) -> Option<&'static str> {
    let draft = drafts.get(index)?;
    let path = draft.path.trim();
    if path.is_empty() {
        return Some("路径为空");
    }
    let method = draft.method.trim();
    if existing
        .iter()
        .any(|(m, p)| m.eq_ignore_ascii_case(method) && p == path)
    {
        return Some("与项目内重复");
    }
    if selected.get(index).copied().unwrap_or(false) {
        let dup = drafts.iter().enumerate().any(|(j, other)| {
            j != index
                && selected.get(j).copied().unwrap_or(false)
                && other.method.trim().eq_ignore_ascii_case(method)
                && other.path.trim() == path
        });
        if dup {
            return Some("与本批重复");
        }
    }
    None
}

pub(crate) fn default_selected(drafts: &[ImportDraft], existing: &[(String, String)]) -> Vec<bool> {
    let mut selected = vec![false; drafts.len()];
    let mut taken: Vec<(String, String)> = Vec::new();
    for (i, draft) in drafts.iter().enumerate() {
        let path = draft.path.trim();
        if path.is_empty() {
            continue;
        }
        let method = draft.method.trim();
        if existing
            .iter()
            .any(|(m, p)| m.eq_ignore_ascii_case(method) && p == path)
        {
            continue;
        }
        if taken
            .iter()
            .any(|(m, p)| m.eq_ignore_ascii_case(method) && p == path)
        {
            continue;
        }
        taken.push((method.to_string(), path.to_string()));
        selected[i] = true;
    }
    selected
}

pub(crate) fn commit_enabled(
    drafts: &[ImportDraft],
    selected: &[bool],
    existing: &[(String, String)],
) -> bool {
    let mut any = false;
    for i in 0..drafts.len() {
        if !selected.get(i).copied().unwrap_or(false) {
            continue;
        }
        if row_issue(i, drafts, selected, existing).is_some() {
            return false;
        }
        any = true;
    }
    any
}

impl AppView {
    pub(super) fn close_import(&mut self) {
        if let Some(state) = self.import.as_ref() {
            state.ticket.fetch_add(1, Ordering::SeqCst);
        }
        let project_id = match &self.screen {
            super::app::Screen::Import { project_id } => project_id.clone(),
            _ => self
                .import
                .as_ref()
                .map(|s| s.project_id.clone())
                .unwrap_or_default(),
        };
        self.import = None;
        if !project_id.is_empty() {
            self.screen = super::app::Screen::Work { project_id };
        }
    }

    fn abort_import_parse(&mut self) {
        let Some(state) = self.import.as_mut() else {
            return;
        };
        state.ticket.fetch_add(1, Ordering::SeqCst);
        state.parsing = false;
    }

    fn toggle_import_row(&mut self, index: usize, checked: bool) {
        let Some(state) = self.import.as_mut() else {
            return;
        };
        if let Some(slot) = state.selected.get_mut(index) {
            *slot = checked;
        }
    }

    fn start_import_parse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.import.as_ref() else {
            return;
        };
        if state.parsing {
            return;
        }
        let project_id = state.project_id.clone();
        let paste = state.paste.read(cx).value().to_string();
        if paste.trim().is_empty() {
            if let Some(state) = self.import.as_mut() {
                state.error = Some("请先粘贴接口说明".into());
            }
            return;
        }
        if !model_source_ready(&self.service) {
            self.prompt_select_source(window, cx);
            return;
        }
        let expected = if let Some(state) = self.import.as_mut() {
            let expected = state.ticket.fetch_add(1, Ordering::SeqCst) + 1;
            state.parsing = true;
            state.error = None;
            state.drafts.clear();
            state.selected.clear();
            expected
        } else {
            return;
        };
        let job = self
            .service
            .import_paste_job(project_id.clone(), paste.clone());
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(async move { job() }).await;
            this.update_in(cx, |view, _, cx| {
                if view
                    .import
                    .as_ref()
                    .is_none_or(|s| s.ticket.load(Ordering::SeqCst) != expected)
                {
                    return;
                }
                match result {
                    Ok(drafts) => {
                        let remembered = view.service.remember_import_paste(&project_id, &paste);
                        let existing = view
                            .service
                            .list_endpoints(&project_id)
                            .unwrap_or_default()
                            .into_iter()
                            .map(|e| (e.method, e.path))
                            .collect::<Vec<_>>();
                        let Some(state) = view.import.as_mut() else {
                            return;
                        };
                        if state.ticket.load(Ordering::SeqCst) != expected {
                            return;
                        }
                        state.parsing = false;
                        match remembered {
                            Ok(()) => {
                                state.selected = default_selected(&drafts, &existing);
                                state.existing = existing;
                                state.drafts = drafts;
                                state.error = None;
                            }
                            Err(err) => {
                                state.error = Some(err.to_string());
                            }
                        }
                    }
                    Err(err) => {
                        let Some(state) = view.import.as_mut() else {
                            return;
                        };
                        if state.ticket.load(Ordering::SeqCst) != expected {
                            return;
                        }
                        state.parsing = false;
                        state.drafts.clear();
                        state.selected.clear();
                        state.error = Some(err.to_string());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn prompt_select_source(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            dialog
                .title("去设置里选择模型来源")
                .child("导入需要先在设置里选择可用的模型来源。")
                .alert()
                .button_props(DialogButtonProps::default().ok_text("去设置"))
                .on_ok(move |_, window, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.open_settings(window, cx);
                    });
                    true
                })
        });
    }

    fn commit_import_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.import.as_ref() else {
            return;
        };
        if !commit_enabled(&state.drafts, &state.selected, &state.existing) {
            return;
        }
        let project_id = state.project_id.clone();
        let drafts: Vec<ImportDraft> = state
            .drafts
            .iter()
            .enumerate()
            .filter(|(i, _)| state.selected.get(*i).copied().unwrap_or(false))
            .filter(|(i, _)| {
                row_issue(*i, &state.drafts, &state.selected, &state.existing).is_none()
            })
            .map(|(_, d)| d.clone())
            .collect();
        if drafts.is_empty() {
            return;
        }
        if let Err(err) = self.service.commit_import(&project_id, drafts) {
            window.push_notification(Notification::error(err.to_string()), cx);
            return;
        }
        self.import = None;
        self.work = None;
        self.open_work(project_id);
    }
}

#[cfg(test)]
mod tests {
    use super::{commit_enabled, default_selected, row_issue};
    use crate::import::ImportDraft;
    use serde_json::json;

    fn draft(method: &str, path: &str) -> ImportDraft {
        ImportDraft {
            name: "n".into(),
            path: path.into(),
            method: method.into(),
            deprecated: false,
            notes: String::new(),
            request_headers: Vec::new(),
            request_body_fields: Vec::new(),
            response_fields: Vec::new(),
            success_body: json!({}),
        }
    }

    #[test]
    fn empty_path_is_marked_and_blocks_commit() {
        let drafts = vec![draft("POST", "  ")];
        let selected = vec![true];
        assert_eq!(row_issue(0, &drafts, &selected, &[]), Some("路径为空"));
        assert!(!commit_enabled(&drafts, &selected, &[]));
        assert!(!commit_enabled(&drafts, &[false], &[]));
    }

    #[test]
    fn project_duplicate_is_marked() {
        let drafts = vec![draft("POST", "/a")];
        let selected = vec![true];
        let existing = vec![("post".into(), "/a".into())];
        assert_eq!(
            row_issue(0, &drafts, &selected, &existing),
            Some("与项目内重复")
        );
        assert!(!commit_enabled(&drafts, &selected, &existing));
    }

    #[test]
    fn batch_duplicate_clears_when_one_unchecked() {
        let drafts = vec![draft("POST", "/a"), draft("POST", "/a")];
        let both = vec![true, true];
        assert_eq!(row_issue(0, &drafts, &both, &[]), Some("与本批重复"));
        assert_eq!(row_issue(1, &drafts, &both, &[]), Some("与本批重复"));
        assert!(!commit_enabled(&drafts, &both, &[]));
        let one = vec![true, false];
        assert_eq!(row_issue(0, &drafts, &one, &[]), None);
        assert!(commit_enabled(&drafts, &one, &[]));
    }

    #[test]
    fn default_selected_skips_empty_and_duplicates() {
        let drafts = vec![
            draft("POST", "/a"),
            draft("POST", "/a"),
            draft("GET", "/a"),
            draft("POST", ""),
            draft("POST", "/b"),
        ];
        let existing = vec![("GET".into(), "/a".into())];
        let selected = default_selected(&drafts, &existing);
        assert_eq!(selected, vec![true, false, false, false, true]);
        assert!(commit_enabled(&drafts, &selected, &existing));
    }
}
