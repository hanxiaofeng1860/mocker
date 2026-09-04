use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    form::{field, v_form},
    h_flex,
    input::{Input, InputState},
    notification::Notification,
    v_flex, Sizable as _, StyledExt as _, WindowExt as _,
};

use crate::domain::HeaderKv;

use super::app::AppView;
use super::style;

pub(super) struct NewProjectForm {
    name: Entity<InputState>,
    port: Entity<InputState>,
    success_code: Entity<InputState>,
    fail_code: Entity<InputState>,
    header_key: Entity<InputState>,
}

impl NewProjectForm {
    pub(super) fn new(window: &mut Window, cx: &mut Context<AppView>) -> Self {
        Self {
            name: cx.new(|cx| InputState::new(window, cx).placeholder("例如：智能话机")),
            port: cx.new(|cx| InputState::new(window, cx).placeholder("7788")),
            success_code: cx.new(|cx| InputState::new(window, cx).default_value("0000")),
            fail_code: cx.new(|cx| InputState::new(window, cx).default_value("9999")),
            header_key: cx.new(|cx| InputState::new(window, cx).placeholder("sn")),
        }
    }
}

pub(super) fn parse_port(raw: &str) -> Option<u16> {
    raw.trim().parse().ok().filter(|port| *port != 0)
}

pub(super) fn view(form: &NewProjectForm, cx: &mut Context<AppView>) -> AnyElement {
    v_flex()
        .w_full()
        .max_w(px(720.))
        .gap_3()
        .child(
            h_flex()
                .gap_3()
                .child(
                    Button::new("back-new-project")
                        .ghost()
                        .small()
                        .label("← 项目")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.go_home_or_empty();
                            cx.notify();
                        })),
                )
                .child(div().text_xl().font_semibold().child("新建项目")),
        )
        .child(
            style::card(cx)
                .w_full()
                .gap_4()
                .p_5()
                .child(
                    v_form()
                        .columns(2)
                        .child(
                            field()
                                .label("项目名称")
                                .col_span(2)
                                .child(Input::new(&form.name).w_full()),
                        )
                        .child(field().label("端口").child(Input::new(&form.port).w_full()))
                        .child(
                            field()
                                .label("成功码")
                                .child(Input::new(&form.success_code).w_full()),
                        )
                        .child(
                            field()
                                .label("失败码")
                                .child(Input::new(&form.fail_code).w_full()),
                        )
                        .child(
                            field()
                                .label("默认请求头（可选，如 sn）")
                                .col_span(2)
                                .child(Input::new(&form.header_key).w_full()),
                        ),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("create-project")
                                .primary()
                                .label("创建并打开")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.submit_new_project(window, cx);
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("cancel-new-project")
                                .ghost()
                                .label("取消")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.go_home_or_empty();
                                    cx.notify();
                                })),
                        ),
                ),
        )
        .into_any_element()
}

impl AppView {
    pub(super) fn submit_new_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.new_project.as_ref() else {
            return;
        };
        let name = form.name.read(cx).value();
        let port_raw = form.port.read(cx).value();
        let success = form.success_code.read(cx).value();
        let fail = form.fail_code.read(cx).value();
        let header = form.header_key.read(cx).value();

        let Some(port) = parse_port(port_raw.as_str()) else {
            window.push_notification(Notification::error("端口无效"), cx);
            return;
        };

        let success = trimmed_or(&success, "0000");
        let fail = trimmed_or(&fail, "9999");
        let headers = match header.trim() {
            "" => Vec::new(),
            key => vec![HeaderKv {
                key: key.to_string(),
                value: String::new(),
            }],
        };

        match self
            .service
            .create_project(name.trim(), port, &success, &fail, headers)
        {
            Ok(project) => self.open_work(project.id),
            Err(err) => {
                window.push_notification(Notification::error(err.to_string()), cx);
            }
        }
    }
}

fn trimmed_or(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::parse_port;

    #[test]
    fn parse_port_accepts_positive_u16() {
        assert_eq!(parse_port("7788"), Some(7788));
        assert_eq!(parse_port(" 1 "), Some(1));
        assert_eq!(parse_port("65535"), Some(65535));
    }

    #[test]
    fn parse_port_rejects_invalid() {
        assert_eq!(parse_port(""), None);
        assert_eq!(parse_port("0"), None);
        assert_eq!(parse_port("abc"), None);
        assert_eq!(parse_port("65536"), None);
        assert_eq!(parse_port("-1"), None);
    }
}
