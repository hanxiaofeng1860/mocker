use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    dialog::DialogButtonProps,
    form::{field, v_form},
    h_flex,
    input::{Input, InputState},
    notification::Notification,
    radio::{Radio, RadioGroup},
    select::{SearchableVec, Select, SelectEvent, SelectState},
    tag::Tag,
    v_flex, ActiveTheme as _, Disableable as _, IndexPath, Selectable as _, Sizable as _,
    StyledExt as _, WindowExt as _,
};

use crate::domain::{GlobalSettings, ManualSource};
use crate::llm::{list_models, HttpModelClient, ModelClient};
use crate::service::AppService;
use crate::sources::{home_dir, scan_sources, ModelSource, Protocol};
use crate::store;

use super::app::AppView;
use super::style;
use super::theme::{apply_ink_theme, apply_paper_theme};

pub(super) struct SettingsState {
    sources: Vec<ModelSource>,
    selected_source_id: String,
    testing: bool,
    probing: bool,
    editing_manual_id: Option<String>,
    manual_sources: Vec<ManualSource>,
    manual_protocol: String,
    manual_name: Entity<InputState>,
    manual_base_url: Entity<InputState>,
    manual_api_key: Entity<InputState>,
    manual_model: Entity<InputState>,
    manual_model_select: Entity<SelectState<SearchableVec<String>>>,
    _subs: Vec<Subscription>,
}

impl SettingsState {
    pub(super) fn new(
        service: &AppService,
        window: &mut Window,
        cx: &mut Context<AppView>,
    ) -> Self {
        let stored = service.load_settings().unwrap_or_default();
        let manual_name = cx.new(|cx| InputState::new(window, cx).placeholder("例如：DeepSeek"));
        let manual_base_url =
            cx.new(|cx| InputState::new(window, cx).placeholder("https://api.example.com"));
        let manual_api_key = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("API Key")
        });
        let manual_model =
            cx.new(|cx| InputState::new(window, cx).placeholder("model 或探测后选择"));
        let manual_model_select = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(Vec::<String>::new()),
                None::<IndexPath>,
                window,
                cx,
            )
            .searchable(true)
        });
        let mut state = Self {
            sources: scan_sources(&home_dir()),
            selected_source_id: stored.selected_source_id,
            testing: false,
            probing: false,
            editing_manual_id: None,
            manual_sources: stored.manual_sources,
            manual_protocol: "openai-chat".into(),
            manual_name,
            manual_base_url,
            manual_api_key,
            manual_model,
            manual_model_select: manual_model_select.clone(),
            _subs: Vec::new(),
        };
        state._subs.push(cx.subscribe_in(
            &manual_model_select,
            window,
            |this, _, event: &SelectEvent<SearchableVec<String>>, window, cx| {
                let SelectEvent::Confirm(Some(value)) = event else {
                    return;
                };
                let Some(settings) = this.settings.as_mut() else {
                    return;
                };
                let value = value.clone();
                settings.manual_model.update(cx, |input, cx| {
                    input.set_value(value, window, cx);
                });
                cx.notify();
            },
        ));
        state
    }
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
    if let Some(manual) = settings.find_manual(&settings.selected_source_id) {
        return Some(HttpModelClient::with_inline_key(
            parse_protocol(&manual.protocol),
            manual.base_url.clone(),
            manual.model.clone(),
            manual.api_key.clone(),
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

fn hinted_model_select(
    state: &Entity<SelectState<SearchableVec<String>>>,
    empty: bool,
    muted: Hsla,
) -> impl IntoElement {
    // Select 空占位用 accent_foreground，Paper 下近乎白色；改叠 muted 提示。
    div()
        .relative()
        .w_full()
        .child(Select::new(state).placeholder("").w_full())
        .when(empty, |this| {
            this.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(12.))
                    .right(px(28.))
                    .flex()
                    .items_center()
                    .text_color(muted)
                    .child("探测后从列表选择"),
            )
        })
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
                .children(
                    state
                        .manual_sources
                        .iter()
                        .map(|source| manual_source_row(source, &selected, cx))
                        .collect::<Vec<_>>(),
                )
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
                                    this.open_manual_dialog(None, window, cx);
                                    cx.notify();
                                })),
                        ),
                )
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

fn manual_source_row(
    source: &ManualSource,
    selected: &str,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let id = source.id.clone();
    let edit_id = id.clone();
    let del_id = id.clone();
    let checked = selected == source.id;
    let proto = if source.protocol == "anthropic-messages" {
        "anthropic"
    } else {
        "openai"
    };
    let subtitle = if !source.model.trim().is_empty() {
        format!("{proto} · {}", source.model.trim())
    } else {
        compact_base(&source.base_url)
    };
    let name = if source.name.trim().is_empty() {
        "未命名".to_string()
    } else {
        source.name.clone()
    };
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
        .child(
            Radio::new(SharedString::from(format!("src-{id}")))
                .flex_1()
                .checked(checked)
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
                                .child(div().font_semibold().child(name))
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .font_family(cx.theme().mono_font_family.clone())
                                        .child(subtitle),
                                ),
                        )
                        .child(Tag::success().small().child("可用")),
                ),
        )
        .child(
            Button::new(SharedString::from(format!("edit-manual-{edit_id}")))
                .ghost()
                .small()
                .label("编辑")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_manual_dialog(Some(edit_id.clone()), window, cx);
                    cx.notify();
                })),
        )
        .child(
            Button::new(SharedString::from(format!("del-manual-{del_id}")))
                .ghost()
                .small()
                .danger()
                .label("删除")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.delete_manual_source(&del_id, window, cx);
                    cx.notify();
                })),
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
            let is_manual = state.manual_sources.iter().any(|m| m.id == id);
            let available = is_manual
                || state
                    .sources
                    .iter()
                    .find(|s| s.id == id)
                    .is_some_and(|s| s.available);
            if !available {
                return;
            }
            state.selected_source_id = id;
        }
        self.persist_selection(window, cx);
    }

    fn open_manual_dialog(
        &mut self,
        edit_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.fill_manual_form(edit_id.as_deref(), window, cx);
        let Some(state) = self.settings.as_ref() else {
            return;
        };
        let name = state.manual_name.clone();
        let base = state.manual_base_url.clone();
        let api_key = state.manual_api_key.clone();
        let model = state.manual_model.clone();
        let model_select = state.manual_model_select.clone();
        let protocol_ix = if state.manual_protocol == "anthropic-messages" {
            0
        } else {
            1
        };
        let title = if edit_id.is_some() {
            "编辑手动接口"
        } else {
            "手动填写接口"
        };
        let proto_id = format!("manual-protocol-{}", edit_id.as_deref().unwrap_or("new"));
        let view = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let view_proto = view.clone();
            let view_ok = view.clone();
            let view_probe = view.clone();
            let view_footer = view.clone();
            let muted = cx.theme().muted_foreground;
            let select_empty = model_select.read(cx).selected_value().is_none();
            dialog
                .title(title)
                .w(px(520.))
                .child(
                    v_form()
                        .columns(2)
                        .child(
                            field()
                                .label("名称")
                                .col_span(2)
                                .child(Input::new(&name).w_full()),
                        )
                        .child(
                            field()
                                .label("Base URL")
                                .col_span(2)
                                .child(Input::new(&base).w_full()),
                        )
                        .child(
                            field()
                                .label("API Key")
                                .col_span(2)
                                .child(Input::new(&api_key).w_full()),
                        )
                        .child(
                            field().label("协议").col_span(2).child(
                                RadioGroup::horizontal(SharedString::from(proto_id.clone()))
                                    .selected_index(Some(protocol_ix))
                                    .child(
                                        Radio::new("proto-anthropic").label("anthropic-messages"),
                                    )
                                    .child(Radio::new("proto-openai").label("openai-chat"))
                                    .on_click(move |ix, _, cx| {
                                        let _ = view_proto.update(cx, |this, _cx| {
                                            if let Some(state) = this.settings.as_mut() {
                                                state.manual_protocol = if *ix == 0 {
                                                    "anthropic-messages".into()
                                                } else {
                                                    "openai-chat".into()
                                                };
                                            }
                                        });
                                    }),
                            ),
                        )
                        .child(
                            field().label("模型").col_span(2).child(
                                v_flex()
                                    .w_full()
                                    .gap_2()
                                    .child(
                                        h_flex()
                                            .w_full()
                                            .gap_2()
                                            .child(Input::new(&model).w_full())
                                            .child(
                                                Button::new("probe-models")
                                                    .small()
                                                    .label("探测")
                                                    .on_click(move |_, window, cx| {
                                                        let _ =
                                                            view_probe.update(cx, |this, cx| {
                                                                this.probe_manual_models(
                                                                    window, cx,
                                                                );
                                                            });
                                                    }),
                                            ),
                                    )
                                    .child(hinted_model_select(&model_select, select_empty, muted))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(muted)
                                            .child("可手填模型 id，或探测后从列表选择。"),
                                    ),
                            ),
                        ),
                )
                .overlay_closable(false)
                .close_button(false)
                .keyboard(false)
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("保存")
                        .cancel_text("取消"),
                )
                .footer(move |ok, cancel, window, cx| {
                    let testing = view_footer
                        .upgrade()
                        .and_then(|v| v.read(cx).settings.as_ref().map(|s| s.testing))
                        .unwrap_or(false);
                    let view_test = view_footer.clone();
                    vec![
                        Button::new("test-manual-source")
                            .label("检测")
                            .loading(testing)
                            .disabled(testing)
                            .on_click(move |_, window, cx| {
                                let _ = view_test.update(cx, |this, cx| {
                                    this.test_manual_dialog(window, cx);
                                });
                            })
                            .into_any_element(),
                        cancel(window, cx),
                        ok(window, cx),
                    ]
                })
                .on_ok(move |_, window, cx| {
                    view_ok
                        .update(cx, |this, cx| this.save_manual_dialog(window, cx))
                        .unwrap_or(true)
                })
        });
        cx.notify();
    }

    fn fill_manual_form(
        &mut self,
        edit_id: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.settings.as_mut() else {
            return;
        };
        state.editing_manual_id = edit_id.map(str::to_string);
        let existing =
            edit_id.and_then(|id| state.manual_sources.iter().find(|m| m.id == id).cloned());
        let (name, base, key, protocol, model) = match existing {
            Some(src) => (src.name, src.base_url, src.api_key, src.protocol, src.model),
            None => (
                String::new(),
                String::new(),
                String::new(),
                "openai-chat".into(),
                String::new(),
            ),
        };
        state.manual_protocol = if protocol.trim().is_empty() {
            "openai-chat".into()
        } else {
            protocol
        };
        state.manual_name.update(cx, |input, cx| {
            input.set_value(name, window, cx);
        });
        state.manual_base_url.update(cx, |input, cx| {
            input.set_value(base, window, cx);
        });
        state.manual_api_key.update(cx, |input, cx| {
            input.set_value(key, window, cx);
        });
        state.manual_model.update(cx, |input, cx| {
            input.set_value(model, window, cx);
        });
        state.manual_model_select.update(cx, |sel, cx| {
            sel.set_items(SearchableVec::new(Vec::<String>::new()), window, cx);
        });
    }

    fn save_manual_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(state) = self.settings.as_ref() else {
            return true;
        };
        let name = state.manual_name.read(cx).value().trim().to_string();
        if name.is_empty() {
            window.push_notification(Notification::error("请填写名称"), cx);
            return false;
        }
        let base = state.manual_base_url.read(cx).value().trim().to_string();
        if base.is_empty() {
            window.push_notification(Notification::error("请填写 Base URL"), cx);
            return false;
        }
        let source = ManualSource {
            id: state
                .editing_manual_id
                .clone()
                .unwrap_or_else(|| format!("manual-{}", store::new_id())),
            name,
            base_url: base,
            api_key: state.manual_api_key.read(cx).value().to_string(),
            protocol: state.manual_protocol.clone(),
            model: state.manual_model.read(cx).value().trim().to_string(),
        };
        if let Some(state) = self.settings.as_mut() {
            if let Some(existing) = state.manual_sources.iter_mut().find(|m| m.id == source.id) {
                *existing = source.clone();
            } else {
                state.manual_sources.push(source.clone());
            }
            state.selected_source_id = source.id;
            state.editing_manual_id = None;
        }
        self.persist_selection(window, cx);
        cx.notify();
        true
    }

    fn delete_manual_source(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.settings.as_mut() else {
            return;
        };
        state.manual_sources.retain(|m| m.id != id);
        if state.selected_source_id == id {
            state.selected_source_id = state
                .manual_sources
                .first()
                .map(|m| m.id.clone())
                .or_else(|| {
                    state
                        .sources
                        .iter()
                        .find(|s| s.available)
                        .map(|s| s.id.clone())
                })
                .unwrap_or_default();
        }
        self.persist_selection(window, cx);
    }

    fn probe_manual_models(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.as_ref().is_some_and(|s| s.probing) {
            return;
        }
        let Some(state) = self.settings.as_ref() else {
            return;
        };
        let base = state.manual_base_url.read(cx).value().trim().to_string();
        if base.is_empty() {
            window.push_notification(Notification::error("请先填写 Base URL"), cx);
            return;
        }
        let key = state.manual_api_key.read(cx).value().to_string();
        let protocol = parse_protocol(&state.manual_protocol);
        if let Some(state) = self.settings.as_mut() {
            state.probing = true;
        }
        window.push_notification(Notification::info("正在探测模型…"), cx);
        cx.spawn_in(window, async move |this, cx| {
            let task = cx.background_spawn(async move { list_models(protocol, &base, &key) });
            let result = task.await;
            this.update_in(cx, |view, window, cx| {
                if let Some(state) = view.settings.as_mut() {
                    state.probing = false;
                }
                match result {
                    Ok(models) if models.is_empty() => {
                        window.push_notification(Notification::error("未探测到模型"), cx);
                    }
                    Ok(models) => {
                        let n = models.len();
                        let current = view
                            .settings
                            .as_ref()
                            .map(|s| s.manual_model.read(cx).value().to_string())
                            .unwrap_or_default();
                        if let Some(state) = view.settings.as_mut() {
                            state.manual_model_select.update(cx, |sel, cx| {
                                sel.set_items(SearchableVec::new(models.clone()), window, cx);
                                if models.iter().any(|m| m == &current) {
                                    sel.set_selected_value(&current, window, cx);
                                }
                            });
                        }
                        window.push_notification(
                            Notification::success(format!("探测到 {n} 个模型")),
                            cx,
                        );
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

    fn persist_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.settings.as_ref() else {
            return;
        };
        let mut settings = GlobalSettings {
            selected_source_id: state.selected_source_id.clone(),
            selected_model: String::new(),
            manual_sources: state.manual_sources.clone(),
        };
        let sources = state.sources.clone();
        if let Some(manual) = settings.find_manual(&settings.selected_source_id) {
            settings.selected_model = manual.model.clone();
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
            let model_name = settings
                .find_manual(&settings.selected_source_id)
                .map(|m| {
                    if m.name.trim().is_empty() {
                        m.model.clone()
                    } else {
                        m.name.clone()
                    }
                })
                .or_else(|| {
                    state
                        .sources
                        .iter()
                        .find(|s| s.id == settings.selected_source_id)
                        .map(|s| s.model.clone())
                })
                .unwrap_or_default();
            (client, model_name)
        };
        self.spawn_model_test(client, model_name, window, cx);
    }

    fn test_manual_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.as_ref().is_some_and(|s| s.testing) {
            return;
        }
        let Some(state) = self.settings.as_ref() else {
            return;
        };
        let base = state.manual_base_url.read(cx).value().trim().to_string();
        if base.is_empty() {
            window.push_notification(Notification::error("请填写 Base URL"), cx);
            return;
        }
        let model = state.manual_model.read(cx).value().trim().to_string();
        if model.is_empty() {
            window.push_notification(Notification::error("请填写模型"), cx);
            return;
        }
        let name = state.manual_name.read(cx).value().trim().to_string();
        let label = if name.is_empty() { model.clone() } else { name };
        let client = HttpModelClient::with_inline_key(
            parse_protocol(&state.manual_protocol),
            base,
            model,
            state.manual_api_key.read(cx).value().to_string(),
        );
        self.spawn_model_test(client, label, window, cx);
    }

    fn spawn_model_test(
        &mut self,
        client: HttpModelClient,
        model_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(state) = self.settings.as_mut() {
            state.testing = true;
        }
        window.push_notification(Notification::info("正在检测模型…"), cx);
        cx.notify();
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

fn snapshot_settings(state: &SettingsState, _cx: &App) -> GlobalSettings {
    let mut settings = GlobalSettings {
        selected_source_id: state.selected_source_id.clone(),
        selected_model: String::new(),
        manual_sources: state.manual_sources.clone(),
    };
    if let Some(manual) = settings.find_manual(&settings.selected_source_id) {
        settings.selected_model = manual.model.clone();
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
            selected_source_id: "manual-1".into(),
            manual_sources: vec![crate::domain::ManualSource {
                id: "manual-1".into(),
                name: "本地".into(),
                base_url: "http://127.0.0.1:1".into(),
                api_key: "sk-test".into(),
                protocol: "openai-chat".into(),
                model: "gpt".into(),
            }],
            ..Default::default()
        };
        assert!(client_for_selection(&settings, &[]).is_some());
    }
}
