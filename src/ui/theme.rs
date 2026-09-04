use std::borrow::Cow;

use gpui::{px, rgb, transparent_black, App, Hsla, SharedString};
use gpui_component::{scroll::ScrollbarShow, Colorize as _, Theme, ThemeMode};

const UI_FONT: &str = "JetBrains Mono";

fn hsla(hex: u32) -> Hsla {
    rgb(hex).into()
}

pub fn load_app_fonts(cx: &App) {
    let fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!(
            "../../assets/fonts/JetBrainsMono-Regular.ttf"
        )),
        Cow::Borrowed(include_bytes!(
            "../../assets/fonts/JetBrainsMono-Medium.ttf"
        )),
        Cow::Borrowed(include_bytes!(
            "../../assets/fonts/JetBrainsMono-SemiBold.ttf"
        )),
        Cow::Borrowed(include_bytes!("../../assets/fonts/JetBrainsMono-Bold.ttf")),
    ];
    cx.text_system()
        .add_fonts(fonts)
        .expect("load JetBrains Mono");
}

fn apply_ui_font(theme: &mut Theme) {
    let family: SharedString = UI_FONT.into();
    theme.font_family = family.clone();
    theme.mono_font_family = family;
}

pub fn apply_paper_theme(cx: &mut App) {
    // init() follows system appearance; leftover tokens must stay light.
    Theme::change(ThemeMode::Light, None, cx);
    let theme = Theme::global_mut(cx);
    let background = hsla(0xF2EBDC);
    let primary = hsla(0x3A5A40);
    let border = hsla(0xD4C8B0);
    let accent = hsla(0xA4453B);
    let popover = hsla(0xF8F3E8);
    let muted = hsla(0xE8DFD0);
    let foreground = hsla(0x2A2620);
    theme.background = background;
    theme.foreground = foreground;
    theme.primary = primary;
    theme.primary_foreground = hsla(0xFBF6EA);
    theme.primary_hover = primary.darken(0.08);
    theme.primary_active = primary.darken(0.14);
    theme.border = border;
    theme.accent = accent;
    theme.accent_foreground = hsla(0xFBF6EA);
    theme.input = border;
    theme.ring = primary;
    theme.list = background;
    theme.list_hover = muted;
    theme.list_active = primary.opacity(0.16);
    theme.list_even = background;
    theme.danger = accent;
    theme.danger_hover = accent.darken(0.08);
    theme.danger_active = accent.darken(0.14);
    theme.popover = popover;
    theme.popover_foreground = foreground;
    theme.muted = muted;
    theme.muted_foreground = hsla(0x7A6F64);
    theme.secondary = popover;
    theme.secondary_foreground = foreground;
    theme.secondary_hover = muted;
    theme.secondary_active = muted.darken(0.06);
    theme.radius = px(8.);
    theme.radius_lg = px(12.);
    theme.shadow = true;
    // TitleBar reads these, not `background`.
    theme.title_bar = background;
    theme.title_bar_border = border;
    apply_scrollbar(theme, hsla(0x7A6F64));
    apply_ui_font(theme);
}

pub fn apply_ink_theme(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    let theme = Theme::global_mut(cx);
    let background = hsla(0x1E1E1E);
    let primary = hsla(0x3A5A40);
    let border = hsla(0x3A3A3A);
    let accent = hsla(0xA4453B);
    let popover = hsla(0x252525);
    let muted = hsla(0x2A2A2A);
    let foreground = hsla(0xD4D4D4);
    theme.background = background;
    theme.foreground = foreground;
    theme.primary = primary;
    theme.primary_foreground = hsla(0xFBF6EA);
    theme.primary_hover = primary.lighten(0.08);
    theme.primary_active = primary.lighten(0.04);
    theme.border = border;
    theme.accent = accent;
    theme.accent_foreground = hsla(0xFBF6EA);
    theme.input = border;
    theme.ring = primary;
    theme.list = background;
    theme.list_hover = muted;
    theme.list_active = primary.opacity(0.24);
    theme.list_even = background;
    theme.danger = accent;
    theme.danger_hover = accent.lighten(0.08);
    theme.danger_active = accent.lighten(0.04);
    theme.popover = popover;
    theme.popover_foreground = foreground;
    theme.muted = muted;
    theme.muted_foreground = hsla(0x999999);
    theme.secondary = popover;
    theme.secondary_foreground = foreground;
    theme.secondary_hover = muted;
    theme.secondary_active = muted.lighten(0.06);
    theme.radius = px(8.);
    theme.radius_lg = px(12.);
    theme.shadow = true;
    theme.title_bar = background;
    theme.title_bar_border = border;
    apply_scrollbar(theme, hsla(0x999999));
    apply_ui_font(theme);
}

fn apply_scrollbar(theme: &mut Theme, thumb: Hsla) {
    // Track fill flashes a full-height ghost while scrolling if it has alpha.
    // Thumb only: show while scrolling, then fade (library delay ~2s).
    theme.scrollbar = transparent_black();
    theme.scrollbar_thumb = thumb.opacity(0.45);
    theme.scrollbar_thumb_hover = thumb.opacity(0.7);
    theme.scrollbar_show = ScrollbarShow::Scrolling;
}
