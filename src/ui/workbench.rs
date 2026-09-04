use std::time::Duration;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    dialog::DialogButtonProps,
    form::{field, v_form},
    h_flex,
    input::{Input, InputEvent, InputState},
    menu::{ContextMenuExt as _, PopupMenuItem},
    notification::Notification,
    scroll::ScrollableElement as _,
    select::{Select, SelectEvent, SelectState},
    switch::Switch,
    tag::Tag,
    v_flex, ActiveTheme as _, Colorize as _, Disableable as _, IndexPath, Selectable as _,
    Sizable as _, StyledExt as _, WindowExt as _,
};

use crate::domain::{Endpoint, Field, FieldLoc, HeaderKv, Project, RequestLog, SceneKind};
use crate::import::semantic_values_prompt;
use crate::llm::ModelClient;
use crate::service::AppService;
use crate::sources::{home_dir, scan_sources};
use crate::store;

use super::app::AppView;
use super::new_project::parse_port;
use super::settings::client_for_selection;
use super::style;

const METHODS: [&'static str; 5] = ["GET", "POST", "PUT", "PATCH", "DELETE"];

pub(super) struct FieldRow {
    field: Field,
    name: Entity<InputState>,
    name_zh: Entity<InputState>,
    type_name: Entity<InputState>,
    comment: Entity<InputState>,
    required: bool,
}

impl FieldRow {
    fn new(field: Field, window: &mut Window, cx: &mut Context<AppView>) -> Self {
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("name")
                .default_value(field.name.clone())
        });
        let name_zh = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("中文名")
                .default_value(field.name_zh.clone())
        });
        let type_name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("String")
                .default_value(if field.type_name.is_empty() {
                    "String".into()
                } else {
                    field.type_name.clone()
                })
        });
        let comment = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("说明")
                .default_value(field.comment.clone())
        });
        Self {
            required: field.required,
            field,
            name,
            name_zh,
            type_name,
            comment,
        }
    }
}

pub(super) struct WorkbenchState {
    pub(super) project_id: String,
    project: Project,
    endpoints: Vec<Endpoint>,
    selected_id: Option<String>,
    logs: Vec<RequestLog>,
    search: Entity<InputState>,
    name: Entity<InputState>,
    path: Entity<InputState>,
    json: Entity<InputState>,
    method: Entity<SelectState<Vec<&'static str>>>,
    settings_name: Entity<InputState>,
    settings_port: Entity<InputState>,
    settings_success: Entity<InputState>,
    settings_fail: Entity<InputState>,
    settings_headers: Entity<InputState>,
    enabled: bool,
    current_scene: SceneKind,
    json_invalid: bool,
    fields: Vec<FieldRow>,
    show_settings: bool,
    settings_anim: u64,
    regenerating: bool,
    suppress_save: bool,
    _subs: Vec<Subscription>,
    _field_subs: Vec<Subscription>,
    _logs_task: Task<()>,
}

impl WorkbenchState {
    pub(super) fn new(
        service: &AppService,
        project_id: String,
        window: &mut Window,
        cx: &mut Context<AppView>,
    ) -> Self {
        let project = service
            .get_project(&project_id)
            .ok()
            .flatten()
            .unwrap_or_else(|| Project {
                id: project_id.clone(),
                name: "找不到项目".into(),
                port: 0,
                success_code: "0000".into(),
                fail_code: "9999".into(),
                default_headers: Vec::new(),
            });
        let endpoints = service.list_endpoints(&project_id).unwrap_or_default();
        let selected = endpoints.first().cloned();
        let scene = selected
            .as_ref()
            .map(|e| e.current_scene)
            .unwrap_or(SceneKind::Success);
        let json_text = selected
            .as_ref()
            .and_then(|e| service.get_scene(&e.id, scene).ok().flatten())
            .map(|s| s.body_json)
            .unwrap_or_default();
        let method_ix = selected
            .as_ref()
            .map(|e| method_index(&e.method))
            .unwrap_or(1);
        let enabled = selected.as_ref().map(|e| e.enabled).unwrap_or(true);
        let logs = service.logs(&project_id).unwrap_or_default();

        let search = cx.new(|cx| InputState::new(window, cx).placeholder("搜索接口"));
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("接口名称")
                .default_value(
                    selected
                        .as_ref()
                        .map(|e| e.name.clone())
                        .unwrap_or_default(),
                )
        });
        let path = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("/api/example")
                .default_value(
                    selected
                        .as_ref()
                        .map(|e| e.path.clone())
                        .unwrap_or_default(),
                )
        });
        let json = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .rows(14)
                .placeholder("{ }")
                .default_value(json_text.clone())
        });
        let method = cx.new(|cx| {
            SelectState::new(
                METHODS.to_vec(),
                Some(IndexPath::new(method_ix)),
                window,
                cx,
            )
        });
        let settings_name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("例如：智能话机")
                .default_value(project.name.clone())
        });
        let settings_port = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("7788")
                .default_value(if project.port == 0 {
                    String::new()
                } else {
                    project.port.to_string()
                })
        });
        let settings_success =
            cx.new(|cx| InputState::new(window, cx).default_value(project.success_code.clone()));
        let settings_fail =
            cx.new(|cx| InputState::new(window, cx).default_value(project.fail_code.clone()));
        let settings_headers = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("sn")
                .default_value(header_keys_text(&project.default_headers))
        });

        let mut state = Self {
            selected_id: selected.as_ref().map(|e| e.id.clone()),
            json_invalid: serde_json::from_str::<serde_json::Value>(&json_text).is_err(),
            project_id,
            project,
            endpoints,
            logs,
            search: search.clone(),
            name: name.clone(),
            path: path.clone(),
            json: json.clone(),
            method: method.clone(),
            settings_name,
            settings_port,
            settings_success,
            settings_fail,
            settings_headers,
            enabled,
            current_scene: scene,
            fields: Vec::new(),
            show_settings: false,
            settings_anim: 0,
            regenerating: false,
            suppress_save: false,
            _subs: Vec::new(),
            _field_subs: Vec::new(),
            _logs_task: cx.spawn_in(window, async move |this, cx| loop {
                Timer::after(Duration::from_millis(500)).await;
                if this
                    .update_in(cx, |view, _, cx| {
                        view.refresh_work_logs();
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }),
        };

        state._subs.push(
            cx.subscribe_in(&search, window, |_, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        );
        state._subs.push(cx.subscribe_in(
            &name,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.save_work_endpoint(window, cx);
                }
            },
        ));
        state._subs.push(cx.subscribe_in(
            &path,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.save_work_endpoint(window, cx);
                }
            },
        ));
        state._subs.push(cx.subscribe_in(
            &json,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.save_work_json(window, cx);
                }
            },
        ));
        state._subs.push(cx.subscribe_in(
            &method,
            window,
            |this, _, event: &SelectEvent<Vec<&'static str>>, window, cx| {
                if matches!(event, SelectEvent::Confirm(Some(_))) {
                    this.save_work_endpoint(window, cx);
                    cx.notify();
                }
            },
        ));

        if let Some(ep) = selected.as_ref() {
            state.rebuild_fields(service, &ep.id, window, cx);
        }
        state
    }

    fn rebuild_fields(
        &mut self,
        service: &AppService,
        endpoint_id: &str,
        window: &mut Window,
        cx: &mut Context<AppView>,
    ) {
        self._field_subs.clear();
        self.fields.clear();
        let list = service.list_fields(endpoint_id).unwrap_or_default();
        for field in list {
            let row = FieldRow::new(field, window, cx);
            let loc = row.field.location;
            self._field_subs
                .push(subscribe_field_input(&row.name, loc, window, cx));
            self._field_subs
                .push(subscribe_field_input(&row.name_zh, loc, window, cx));
            self._field_subs
                .push(subscribe_field_input(&row.type_name, loc, window, cx));
            self._field_subs
                .push(subscribe_field_input(&row.comment, loc, window, cx));
            self.fields.push(row);
        }
    }

    fn collect_fields(&self, cx: &App) -> Vec<Field> {
        self.fields
            .iter()
            .map(|row| {
                let mut field = row.field.clone();
                field.name = row.name.read(cx).value().to_string();
                field.name_zh = row.name_zh.read(cx).value().to_string();
                field.type_name = row.type_name.read(cx).value().to_string();
                field.comment = row.comment.read(cx).value().to_string();
                field.required = row.required;
                field
            })
            .collect()
    }
}

fn subscribe_field_input(
    input: &Entity<InputState>,
    loc: FieldLoc,
    window: &Window,
    cx: &mut Context<AppView>,
) -> Subscription {
    cx.subscribe_in(
        input,
        window,
        move |this, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::Change) {
                this.save_work_fields(window, cx);
                if loc == FieldLoc::Response {
                    this.sync_json_from_response_fields(window, cx);
                }
            }
        },
    )
}

pub(super) fn view(
    state: &WorkbenchState,
    service: &AppService,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let running = service.is_running(&state.project_id);
    v_flex()
        .size_full()
        .gap_3()
        .child(top_bar(state, running, cx))
        .when(state.show_settings, |this| {
            this.child(style::appear(
                format!("work-settings-{}", state.settings_anim),
                project_settings_form(state, cx),
            ))
        })
        .when(!state.show_settings, |this| {
            this.child(
                // h_flex() is items_center; that makes the editor as tall as
                // its content so overflow never kicks in. Use a stretching row.
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .gap_3()
                    .child(sidebar(state, cx))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .h_full()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scrollbar()
                                    .child(editor(state, cx)),
                            ),
                    ),
            )
        })
        .into_any_element()
}

fn top_bar(state: &WorkbenchState, running: bool, cx: &mut Context<AppView>) -> impl IntoElement {
    let badge = if running {
        Tag::success().small().child("运行中")
    } else {
        Tag::secondary().small().child("已停止")
    };
    let toggle = if running { "停止" } else { "启动" };
    h_flex()
        .w_full()
        .items_center()
        .gap_2()
        .child(
            Button::new("back-work")
                .ghost()
                .small()
                .label(if state.show_settings {
                    "← 工作台"
                } else {
                    "← 项目"
                })
                .on_click(cx.listener(|this, _, _, cx| {
                    this.back_from_work();
                    cx.notify();
                })),
        )
        .child(div().font_semibold().child(state.project.name.clone()))
        .child(badge)
        .child(
            div()
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(cx.theme().muted_foreground)
                .child(format!(":{}", state.project.port)),
        )
        .child(
            Button::new("toggle-run")
                .small()
                .when(!running, |this| this.primary())
                .label(toggle)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.toggle_work_runtime(window, cx);
                    cx.notify();
                })),
        )
        .child(div().flex_1())
        .child(live_pill("live-dot-bar", cx))
        .child(
            Button::new("work-logs")
                .small()
                .label(if state.logs.is_empty() {
                    "请求日志".into()
                } else {
                    format!("请求日志 · {}", state.logs.len())
                })
                .on_click(cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.open_logs_dialog(window, cx);
                    cx.notify();
                })),
        )
        .child(
            Button::new("work-import")
                .small()
                .label("导入")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.go_import();
                    cx.notify();
                })),
        )
        .child(
            Button::new("work-project-settings")
                .small()
                .label("项目设置")
                .selected(state.show_settings)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.toggle_project_settings(window, cx);
                    cx.notify();
                })),
        )
}

fn live_pill(dot_id: &'static str, cx: &mut Context<AppView>) -> impl IntoElement {
    h_flex()
        .items_center()
        .gap_1()
        .h_6()
        .px_2()
        .rounded(px(999.))
        .bg(cx.theme().primary.opacity(0.16))
        .text_color(cx.theme().primary)
        .text_xs()
        .child(style::pulse_dot(dot_id, cx.theme().primary))
        .child("已写入 · 下一请求生效")
}

fn project_settings_form(state: &WorkbenchState, cx: &mut Context<AppView>) -> impl IntoElement {
    style::card(cx)
        .w_full()
        .gap_3()
        .p_4()
        .child(
            v_form()
                .columns(2)
                .child(
                    field()
                        .label("项目名称")
                        .col_span(2)
                        .child(Input::new(&state.settings_name).w_full()),
                )
                .child(
                    field()
                        .label("端口")
                        .child(Input::new(&state.settings_port).w_full()),
                )
                .child(
                    field()
                        .label("成功码")
                        .child(Input::new(&state.settings_success).w_full()),
                )
                .child(
                    field()
                        .label("失败码")
                        .child(Input::new(&state.settings_fail).w_full()),
                )
                .child(
                    field()
                        .label("默认请求头（逗号分隔）")
                        .col_span(2)
                        .child(Input::new(&state.settings_headers).w_full()),
                ),
        )
        .child(
            h_flex()
                .gap_2()
                .child(
                    Button::new("save-project-settings")
                        .primary()
                        .label("保存")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.save_project_settings(window, cx);
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("cancel-project-settings")
                        .ghost()
                        .label("取消")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(work) = this.work.as_mut() {
                                work.show_settings = false;
                            }
                            cx.notify();
                        })),
                ),
        )
}

fn sidebar(state: &WorkbenchState, cx: &mut Context<AppView>) -> impl IntoElement {
    let query = state.search.read(cx).value().to_lowercase();
    let items: Vec<Endpoint> = state
        .endpoints
        .iter()
        .filter(|ep| {
            query.is_empty()
                || ep.name.to_lowercase().contains(&query)
                || ep.path.to_lowercase().contains(&query)
        })
        .cloned()
        .collect();
    let selected = state.selected_id.clone();

    style::sidebar_panel(cx)
        .w(px(240.))
        .min_w(px(240.))
        .h_full()
        .min_h_0()
        .gap_2()
        .p_3()
        .child(
            h_flex()
                .w_full()
                .flex_shrink_0()
                .gap_1()
                .child(Input::new(&state.search).small().flex_1())
                .child(
                    Button::new("new-ep-icon")
                        .primary()
                        .small()
                        .compact()
                        .label("+")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.create_work_endpoint(window, cx);
                            cx.notify();
                        })),
                ),
        )
        .child(
            v_flex()
                .id("work-ep-list")
                .flex_1()
                .min_h_0()
                .w_full()
                .overflow_y_scrollbar()
                .gap(px(10.))
                .children(
                    items
                        .into_iter()
                        .map(|ep| endpoint_row(ep, selected.as_deref(), cx))
                        .collect::<Vec<_>>(),
                ),
        )
        .child(
            Button::new("new-ep-full")
                .w_full()
                .flex_shrink_0()
                .small()
                .label("+ 手工新建接口")
                .on_click(cx.listener(|this, _, window, cx| {
                    this.create_work_endpoint(window, cx);
                    cx.notify();
                })),
        )
}

fn endpoint_row(
    ep: Endpoint,
    selected: Option<&str>,
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    let id = ep.id.clone();
    let wrap_id = id.clone();
    let menu_id = id.clone();
    let view = cx.entity().downgrade();
    let on = selected == Some(ep.id.as_str());
    let tail = path_tail(&ep.path).to_string();
    let row = h_flex()
        .id(SharedString::from(format!("ep-row-{id}")))
        .w_full()
        .gap_2()
        .items_start()
        .px_3()
        .py_2()
        .rounded(cx.theme().radius)
        .cursor_pointer()
        .border_1()
        .border_color(if on {
            cx.theme().primary.opacity(0.40)
        } else {
            cx.theme().transparent
        })
        .bg(if on {
            cx.theme().popover
        } else {
            cx.theme().transparent
        })
        .when(on, |this| this.shadow(style::paper_shadow(cx)))
        .when(!on, |this| this.hover(|s| s.bg(cx.theme().muted)))
        .child(method_badge(&ep.method, cx))
        .child(
            v_flex()
                .min_w_0()
                .child(
                    div()
                        .when(on, |this| this.font_medium())
                        .child(ep.name.clone()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .font_family(cx.theme().mono_font_family.clone())
                        .child(tail),
                ),
        )
        .on_click(cx.listener(move |this, _, window, cx| {
            this.select_work_endpoint(id.clone(), window, cx);
            cx.notify();
        }))
        .context_menu(move |menu, _, _| {
            let view = view.clone();
            let menu_id = menu_id.clone();
            menu.item(PopupMenuItem::new("删除").on_click(move |_, window, cx| {
                let _ = view.update(cx, |this, cx| {
                    this.delete_work_endpoint(menu_id.clone(), window, cx);
                    cx.notify();
                });
            }))
        });
    div()
        .id(SharedString::from(format!("ep-wrap-{wrap_id}")))
        .w_full()
        .mt(px(6.))
        .mb(px(6.))
        .child(row)
}

fn method_badge(method: &str, cx: &App) -> impl IntoElement {
    let label = method.to_ascii_uppercase();
    let ink = match label.as_str() {
        "GET" => cx.theme().primary,
        "DELETE" => cx.theme().accent,
        "PUT" | "PATCH" => cx.theme().accent.mix(cx.theme().primary, 0.45),
        _ => cx.theme().foreground,
    };
    Tag::custom(ink.opacity(0.12), ink, ink.opacity(0.22))
        .small()
        .rounded(px(6.))
        .child(label)
}

fn editor(state: &WorkbenchState, cx: &mut Context<AppView>) -> impl IntoElement {
    if state.selected_id.is_none() {
        return empty_editor(cx).into_any_element();
    }
    v_flex()
        .w_full()
        .gap_3()
        .child(
            h_flex()
                .w_full()
                .items_end()
                .gap_3()
                .child(
                    v_flex()
                        .flex_1()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("接口名称"),
                        )
                        .child(Input::new(&state.name).w_full()),
                )
                .child(
                    Switch::new("ep-enabled")
                        .label("启用")
                        .checked(state.enabled)
                        .on_click(cx.listener(|this, checked, window, cx| {
                            this.set_work_enabled(*checked, window, cx);
                            cx.notify();
                        })),
                ),
        )
        .child(
            h_flex()
                .w_full()
                .items_end()
                .gap_3()
                .child(
                    v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("方法"),
                        )
                        .child(Select::new(&state.method).w(px(120.))),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("请求路径"),
                        )
                        .child(div().w_full().child(Input::new(&state.path).w_full())),
                ),
        )
        .child(block_label("当前场景 · 点选即切换运行时", cx))
        .child(scene_chips(state, cx))
        .child(block_label("入参 · Header", cx))
        .child(field_table(state, FieldLoc::Header, cx))
        .child(block_label("入参 · Body", cx))
        .child(field_table(state, FieldLoc::Body, cx))
        .child(
            h_flex()
                .w_full()
                .items_start()
                .gap_3()
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(block_label("响应字段", cx))
                        .child(field_table(state, FieldLoc::Response, cx)),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(block_label("场景 JSON · 改完即写入快照", cx))
                        .child(json_sheet(state, cx)),
                ),
        )
        .into_any_element()
}

fn empty_editor(cx: &mut Context<AppView>) -> impl IntoElement {
    style::card(cx)
        .w_full()
        .gap_3()
        .p_8()
        .items_center()
        .justify_center()
        .child(
            div()
                .text_color(cx.theme().muted_foreground)
                .child("还没有接口。导入文档或手工新建。"),
        )
        .child(
            h_flex()
                .gap_2()
                .child(
                    Button::new("empty-import")
                        .primary()
                        .label("导入")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.go_import();
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("empty-new-ep")
                        .label("手工新建")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.create_work_endpoint(window, cx);
                            cx.notify();
                        })),
                ),
        )
}

fn scene_chips(state: &WorkbenchState, cx: &mut Context<AppView>) -> impl IntoElement {
    h_flex().gap_1().flex_wrap().children(
        SceneKind::all()
            .into_iter()
            .map(|kind| {
                let on = state.current_scene == kind;
                Button::new(SharedString::from(format!("scene-{}", kind.as_str())))
                    .small()
                    .label(scene_label(kind))
                    .selected(on)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.set_work_scene(kind, window, cx);
                        cx.notify();
                    }))
            })
            .collect::<Vec<_>>(),
    )
}

fn field_table(
    state: &WorkbenchState,
    loc: FieldLoc,
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    let rows: Vec<&FieldRow> = state
        .fields
        .iter()
        .filter(|r| r.field.location == loc)
        .collect();
    let add_label = match loc {
        FieldLoc::Header => "+ 添加 header",
        FieldLoc::Body => "+ 添加 body 字段",
        FieldLoc::Response => "+ 添加字段",
    };
    let add_id = match loc {
        FieldLoc::Header => "add-header",
        FieldLoc::Body => "add-body",
        FieldLoc::Response => "add-response",
    };

    style::sheet(cx)
        .w_full()
        .when(rows.is_empty(), |this| {
            this.child(
                h_flex()
                    .p_2()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new(SharedString::from(format!("{add_id}-empty")))
                            .small()
                            .label(add_label)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.add_work_field(loc, window, cx);
                                cx.notify();
                            })),
                    ),
            )
        })
        .when(!rows.is_empty(), |this| {
            this.child(field_header(loc, cx)).children(
                rows.into_iter()
                    .map(|row| field_row(row, loc, &state.fields, cx))
                    .collect::<Vec<_>>(),
            )
        })
        .when(
            state.fields.iter().any(|r| r.field.location == loc),
            |this| {
                this.child(
                    h_flex()
                        .p_2()
                        .gap_2()
                        .child(
                            Button::new(SharedString::from(add_id.to_string()))
                                .small()
                                .label(add_label)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.add_work_field(loc, window, cx);
                                    cx.notify();
                                })),
                        )
                        .when(loc == FieldLoc::Response, |this| {
                            this.child(
                                Button::new("regen-fields")
                                    .small()
                                    .label("按字段重新生成")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.confirm_regenerate_fields(window, cx);
                                    })),
                            )
                        }),
                )
            },
        )
}

fn field_header(loc: FieldLoc, cx: &mut Context<AppView>) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let cell = |text: &'static str, flex: bool| {
        div()
            .when(flex, |d| d.flex_1())
            .when(!flex, |d| d.w(px(52.)))
            .text_xs()
            .text_color(muted)
            .child(text)
    };
    match loc {
        FieldLoc::Response => h_flex()
            .w_full()
            .gap_1()
            .px_2()
            .pt_2()
            .child(cell("路径", true))
            .child(cell("英文名", true))
            .child(cell("中文名", true))
            .child(cell("类型", true))
            .child(div().w(px(36.))),
        _ => h_flex()
            .w_full()
            .gap_1()
            .px_2()
            .pt_2()
            .child(cell("英文名", true))
            .child(cell("中文名", true))
            .child(cell("类型", true))
            .child(div().w(px(48.)).text_xs().text_color(muted).child("必填"))
            .child(cell("说明", true))
            .child(div().w(px(36.))),
    }
}

fn field_row(
    row: &FieldRow,
    loc: FieldLoc,
    all: &[FieldRow],
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    let id = row.field.id.clone();
    let delete = Button::new(SharedString::from(format!("del-field-{id}")))
        .ghost()
        .small()
        .compact()
        .danger()
        .label("删")
        .on_click(cx.listener(move |this, _, window, cx| {
            this.delete_work_field(id.clone(), window, cx);
            cx.notify();
        }));
    match loc {
        FieldLoc::Response => {
            let path = field_path_label(&row.field, all);
            h_flex()
                .w_full()
                .gap_1()
                .px_2()
                .py_1()
                .child(
                    div()
                        .flex_1()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .font_family(cx.theme().mono_font_family.clone())
                        .child(path),
                )
                .child(Input::new(&row.name).small().flex_1())
                .child(Input::new(&row.name_zh).small().flex_1())
                .child(Input::new(&row.type_name).small().flex_1())
                .child(delete)
        }
        _ => {
            let required = row.required;
            let req_id = row.field.id.clone();
            h_flex()
                .w_full()
                .gap_1()
                .px_2()
                .py_1()
                .child(Input::new(&row.name).small().flex_1())
                .child(Input::new(&row.name_zh).small().flex_1())
                .child(Input::new(&row.type_name).small().flex_1())
                .child(
                    Checkbox::new(SharedString::from(format!("req-{req_id}")))
                        .checked(required)
                        .on_click(cx.listener(move |this, checked, window, cx| {
                            this.set_work_field_required(req_id.clone(), *checked, window, cx);
                            cx.notify();
                        })),
                )
                .child(Input::new(&row.comment).small().flex_1())
                .child(delete)
        }
    }
}

fn json_sheet(state: &WorkbenchState, cx: &mut Context<AppView>) -> impl IntoElement {
    style::sheet(cx)
        .w_full()
        .overflow_hidden()
        .child(Input::new(&state.json).h(px(220.)).w_full())
        .when(state.json_invalid, |this| {
            this.child(
                div()
                    .px_2()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("JSON 不合法，已按原文保存"),
            )
        })
        .child(
            h_flex()
                .p_2()
                .gap_2()
                .items_center()
                .child(
                    Button::new("ai-regen")
                        .small()
                        .primary()
                        .label("AI 填充语义值")
                        .loading(state.regenerating)
                        .disabled(state.regenerating)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.regenerate_success_ai(window, cx);
                            cx.notify();
                        })),
                ),
        )
}

fn log_dialog_body(logs: &[RequestLog], list_h: Pixels, cx: &App) -> impl IntoElement {
    v_flex().w_full().gap_2().child(log_header(cx)).map(|this| {
        if logs.is_empty() {
            this.child(
                div()
                    .p_3()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("暂无请求"),
            )
        } else {
            this.child(
                v_flex()
                    .id("dialog-log-rows")
                    .w_full()
                    .h(list_h)
                    .max_h(list_h)
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .children(
                        logs.iter()
                            .take(100)
                            .map(|log| log_row(log, cx))
                            .collect::<Vec<_>>(),
                    ),
            )
        }
    })
}

fn log_header(cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    h_flex()
        .w_full()
        .gap_2()
        .px_3()
        .pb_1()
        .text_xs()
        .text_color(muted)
        .child(div().w(px(72.)).child("时间"))
        .child(div().w(px(56.)).child("方法"))
        .child(div().flex_1().child("路径"))
        .child(div().w(px(48.)).child("状态"))
        .child(div().w(px(56.)).child("场景"))
        .child(div().w(px(52.)).child("耗时"))
}

fn log_row(log: &RequestLog, cx: &App) -> impl IntoElement {
    let missing = if log.missing_default_headers.is_empty() {
        String::new()
    } else {
        format!("缺 header: {}", log.missing_default_headers.join(", "))
    };
    let preview = truncate_chars(log.res_body.trim(), 160);
    v_flex()
        .w_full()
        .px_3()
        .py_2()
        .gap_1()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(
            h_flex()
                .w_full()
                .gap_2()
                .text_xs()
                .child(div().w(px(72.)).child(short_time(&log.at)))
                .child(div().w(px(56.)).child(log.method.clone()))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .font_family(cx.theme().mono_font_family.clone())
                        .child(log.url.clone()),
                )
                .child(div().w(px(48.)).child(log.status.to_string()))
                .child(
                    div()
                        .w(px(56.))
                        .child(scene_log_label(log.scene.as_deref())),
                )
                .child(div().w(px(52.)).child(format!("{}ms", log.elapsed_ms))),
        )
        .when(!missing.is_empty(), |this| {
            this.child(div().text_xs().text_color(cx.theme().danger).child(missing))
        })
        .when(!preview.is_empty(), |this| {
            this.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .font_family(cx.theme().mono_font_family.clone())
                    .child(preview),
            )
        })
}

fn truncate_chars(s: &str, max: usize) -> String {
    let mut chars = s.chars();
    let taken: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{taken}…")
    } else {
        taken
    }
}

fn block_label(text: &'static str, cx: &mut Context<AppView>) -> impl IntoElement {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

impl AppView {
    pub(super) fn go_import(&mut self) {
        let project_id = match &self.screen {
            super::app::Screen::Work { project_id } => project_id.clone(),
            _ => self
                .work
                .as_ref()
                .map(|w| w.project_id.clone())
                .unwrap_or_default(),
        };
        if project_id.is_empty() {
            return;
        }
        self.screen = super::app::Screen::Import { project_id };
    }

    fn refresh_work_logs(&mut self) {
        let Some(work) = self.work.as_mut() else {
            return;
        };
        if let Ok(logs) = self.service.logs(&work.project_id) {
            work.logs = logs;
        }
    }

    fn toggle_work_runtime(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(work) = self.work.as_ref() else {
            return;
        };
        let id = work.project_id.clone();
        let running = self.service.is_running(&id);
        let result = if running {
            self.service.stop(&id)
        } else {
            self.service.start(&id)
        };
        if let Err(err) = result {
            window.push_notification(Notification::error(err.to_string()), cx);
        }
    }

    pub(super) fn back_from_work(&mut self) {
        if self.work.as_ref().is_some_and(|work| work.show_settings) {
            if let Some(work) = self.work.as_mut() {
                work.show_settings = false;
            }
            return;
        }
        self.go_home_or_empty();
    }

    fn toggle_project_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(work) = self.work.as_mut() else {
            return;
        };
        work.show_settings = !work.show_settings;
        if work.show_settings {
            work.settings_anim = work.settings_anim.saturating_add(1);
            let project = work.project.clone();
            work.settings_name.update(cx, |input, cx| {
                input.set_value(project.name.clone(), window, cx);
            });
            work.settings_port.update(cx, |input, cx| {
                input.set_value(project.port.to_string(), window, cx);
            });
            work.settings_success.update(cx, |input, cx| {
                input.set_value(project.success_code.clone(), window, cx);
            });
            work.settings_fail.update(cx, |input, cx| {
                input.set_value(project.fail_code.clone(), window, cx);
            });
            work.settings_headers.update(cx, |input, cx| {
                input.set_value(header_keys_text(&project.default_headers), window, cx);
            });
        }
    }

    fn save_project_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(work) = self.work.as_ref() else {
            return;
        };
        let name = work.settings_name.read(cx).value();
        let port_raw = work.settings_port.read(cx).value();
        let success = work.settings_success.read(cx).value();
        let fail = work.settings_fail.read(cx).value();
        let headers_raw = work.settings_headers.read(cx).value();
        let Some(port) = parse_port(port_raw.as_str()) else {
            window.push_notification(Notification::error("端口无效"), cx);
            return;
        };
        let mut project = work.project.clone();
        project.name = name.trim().to_string();
        project.port = port;
        project.success_code = trimmed_or(success.as_str(), "0000");
        project.fail_code = trimmed_or(fail.as_str(), "9999");
        project.default_headers = parse_headers(headers_raw.as_str(), &project.default_headers);
        match self.service.save_project(project.clone()) {
            Ok(()) => {
                if let Some(work) = self.work.as_mut() {
                    work.project = project;
                    work.show_settings = false;
                }
            }
            Err(err) => {
                window.push_notification(Notification::error(err.to_string()), cx);
            }
        }
    }

    fn select_work_endpoint(
        &mut self,
        endpoint_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.load_work_endpoint(&endpoint_id, window, cx);
    }

    fn load_work_endpoint(
        &mut self,
        endpoint_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(work) = self.work.as_mut() else {
            return;
        };
        let Some(ep) = work.endpoints.iter().find(|e| e.id == endpoint_id).cloned() else {
            return;
        };
        let scene = self
            .service
            .get_scene(&ep.id, ep.current_scene)
            .ok()
            .flatten();
        let json_text = scene.map(|s| s.body_json).unwrap_or_default();
        let method = method_static(&ep.method);
        work.suppress_save = true;
        work.selected_id = Some(ep.id.clone());
        work.enabled = ep.enabled;
        work.current_scene = ep.current_scene;
        work.json_invalid = serde_json::from_str::<serde_json::Value>(&json_text).is_err();
        work.name.update(cx, |input, cx| {
            input.set_value(ep.name.clone(), window, cx);
        });
        work.path.update(cx, |input, cx| {
            input.set_value(ep.path.clone(), window, cx);
        });
        work.json.update(cx, |input, cx| {
            input.set_value(json_text, window, cx);
        });
        work.method.update(cx, |sel, cx| {
            sel.set_selected_value(&method, window, cx);
        });
        let ep_id = ep.id.clone();
        if let Some(work) = self.work.as_mut() {
            work.rebuild_fields(&self.service, &ep_id, window, cx);
            work.suppress_save = false;
        }
    }

    fn save_work_endpoint(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(work) = self.work.as_ref() else {
            return;
        };
        if work.suppress_save {
            return;
        }
        let Some(id) = work.selected_id.clone() else {
            return;
        };
        let Some(mut ep) = work.endpoints.iter().find(|e| e.id == id).cloned() else {
            return;
        };
        ep.name = work.name.read(cx).value().to_string();
        ep.path = work.path.read(cx).value().to_string();
        ep.enabled = work.enabled;
        ep.current_scene = work.current_scene;
        ep.method = work
            .method
            .read(cx)
            .selected_value()
            .copied()
            .unwrap_or("POST")
            .to_string();
        match self.service.save_endpoint(ep.clone()) {
            Ok(()) => {
                if let Some(work) = self.work.as_mut() {
                    if let Some(slot) = work.endpoints.iter_mut().find(|e| e.id == id) {
                        *slot = ep;
                    }
                }
            }
            Err(err) => {
                window.push_notification(Notification::error(err.to_string()), cx);
            }
        }
    }

    fn save_work_json(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(work) = self.work.as_ref() else {
            return;
        };
        if work.suppress_save {
            return;
        }
        let Some(id) = work.selected_id.clone() else {
            return;
        };
        let kind = work.current_scene;
        let body = work.json.read(cx).value().to_string();
        let invalid = serde_json::from_str::<serde_json::Value>(&body).is_err();
        if let Some(work) = self.work.as_mut() {
            work.json_invalid = invalid;
        }
        if let Err(err) = self.service.save_scene_body(&id, kind, body) {
            window.push_notification(Notification::error(err.to_string()), cx);
        }
        cx.notify();
    }

    fn save_work_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(work) = self.work.as_ref() else {
            return;
        };
        if work.suppress_save {
            return;
        }
        let Some(id) = work.selected_id.clone() else {
            return;
        };
        let fields = work.collect_fields(cx);
        if let Err(err) = self.service.save_fields(&id, fields) {
            window.push_notification(Notification::error(err.to_string()), cx);
        }
    }

    fn set_work_enabled(&mut self, enabled: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(work) = self.work.as_mut() {
            work.enabled = enabled;
        }
        self.save_work_endpoint(window, cx);
    }

    fn set_work_scene(&mut self, kind: SceneKind, window: &mut Window, cx: &mut Context<Self>) {
        let Some(work) = self.work.as_mut() else {
            return;
        };
        let Some(id) = work.selected_id.clone() else {
            return;
        };
        if let Err(err) = self.service.set_scene(&id, kind) {
            window.push_notification(Notification::error(err.to_string()), cx);
            return;
        }
        if let Some(work) = self.work.as_mut() {
            work.current_scene = kind;
            if let Some(ep) = work.endpoints.iter_mut().find(|e| e.id == id) {
                ep.current_scene = kind;
            }
        }
        let json_text = self
            .service
            .get_scene(&id, kind)
            .ok()
            .flatten()
            .map(|s| s.body_json)
            .unwrap_or_default();
        if let Some(work) = self.work.as_mut() {
            work.suppress_save = true;
            work.json_invalid = serde_json::from_str::<serde_json::Value>(&json_text).is_err();
            work.json.update(cx, |input, cx| {
                input.set_value(json_text, window, cx);
            });
            work.suppress_save = false;
        }
    }

    fn delete_work_endpoint(
        &mut self,
        endpoint_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(err) = self.service.delete_endpoint(&endpoint_id) {
            window.push_notification(Notification::error(err.to_string()), cx);
            return;
        }
        let Some(work) = self.work.as_mut() else {
            return;
        };
        let was_selected = work.selected_id.as_deref() == Some(endpoint_id.as_str());
        work.endpoints.retain(|ep| ep.id != endpoint_id);
        if !was_selected {
            return;
        }
        if let Some(next_id) = work.endpoints.first().map(|ep| ep.id.clone()) {
            self.load_work_endpoint(&next_id, window, cx);
        } else if let Some(work) = self.work.as_mut() {
            work.selected_id = None;
            work.fields.clear();
            work._field_subs.clear();
        }
    }

    fn create_work_endpoint(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(work) = self.work.as_ref() else {
            return;
        };
        let project_id = work.project_id.clone();
        match self.service.create_endpoint(&project_id) {
            Ok(ep) => {
                let id = ep.id.clone();
                if let Some(work) = self.work.as_mut() {
                    work.endpoints.push(ep);
                }
                self.load_work_endpoint(&id, window, cx);
            }
            Err(err) => {
                window.push_notification(Notification::error(err.to_string()), cx);
            }
        }
    }

    fn add_work_field(&mut self, loc: FieldLoc, window: &mut Window, cx: &mut Context<Self>) {
        let Some(work) = self.work.as_ref() else {
            return;
        };
        if work.suppress_save {
            return;
        }
        let Some(endpoint_id) = work.selected_id.clone() else {
            return;
        };
        let mut fields = work.collect_fields(cx);
        fields.push(Field {
            id: store::new_id(),
            endpoint_id: endpoint_id.clone(),
            location: loc,
            name: String::new(),
            name_zh: String::new(),
            type_name: "String".into(),
            required: false,
            comment: String::new(),
            enum_values: Vec::new(),
            parent_id: None,
        });
        if let Err(err) = self.service.save_fields(&endpoint_id, fields) {
            window.push_notification(Notification::error(err.to_string()), cx);
            return;
        }
        if let Some(work) = self.work.as_mut() {
            work.rebuild_fields(&self.service, &endpoint_id, window, cx);
        }
        if loc == FieldLoc::Response {
            self.sync_json_from_response_fields(window, cx);
        }
    }

    fn delete_work_field(&mut self, field_id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(work) = self.work.as_ref() else {
            return;
        };
        let Some(endpoint_id) = work.selected_id.clone() else {
            return;
        };
        let was_response = work
            .fields
            .iter()
            .any(|r| r.field.id == field_id && r.field.location == FieldLoc::Response);
        let fields: Vec<Field> = work
            .collect_fields(cx)
            .into_iter()
            .filter(|f| f.id != field_id)
            .collect();
        if let Err(err) = self.service.save_fields(&endpoint_id, fields) {
            window.push_notification(Notification::error(err.to_string()), cx);
            return;
        }
        if let Some(work) = self.work.as_mut() {
            work.rebuild_fields(&self.service, &endpoint_id, window, cx);
        }
        if was_response {
            self.sync_json_from_response_fields(window, cx);
        }
    }

    fn set_work_field_required(
        &mut self,
        field_id: String,
        required: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(work) = self.work.as_mut() {
            if let Some(row) = work.fields.iter_mut().find(|r| r.field.id == field_id) {
                row.required = required;
                row.field.required = required;
            }
        }
        self.save_work_fields(window, cx);
    }

    fn confirm_regenerate_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_json_from_response_fields(window, cx);
    }

    fn sync_json_from_response_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(work) = self.work.as_ref() else {
            return;
        };
        if work.suppress_save {
            return;
        }
        let Some(id) = work.selected_id.clone() else {
            return;
        };
        let kind = work.current_scene;
        if let Err(err) = self
            .service
            .regenerate_scene_from_fields(&id, SceneKind::Success)
        {
            window.push_notification(Notification::error(err.to_string()), cx);
            return;
        }
        if kind == SceneKind::Empty {
            if let Err(err) = self
                .service
                .regenerate_scene_from_fields(&id, SceneKind::Empty)
            {
                window.push_notification(Notification::error(err.to_string()), cx);
                return;
            }
        }
        self.reload_work_json(window, cx);
    }

    fn reload_work_json(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(work) = self.work.as_ref() else {
            return;
        };
        let Some(id) = work.selected_id.clone() else {
            return;
        };
        let kind = work.current_scene;
        let json_text = self
            .service
            .get_scene(&id, kind)
            .ok()
            .flatten()
            .map(|s| s.body_json)
            .unwrap_or_default();
        if let Some(work) = self.work.as_mut() {
            work.suppress_save = true;
            work.json_invalid = serde_json::from_str::<serde_json::Value>(&json_text).is_err();
            work.json.update(cx, |input, cx| {
                input.set_value(json_text, window, cx);
            });
            work.suppress_save = false;
        }
        cx.notify();
    }

    fn regenerate_success_ai(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(work) = self.work.as_mut() else {
            return;
        };
        if work.regenerating {
            return;
        }
        let Some(ep_id) = work.selected_id.clone() else {
            return;
        };
        let Some(ep) = work.endpoints.iter().find(|e| e.id == ep_id).cloned() else {
            return;
        };
        let project = work.project.clone();
        let fields = work.collect_fields(cx);
        let settings = match self.service.load_settings() {
            Ok(s) => s,
            Err(err) => {
                window.push_notification(Notification::error(err.to_string()), cx);
                return;
            }
        };
        let sources = scan_sources(&home_dir());
        let Some(client) = client_for_selection(&settings, &sources) else {
            window.push_notification(Notification::error("去设置里选择模型来源"), cx);
            return;
        };
        let current_json = work.json.read(cx).value().to_string();
        let prompt = semantic_values_prompt(
            &project.success_code,
            &ep.name,
            &ep.method,
            &ep.path,
            &fields,
            &current_json,
        );
        work.regenerating = true;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { client.complete_json(&prompt) })
                .await;
            this.update_in(cx, |view, window, cx| {
                if let Some(work) = view.work.as_mut() {
                    work.regenerating = false;
                }
                match result {
                    Ok(raw) => match view.service.apply_generated_success(&ep_id, &raw) {
                        Ok(()) => {
                            view.reload_work_json(window, cx);
                            window.push_notification(
                                Notification::success("已用 AI 填入语义化示例值"),
                                cx,
                            );
                        }
                        Err(err) => {
                            window.push_notification(Notification::error(err.to_string()), cx);
                        }
                    },
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

    fn clear_work_logs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(work) = self.work.as_ref() else {
            return;
        };
        let id = work.project_id.clone();
        if let Err(err) = self.service.clear_logs(&id) {
            window.push_notification(Notification::error(err.to_string()), cx);
            return;
        }
        if let Some(work) = self.work.as_mut() {
            work.logs.clear();
        }
    }

    fn open_logs_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_work_logs();
        let logs = self
            .work
            .as_ref()
            .map(|w| w.logs.clone())
            .unwrap_or_default();
        let view = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, window, cx| {
            let view = view.clone();
            let vh = f32::from(window.viewport_size().height);
            let list_h = px(vh.mul_add(0.52, 0.).clamp(220., 420.));
            dialog
                .title("请求日志")
                .w(px(760.))
                .max_h(px((vh * 0.78).clamp(360., 620.)))
                .child(
                    v_flex()
                        .w_full()
                        .gap_2()
                        .max_h(px((vh * 0.62).clamp(260., 480.)))
                        .child(
                            h_flex().w_full().justify_end().child(
                                Button::new("dlg-clear-logs")
                                    .ghost()
                                    .small()
                                    .label("清空")
                                    .on_click(move |_, window, cx| {
                                        let _ = view.update(cx, |this, cx| {
                                            this.clear_work_logs(window, cx);
                                            cx.notify();
                                        });
                                        window.close_dialog(cx);
                                    }),
                            ),
                        )
                        .child(log_dialog_body(&logs, list_h, cx)),
                )
                .alert()
                .button_props(DialogButtonProps::default().ok_text("关闭"))
        });
    }
}

fn method_index(method: &str) -> usize {
    METHODS
        .iter()
        .position(|m| m.eq_ignore_ascii_case(method))
        .unwrap_or(1)
}

fn method_static(method: &str) -> &'static str {
    METHODS
        .iter()
        .copied()
        .find(|m| m.eq_ignore_ascii_case(method))
        .unwrap_or("POST")
}

fn path_tail(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
}

fn scene_label(kind: SceneKind) -> &'static str {
    match kind {
        SceneKind::Success => "成功",
        SceneKind::Empty => "空数据",
        SceneKind::ParamError => "参数错误",
        SceneKind::BusinessError => "业务失败",
    }
}

fn scene_log_label(scene: Option<&str>) -> String {
    match scene {
        Some("success") => "成功".into(),
        Some("empty") => "空数据".into(),
        Some("param_error") => "参数错误".into(),
        Some("business_error") => "业务失败".into(),
        Some(other) => other.to_string(),
        None => "-".into(),
    }
}

fn short_time(at: &str) -> String {
    if let Some(t) = at.split('T').nth(1) {
        t.chars().take(8).collect()
    } else {
        at.to_string()
    }
}

fn header_keys_text(headers: &[HeaderKv]) -> String {
    headers
        .iter()
        .map(|h| h.key.as_str())
        .filter(|k| !k.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn parse_headers(raw: &str, previous: &[HeaderKv]) -> Vec<HeaderKv> {
    raw.split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|key| HeaderKv {
            key: key.to_string(),
            value: previous
                .iter()
                .find(|h| h.key == key)
                .map(|h| h.value.clone())
                .unwrap_or_default(),
        })
        .collect()
}

fn trimmed_or(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn field_path_label(field: &Field, all: &[FieldRow]) -> String {
    if let Some(pid) = &field.parent_id {
        all.iter()
            .find(|r| r.field.id == *pid)
            .map(|r| r.field.name.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "data".into())
    } else {
        "data".into()
    }
}

#[allow(dead_code)]
fn fields_as_paste(ep: &Endpoint, fields: &[Field]) -> String {
    let mut out = format!("{} {} {}\n", ep.method, ep.path, ep.name);
    for field in fields {
        let loc = match field.location {
            FieldLoc::Header => "header",
            FieldLoc::Body => "body",
            FieldLoc::Response => "response",
        };
        out.push_str(&format!(
            "{loc} {} {} {}\n",
            field.name, field.name_zh, field.type_name
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{parse_headers, path_tail, scene_log_label, short_time};
    use crate::domain::HeaderKv;

    #[test]
    fn path_tail_uses_last_segment() {
        assert_eq!(
            path_tail("/hl/pub/phone/v1/queryPhoneHomeData"),
            "queryPhoneHomeData"
        );
        assert_eq!(path_tail("/foo/"), "foo");
        assert_eq!(path_tail("/"), "/");
    }

    #[test]
    fn short_time_from_rfc3339() {
        assert_eq!(short_time("2026-09-03T16:41:02+00:00"), "16:41:02");
    }

    #[test]
    fn scene_log_label_maps_kinds() {
        assert_eq!(scene_log_label(Some("empty")), "空数据");
        assert_eq!(scene_log_label(None), "-");
    }

    #[test]
    fn parse_headers_keeps_previous_values() {
        let prev = vec![HeaderKv {
            key: "sn".into(),
            value: "keep".into(),
        }];
        let got = parse_headers("sn, token", &prev);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].key, "sn");
        assert_eq!(got[0].value, "keep");
        assert_eq!(got[1].key, "token");
        assert!(got[1].value.is_empty());
    }
}
