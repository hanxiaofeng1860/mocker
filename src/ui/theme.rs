use gpui::{rgb, App, Hsla};
use gpui_component::{Colorize as _, Theme, ThemeMode};

fn hsla(hex: u32) -> Hsla {
    rgb(hex).into()
}

pub fn apply_paper_theme(cx: &mut App) {
    // init() follows system appearance; leftover tokens must stay light.
    Theme::change(ThemeMode::Light, None, cx);
    let theme = Theme::global_mut(cx);
    let background = hsla(0xF2EBDC);
    let primary = hsla(0x3A5A40);
    let border = hsla(0xD4C8B0);
    let accent = hsla(0xA4453B);
    theme.background = background;
    theme.foreground = hsla(0x2A2620);
    theme.primary = primary;
    theme.primary_foreground = hsla(0xFBF6EA);
    theme.primary_hover = primary.darken(0.08);
    theme.border = border;
    theme.accent = accent;
    theme.input = border;
    theme.list = background;
    theme.list_active = primary.opacity(0.16);
    theme.danger = accent;
    // TitleBar reads these, not `background`.
    theme.title_bar = background;
    theme.title_bar_border = border;
}

pub fn apply_ink_theme(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    let theme = Theme::global_mut(cx);
    let background = hsla(0x1E1E1E);
    let primary = hsla(0x3A5A40);
    let border = hsla(0x3A3A3A);
    let accent = hsla(0xA4453B);
    theme.background = background;
    theme.foreground = hsla(0xD4D4D4);
    theme.primary = primary;
    theme.primary_foreground = hsla(0xFBF6EA);
    theme.primary_hover = primary.lighten(0.08);
    theme.border = border;
    theme.accent = accent;
    theme.input = border;
    theme.list = background;
    theme.list_active = primary.opacity(0.24);
    theme.danger = accent;
    theme.title_bar = background;
    theme.title_bar_border = border;
}
