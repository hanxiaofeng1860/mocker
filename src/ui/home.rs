use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    dialog::DialogButtonProps,
    h_flex,
    notification::Notification,
    tag::Tag,
    v_flex, ActiveTheme as _, Sizable as _, StyledExt as _, WindowExt as _,
};

use crate::domain::Project;
use crate::service::AppService;

use super::app::AppView;
use super::style;

pub(super) fn empty(cx: &mut Context<AppView>) -> AnyElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_4()
        .child(div().text_xl().font_semibold().child("还没有项目"))
        .child(
            div()
                .max_w(px(440.))
                .text_color(cx.theme().muted_foreground)
                .child("从设计文档到可请求的接口，只隔一个本地端口。"),
        )
        .child(
            Button::new("empty-new-project")
                .primary()
                .label("新建项目")
                .on_click(cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.go_new_project(window, cx);
                    cx.notify();
                })),
        )
        .into_any_element()
}

pub(super) fn home(service: &AppService, cx: &mut Context<AppView>) -> AnyElement {
    let projects = service.list_projects().unwrap_or_default();
    let cards: Vec<ProjectCard> = projects
        .into_iter()
        .map(|project| {
            let running = service.is_running(&project.id);
            let endpoints = service
                .list_endpoints(&project.id)
                .map(|eps| eps.len())
                .unwrap_or(0);
            ProjectCard {
                project,
                running,
                endpoints,
            }
        })
        .collect();
    let total = cards.len();
    let running_n = cards.iter().filter(|c| c.running).count();
    let running_cards: Vec<ProjectCard> = cards.iter().filter(|c| c.running).cloned().collect();

    v_flex()
        .size_full()
        .gap_3()
        .child(
            h_flex()
                .w_full()
                .flex_shrink_0()
                .gap_3()
                .child(div().text_xl().font_semibold().child("项目"))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{total} 个 · {running_n} 个运行中")),
                )
                .child(div().flex_1())
                .child(
                    Button::new("home-new-project")
                        .primary()
                        .small()
                        .label("新建项目")
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.go_new_project(window, cx);
                            cx.notify();
                        })),
                ),
        )
        .child(
            v_flex()
                .id("home-projects")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(style::section_label("运行中", cx))
                .child(card_grid("running", &running_cards, cx))
                .child(style::section_label("全部项目", cx))
                .child(card_grid("all", &cards, cx)),
        )
        .into_any_element()
}

#[derive(Clone)]
struct ProjectCard {
    project: Project,
    running: bool,
    endpoints: usize,
}

fn card_grid(section: &str, cards: &[ProjectCard], cx: &mut Context<AppView>) -> impl IntoElement {
    let section = section.to_string();
    div().w_full().grid().grid_cols(2).gap_3().children(
        cards
            .iter()
            .map(|card| project_card(&section, card, cx))
            .collect::<Vec<_>>(),
    )
}

fn project_card(section: &str, card: &ProjectCard, cx: &mut Context<AppView>) -> impl IntoElement {
    let id = card.project.id.clone();
    let name = card.project.name.clone();
    let port = card.project.port;
    let endpoints = card.endpoints;
    let running = card.running;
    let badge = if running {
        Tag::success().small().child("运行中")
    } else {
        Tag::secondary().small().child("已停止")
    };

    style::hover_lift(
        style::card(cx)
            .id(SharedString::from(format!("project-card-{section}-{id}")))
            .w_full()
            .gap_2()
            .p_4()
            .cursor_pointer()
            .child(
                h_flex()
                    .w_full()
                    .items_start()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_lg()
                            .font_semibold()
                            .child(name.clone()),
                    )
                    .child(
                        Button::new(SharedString::from(format!("del-project-{section}-{id}")))
                            .ghost()
                            .small()
                            .danger()
                            .label("删除")
                            .on_click(cx.listener({
                                let id = id.clone();
                                let name = name.clone();
                                move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.confirm_delete_project(
                                        id.clone(),
                                        name.clone(),
                                        running,
                                        window,
                                        cx,
                                    );
                                }
                            })),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(badge)
                    .child(
                        div()
                            .font_family(cx.theme().mono_font_family.clone())
                            .child(format!(":{port}")),
                    )
                    .child(format!("{endpoints} 个接口")),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_work(id.clone());
                cx.notify();
            })),
        cx,
    )
}

impl AppView {
    fn confirm_delete_project(
        &mut self,
        id: String,
        name: String,
        running: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        let hint = if running {
            format!("「{name}」正在运行，删除前会先停止。接口和场景都会一起删掉，且无法恢复。")
        } else {
            format!("删除「{name}」后，接口和场景都会一起删掉，且无法恢复。")
        };
        window.open_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            let id = id.clone();
            let hint = hint.clone();
            dialog
                .title("删除项目")
                .child(hint)
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("删除")
                        .cancel_text("取消")
                        .ok_variant(ButtonVariant::Danger),
                )
                .on_ok(move |_, window, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.delete_home_project(&id, window, cx);
                        cx.notify();
                    });
                    true
                })
        });
    }

    fn delete_home_project(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(err) = self.service.delete_project(id) {
            window.push_notification(Notification::error(err.to_string()), cx);
            return;
        }
        self.go_home_or_empty();
        window.push_notification(Notification::success("项目已删除"), cx);
    }
}
