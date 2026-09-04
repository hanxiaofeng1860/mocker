use std::sync::OnceLock;

use gpui::*;
use gpui_component::{Root, TitleBar};
use mocker::ui::{apply_paper_theme, load_app_fonts, AppView};
use mocker::window_geom::fit_window_size;

fn tokio_runtime() -> &'static tokio::runtime::Runtime {
    static TOKIO: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    TOKIO.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("tokio runtime")
    })
}

fn usable_work_area(cx: &App) -> Bounds<Pixels> {
    #[cfg(target_os = "macos")]
    if let Some(frame) = macos_visible_frame() {
        return frame;
    }
    cx.primary_display()
        .map(|display| display.bounds())
        .unwrap_or_else(|| Bounds {
            origin: point(px(0.), px(0.)),
            size: size(
                px(mocker::window_geom::DESIGN_WIDTH),
                px(mocker::window_geom::DESIGN_HEIGHT),
            ),
        })
}

fn default_window_bounds(cx: &App) -> Bounds<Pixels> {
    let work = usable_work_area(cx);
    let window_size = fit_window_size(work.size);
    let origin = point(
        work.origin.x + (work.size.width - window_size.width) / 2.,
        work.origin.y + (work.size.height - window_size.height) / 2.,
    );
    Bounds {
        origin,
        size: window_size,
    }
}

#[cfg(target_os = "macos")]
fn macos_visible_frame() -> Option<Bounds<Pixels>> {
    unsafe {
        use cocoa::appkit::NSScreen;
        use cocoa::base::nil;
        use cocoa::foundation::NSArray;

        let screens = NSScreen::screens(nil);
        if screens == nil || NSArray::count(screens) == 0 {
            return None;
        }
        let screen = NSArray::objectAtIndex(screens, 0);
        let full = NSScreen::frame(screen);
        let visible = NSScreen::visibleFrame(screen);
        // AppKit origin is bottom-left; GPUI window bounds are top-left of the display.
        let top_inset =
            (full.origin.y + full.size.height) - (visible.origin.y + visible.size.height);
        let left_inset = visible.origin.x - full.origin.x;
        Some(Bounds {
            origin: point(px(left_inset as f32), px(top_inset as f32)),
            size: size(
                px(visible.size.width as f32),
                px(visible.size.height as f32),
            ),
        })
    }
}

fn main() {
    // RuntimeHub prefers Handle::try_current(); enter before GPUI.
    let _enter = tokio_runtime().enter();

    Application::new().run(|cx| {
        gpui_component::init(cx);
        load_app_fonts(cx);
        apply_paper_theme(cx);
        let app = AppView::new().expect("open mocker.db");
        let bounds = default_window_bounds(cx);
        cx.open_window(
            WindowOptions {
                titlebar: Some(TitleBar::title_bar_options()),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(|_| app);
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("open window");
    });
}
