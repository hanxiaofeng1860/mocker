use std::time::Duration;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    animation::cubic_bezier, box_shadow, h_flex, tag::Tag, v_flex, ActiveTheme as _, Colorize as _,
    Sizable as _, StyledExt as _,
};

pub fn fade_in(id: impl Into<ElementId>, child: impl IntoElement) -> impl IntoElement {
    div()
        .flex_1()
        .min_h_0()
        .size_full()
        .child(child)
        .with_animation(
            id,
            Animation::new(Duration::from_millis(280))
                .with_easing(cubic_bezier(0.22, 1.0, 0.36, 1.0)),
            |this, delta| this.opacity(delta),
        )
}

pub fn appear(id: impl Into<SharedString>, child: impl IntoElement) -> impl IntoElement {
    let id = id.into();
    div().id(id.clone()).w_full().child(child).with_animation(
        id,
        Animation::new(Duration::from_millis(240)).with_easing(cubic_bezier(0.22, 1.0, 0.36, 1.0)),
        |this, delta| this.opacity(delta).mt((1.0 - delta) * px(10.)),
    )
}

pub fn paper_shadow(cx: &App) -> Vec<BoxShadow> {
    let c = ink(cx);
    if cx.theme().is_dark() {
        vec![
            box_shadow(0., 1., 2., 0., c.opacity(0.42)),
            box_shadow(0., 4., 12., 0., c.opacity(0.28)),
        ]
    } else {
        vec![
            box_shadow(0., 1., 2., 0., c.opacity(0.10)),
            box_shadow(0., 3., 10., 0., c.opacity(0.07)),
        ]
    }
}

pub fn paper_shadow_lifted(cx: &App) -> Vec<BoxShadow> {
    let c = ink(cx);
    if cx.theme().is_dark() {
        vec![
            box_shadow(0., 2., 6., 0., c.opacity(0.50)),
            box_shadow(0., 10., 28., 0., c.opacity(0.34)),
        ]
    } else {
        vec![
            box_shadow(0., 2., 6., 0., c.opacity(0.12)),
            box_shadow(0., 10., 24., 0., c.opacity(0.10)),
        ]
    }
}

pub fn card(cx: &App) -> Div {
    v_flex()
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().popover)
        .shadow(paper_shadow(cx))
}

pub fn sheet(cx: &App) -> Div {
    v_flex()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().popover)
        .shadow(paper_shadow(cx))
}

pub fn sidebar_panel(cx: &App) -> Div {
    v_flex()
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().muted.opacity(0.45))
        .shadow(paper_shadow(cx))
}

/// 侧栏条目选中：未选中无描边，选中为墨绿底 + 描边。
pub fn selected_nav<T>(this: T, selected: bool, cx: &App) -> T
where
    T: Styled + InteractiveElement + FluentBuilder,
{
    this.border_1()
        .border_color(if selected {
            cx.theme().primary.opacity(0.55)
        } else {
            cx.theme().transparent
        })
        .bg(if selected {
            cx.theme().list_active
        } else {
            cx.theme().transparent
        })
        .when(selected, |this| this.shadow(paper_shadow(cx)))
        .when(!selected, |this| this.hover(|s| s.bg(cx.theme().list_hover)))
}

/// 设置来源等卡片式选中。
pub fn selected_card<T>(this: T, selected: bool, cx: &App) -> T
where
    T: Styled + InteractiveElement + FluentBuilder,
{
    this.border_1()
        .border_color(if selected {
            cx.theme().primary.opacity(0.55)
        } else {
            cx.theme().border
        })
        .bg(if selected {
            cx.theme().list_active
        } else {
            cx.theme().popover
        })
        .when(selected, |this| this.shadow(paper_shadow(cx)))
        .when(!selected, |this| this.hover(|s| s.bg(cx.theme().list_hover)))
}

/// 表格行选中底色（对勾由 `checkbox_mark` 表示）。
pub fn selected_row<T>(this: T, selected: bool, cx: &App) -> T
where
    T: Styled + InteractiveElement + FluentBuilder,
{
    this.bg(if selected {
        cx.theme().list_active
    } else {
        cx.theme().transparent
    })
    .when(!selected, |this| this.hover(|s| s.bg(cx.theme().list_hover)))
}

/// 方框 checkbox：未选中空框，选中显示对勾。不依赖未打包的 SVG 图标。
pub fn checkbox_mark(checked: bool, cx: &App) -> Div {
    h_flex()
        .size(px(18.))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .border_1()
        .border_color(if checked {
            cx.theme().primary
        } else {
            cx.theme().border
        })
        .bg(if checked {
            cx.theme().primary
        } else {
            cx.theme().background
        })
        .when(checked, |this| {
            this.child(
                div()
                    .text_color(cx.theme().primary_foreground)
                    .text_sm()
                    .font_medium()
                    .line_height(px(16.))
                    .child("✓"),
            )
        })
}

pub fn selected_caption(selected: bool, text: &str) -> String {
    if selected {
        format!("✓ {text}")
    } else {
        text.to_string()
    }
}

pub fn method_badge(method: &str, cx: &App) -> impl IntoElement {
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

pub fn hover_lift<T>(this: T, cx: &App) -> T
where
    T: InteractiveElement,
{
    let lifted = paper_shadow_lifted(cx);
    let border = cx.theme().primary.opacity(0.28);
    this.hover(move |style| style.shadow(lifted.clone()).border_color(border))
}

pub fn section_label(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .mt_4()
        .mb_1()
        .text_xs()
        .font_medium()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

pub fn pulse_dot(id: impl Into<ElementId>, color: Hsla) -> impl IntoElement {
    div()
        .w(px(6.))
        .h(px(6.))
        .rounded(px(99.))
        .bg(color)
        .with_animation(
            id,
            Animation::new(Duration::from_secs_f64(1.8))
                .repeat()
                .with_easing(pulsating_between(0.35, 1.0)),
            |this, delta| this.opacity(delta),
        )
}

fn ink(cx: &App) -> Hsla {
    if cx.theme().is_dark() {
        hsla(0., 0., 0., 1.)
    } else {
        hsla(30. / 360., 0.45, 0.18, 1.)
    }
}
