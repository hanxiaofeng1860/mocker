use std::time::Duration;

use gpui::*;
use gpui_component::{
    animation::cubic_bezier, box_shadow, v_flex, ActiveTheme as _, StyledExt as _,
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
