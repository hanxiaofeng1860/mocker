use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    form::{field, v_form},
    h_flex,
    input::{Input, InputEvent, InputState},
    notification::Notification,
    radio::{Radio, RadioGroup},
    tag::Tag,
    v_flex, ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, StyledExt as _,
    WindowExt as _,
};

use crate::domain::GlobalSettings;
use crate::llm::{HttpModelClient, ModelClient};
use crate::service::AppService;
use crate::sources::{home_dir, scan_sources, ModelSource, Protocol};

use super::app::AppView;
use super::style;
use super::theme::{apply_ink_theme, apply_paper_theme};

pub(super) struct SettingsState {
    sources: Vec<ModelSource>,
    selected_source_id: String,
    show_manual: bool,
    manual_anim: u64,
    testing: bool,
    manual_protocol: String,
    manual_base_url: Entity<InputState>,
    manual_api_key: Entity<InputState>,
    manual_model: Entity<InputState>,
    _subs: Vec<Subscription>,
}

impl SettingsState {
    pub(super) fn new(
        service: &AppService,
        window: &mut Window,
        cx: &mut Context<AppView>,
    ) -> Self {
        let stored = service.load_settings().unwrap_or_default();
        let protocol = if stored.manual_protocol.trim().is_empty() {
            "openai-chat".to_string()
        } else {
            stored.manual_protocol.clone()
        };
        let show_manual = stored.selected_source_id == "manual";
        let manual_base_url = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("https://api.example.com")
                .default_value(stored.manual_base_url.clone())
        });
        let manual_api_key = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("API Key")
                .default_value(stored.manual_api_key.clone())
        });
        let manual_model = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("model")
                .default_value(stored.manual_model.clone())
        });
        let mut state = Self {
            sources: scan_sources(&home_dir()),
            selected_source_id: stored.selected_source_id,
            show_manual,
            manual_anim: 0,
            testing: false,
            manual_protocol: protocol,
            manual_base_url: manual_base_url.clone(),
            manual_api_key: manual_api_key.clone(),
            manual_model: manual_model.clone(),
            _subs: Vec::new(),
        };
        state
            ._subs
            .push(subscribe_manual_blur(&manual_base_url, window, cx));
        state
            ._subs
            .push(subscribe_manual_blur(&manual_api_key, window, cx));
        state
            ._subs
            .push(subscribe_manual_blur(&manual_model, window, cx));
        state
    }
}

fn subscribe_manual_blur(
    input: &Entity<InputState>,
    window: &Window,
    cx: &mut Context<AppView>,
) -> Subscription {
    cx.subscribe_in(input, window, |this, _, event: &InputEvent, window, cx| {
        if matches!(event, InputEvent::Blur)
            && this.settings.as_ref().is_some_and(|s| s.show_manual)
        {
            this.persist_selection(window, cx);
        }
    })
}

pub(super) fn apply_saved_client(service: &AppService) {
    let Ok(settings) = service.load_settings() else {
        return;
    };
    let sources = scan_sources(&home_dir());
    if let Some(client) = client_for_selection(&settings, &sources) {
        let _ = service.set_model(client);
    }
}

pub(super) fn client_for_selection(
    settings: &GlobalSettings,
    sources: &[ModelSource],
) -> Option<HttpModelClient> {
    if settings.selected_source_id.is_empty() {
        return None;
    }
    if settings.selected_source_id == "manual" {
        return Some(HttpModelClient::with_inline_key(
            parse_protocol(&settings.manual_protocol),
            settings.manual_base_url.clone(),
            settings.manual_model.clone(),
            settings.manual_api_key.clone(),
        ));
    }
    sources
        .iter()
        .find(|s| s.id == settings.selected_source_id && s.available)
        .map(HttpModelClient::from_source)
}

pub(super) fn parse_protocol(raw: &str) -> Protocol {
    match raw.trim() {
        "anthropic-messages" | "anthropic" => Protocol::AnthropicMessages,
        _ => Protocol::OpenAIChat,
    }
}

fn compact_base(url: &str) -> String {
    let s = url.trim().trim_end_matches('/');
    let rest = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(s);
    rest.split(['/', '?', '#'])
        .next()
        .unwrap_or(rest)
        .to_string()
}

fn source_subtitle(source: &ModelSource) -> String {
    if !source.available && !source.reason.is_empty() {
        return source.reason.clone();
    }
    let proto = match source.protocol {
        Protocol::AnthropicMessages => "anthropic",
        Protocol::OpenAIChat => "openai",
    };
    if !source.model.is_empty() {
        format!("{proto} · {}", source.model)
    } else if !source.base_url.is_empty() {
        compact_base(&source.base_url)
    } else {
        proto.to_string()
    }
}

pub(super) fn view(state: &SettingsState, cx: &mut Context<AppView>) -> AnyElement {
    let selected = state.selected_source_id.clone();
    let protocol_ix = if state.manual_protocol == "anthropic-messages" {
        0
    } else {
        1
    };
    let is_dark = cx.theme().is_dark();

    let rows: Vec<AnyElement> = state
        .sources
        .iter()
        .map(|source| source_row(source, &selected, cx))
        .collect();

    v_flex()
        .size_full()
        .gap_2()
        .child(
            h_flex()
                .flex_shrink_0()
                .gap_3()
                .child(
                    Button::new("back-settings")
                        .ghost()
                        .small()
                        .label("← 返回")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_settings(window, cx);
                            cx.notify();
                        })),
                )
                .child(div().text_xl().font_semibold().child("设置")),
        )
        .child(
            v_flex()
                .id("settings-body")
                .flex_1()
                .min_h_0()
                .w_full()
                .max_w(px(720.))
                .overflow_y_scroll()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("低频配置放这里。所有项目共用一套模型来源。"),
                )
                .child(style::section_label("模型来源", cx))
                .map(|this| {
                    if rows.is_empty() {
                        this.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("未扫描到来源。可手动填写兼容接口。"),
                        )
                    } else {
                        this.children(rows)
                    }
                })
                .child(
                    h_flex()
                        .gap_2()
                        .mt_2()
                        .mb_2()
                        .child(
                            Button::new("refresh-scan")
                                .small()
                                .label("刷新扫描")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.refresh_scan(window, cx);
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("test-source")
                                .small()
                                .label("测试连通")
                                .loading(state.testing)
                                .disabled(state.testing)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.test_connectivity(window, cx);
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("manual-source")
                                .small()
                                .label("手动填写接口")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.show_manual(window, cx);
                                    cx.notify();
                                })),
                        ),
                )
                .when(state.show_manual, |this| {
                    this.child(style::appear(
                        format!("manual-form-{}", state.manual_anim),
                        style::card(cx)
                            .w_full()
                            .gap_3()
                            .p_5()
                            .child(
                                v_form()
                                    .columns(2)
                                    .child(
                                        field()
                                            .label("Base URL")
                                            .col_span(2)
                                            .child(Input::new(&state.manual_base_url).w_full()),
                                    )
                                    .child(
                                        field()
                                            .label("API Key")
                                            .col_span(2)
                                            .child(Input::new(&state.manual_api_key).w_full()),
                                    )
                                    .child(
                                        field().label("协议").col_span(2).child(
                                            RadioGroup::horizontal("manual-protocol")
                                                .selected_index(Some(protocol_ix))
                                                .child(
                                                    Radio::new("proto-anthropic")
                                                        .label("anthropic-messages"),
                                                )
                                                .child(
                                                    Radio::new("proto-openai").label("openai-chat"),
                                                )
                                                .on_click(cx.listener(|this, ix, window, cx| {
                                                    this.set_manual_protocol(*ix, window, cx);
                                                    cx.notify();
                                                })),
                                        ),
                                    )
                                    .child(
                                        field()
                                            .label("模型")
                                            .col_span(2)
                                            .child(Input::new(&state.manual_model).w_full()),
                                    ),
                            ),
                    ))
                })
                .child(style::section_label("外观", cx))
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("theme-paper")
                                .small()
                                .label("Paper")
                                .selected(!is_dark)
                                .on_click(cx.listener(|_, _, _, cx| {
                                    apply_paper_theme(cx);
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("theme-ink")
                                .small()
                                .label("Ink")
                                .selected(is_dark)
                                .on_click(cx.listener(|_, _, _, cx| {
                                    apply_ink_theme(cx);
                                    cx.notify();
                                })),
                        ),
                ),
        )
        .into_any_element()
}

fn source_row(source: &ModelSource, selected: &str, cx: &mut Context<AppView>) -> AnyElement {
    let id = source.id.clone();
    let available = source.available;
    let checked = selected == source.id;
    let badge = if available {
        Tag::success().small().child("可用")
    } else {
        Tag::secondary().small().child("不可用")
    };
    let subtitle = source_subtitle(source);
    let label = source.label.clone();

    h_flex()
        .id(SharedString::from(format!("src-row-{id}")))
        .w_full()
        .gap_2()
        .items_center()
        .p_3()
        .mb_1()
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().popover)
        .shadow(style::paper_shadow(cx))
        .when(!available, |this| this.opacity(0.55))
        .child(
            Radio::new(SharedString::from(format!("src-{id}")))
                .flex_1()
                .checked(checked)
                .disabled(!available)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.select_source(id.clone(), window, cx);
                    cx.notify();
                }))
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap_2()
                        .child(
                            v_flex()
                                .flex_1()
                                .child(div().font_semibold().child(label))
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .font_family(cx.theme().mono_font_family.clone())
                                        .child(subtitle),
                                ),
                        )
                        .child(badge),
                ),
        )
        .into_any_element()
}

impl AppView {
    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.screen, super::app::Screen::Settings { .. }) {
            return;
        }
        self.settings = Some(SettingsState::new(&self.service, window, cx));
        let back = self.screen.clone();
        self.screen = super::app::Screen::Settings {
            back: Box::new(back),
        };
    }

    pub(super) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.is_some() {
            self.persist_selection(window, cx);
        }
        if let super::app::Screen::Settings { back } =
            std::mem::replace(&mut self.screen, super::app::Screen::Home)
        {
            self.screen = *back;
        }
        self.settings = None;
    }

    fn refresh_scan(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sources = scan_sources(&home_dir());
        if let Some(state) = self.settings.as_mut() {
            state.sources = sources;
        }
        self.persist_selection(window, cx);
    }

    fn select_source(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.settings.as_mut() {
            let available = state
                .sources
                .iter()
                .find(|s| s.id == id)
                .is_some_and(|s| s.available);
            if !available {
                return;
            }
            state.selected_source_id = id;
            state.show_manual = false;
        }
        self.persist_selection(window, cx);
    }

    fn show_manual(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.settings.as_mut() {
            state.selected_source_id = "manual".into();
            if !state.show_manual {
                state.manual_anim = state.manual_anim.saturating_add(1);
            }
            state.show_manual = true;
        }
        self.persist_selection(window, cx);
    }

    fn set_manual_protocol(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.settings.as_mut() {
            state.manual_protocol = if ix == 0 {
                "anthropic-messages".into()
            } else {
                "openai-chat".into()
            };
            state.selected_source_id = "manual".into();
            state.show_manual = true;
        }
        self.persist_selection(window, cx);
    }

    fn persist_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.settings.as_ref() else {
            return;
        };
        let mut settings = match self.service.load_settings() {
            Ok(s) => s,
            Err(err) => {
                window.push_notification(Notification::error(err.to_string()), cx);
                return;
            }
        };
        settings.selected_source_id = state.selected_source_id.clone();
        let sources = state.sources.clone();
        if state.selected_source_id == "manual" {
            settings.manual_base_url = state.manual_base_url.read(cx).value().trim().to_string();
            settings.manual_api_key = state.manual_api_key.read(cx).value().to_string();
            settings.manual_protocol = state.manual_protocol.clone();
            settings.manual_model = state.manual_model.read(cx).value().trim().to_string();
            settings.selected_model = settings.manual_model.clone();
        } else if let Some(source) = sources.iter().find(|s| s.id == state.selected_source_id) {
            settings.selected_model = source.model.clone();
        }
        if let Err(err) = self.service.save_settings(&settings) {
            window.push_notification(Notification::error(err.to_string()), cx);
            return;
        }
        if let Some(client) = client_for_selection(&settings, &sources) {
            if let Err(err) = self.service.set_model(client) {
                window.push_notification(Notification::error(err.to_string()), cx);
            }
        }
    }

    fn test_connectivity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.as_ref().is_some_and(|s| s.testing) {
            return;
        }
        if self.settings.as_ref().is_some_and(|s| s.show_manual) {
            self.persist_selection(window, cx);
        }
        let (client, model_name) = {
            let Some(state) = self.settings.as_ref() else {
                return;
            };
            if state.selected_source_id.is_empty() {
                window.push_notification(Notification::error("请先选择模型来源"), cx);
                return;
            }
            let settings = snapshot_settings(state, cx);
            let Some(client) = client_for_selection(&settings, &state.sources) else {
                window.push_notification(Notification::error("当前来源不可用"), cx);
                return;
            };
            let model_name = if settings.selected_source_id == "manual" {
                settings.manual_model.clone()
            } else {
                state
                    .sources
                    .iter()
                    .find(|s| s.id == settings.selected_source_id)
                    .map(|s| s.model.clone())
                    .unwrap_or_default()
            };
            (client, model_name)
        };
        if let Some(state) = self.settings.as_mut() {
            state.testing = true;
        }
        cx.spawn_in(window, async move |this, cx| {
            let task = cx.background_spawn(async move { client.complete_json("ping") });
            let result = task.await;
            this.update_in(cx, |view, window, cx| {
                if let Some(state) = view.settings.as_mut() {
                    state.testing = false;
                }
                match result {
                    Ok(_) => {
                        let msg = if model_name.trim().is_empty() {
                            "连通成功".to_string()
                        } else {
                            format!("连通成功 · {model_name}")
                        };
                        window.push_notification(Notification::success(msg), cx);
                    }
                    Err(err) => {
                        window.push_notification(Notification::error(err.to_string()), cx);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

fn snapshot_settings(state: &SettingsState, cx: &App) -> GlobalSettings {
    let mut settings = GlobalSettings {
        selected_source_id: state.selected_source_id.clone(),
        ..Default::default()
    };
    if state.selected_source_id == "manual" {
        settings.manual_base_url = state.manual_base_url.read(cx).value().trim().to_string();
        settings.manual_api_key = state.manual_api_key.read(cx).value().to_string();
        settings.manual_protocol = state.manual_protocol.clone();
        settings.manual_model = state.manual_model.read(cx).value().trim().to_string();
        settings.selected_model = settings.manual_model.clone();
    } else if let Some(source) = state
        .sources
        .iter()
        .find(|s| s.id == state.selected_source_id)
    {
        settings.selected_model = source.model.clone();
    }
    settings
}

#[cfg(test)]
mod tests {
    use super::{client_for_selection, compact_base, parse_protocol};
    use crate::domain::GlobalSettings;
    use crate::sources::{CredentialFrom, CredentialKind, ModelSource, Protocol};
    use std::path::PathBuf;

    #[test]
    fn parse_protocol_maps_spec_names() {
        assert_eq!(
            parse_protocol("anthropic-messages"),
            Protocol::AnthropicMessages
        );
        assert_eq!(parse_protocol(" openai-chat "), Protocol::OpenAIChat);
        assert_eq!(parse_protocol(""), Protocol::OpenAIChat);
        assert_eq!(parse_protocol("anthropic"), Protocol::AnthropicMessages);
    }

    #[test]
    fn compact_base_strips_scheme_and_path() {
        assert_eq!(compact_base("http://127.0.0.1:15721/v1"), "127.0.0.1:15721");
        assert_eq!(
            compact_base("https://api.example.com/v1/"),
            "api.example.com"
        );
    }

    fn sample_source(available: bool) -> ModelSource {
        ModelSource {
            id: "claude-code".into(),
            label: "Claude Code".into(),
            protocol: Protocol::AnthropicMessages,
            base_url: "http://example.invalid".into(),
            model: "claude-haiku".into(),
            available,
            reason: if available {
                String::new()
            } else {
                "缺 Token".into()
            },
            credential_from: CredentialFrom::File {
                path: PathBuf::from("/tmp/settings.json"),
                kind: CredentialKind::ClaudeEnv,
            },
        }
    }

    #[test]
    fn client_for_selection_skips_unavailable() {
        let settings = GlobalSettings {
            selected_source_id: "claude-code".into(),
            ..Default::default()
        };
        assert!(client_for_selection(&settings, &[sample_source(false)]).is_none());
    }

    #[test]
    fn client_for_selection_uses_available_scan() {
        let settings = GlobalSettings {
            selected_source_id: "claude-code".into(),
            ..Default::default()
        };
        assert!(client_for_selection(&settings, &[sample_source(true)]).is_some());
    }

    #[test]
    fn client_for_selection_manual_always_builds() {
        let settings = GlobalSettings {
            selected_source_id: "manual".into(),
            manual_base_url: "http://127.0.0.1:1".into(),
            manual_api_key: "sk-test".into(),
            manual_protocol: "openai-chat".into(),
            manual_model: "gpt".into(),
            ..Default::default()
        };
        assert!(client_for_selection(&settings, &[]).is_some());
    }
}
