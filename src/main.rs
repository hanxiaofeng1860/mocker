use std::sync::OnceLock;

use gpui::*;
use gpui_component::{Root, TitleBar};
use mocker::ui::{apply_paper_theme, AppView};

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

fn main() {
    // RuntimeHub prefers Handle::try_current(); enter before GPUI.
    let _enter = tokio_runtime().enter();

    Application::new().run(|cx| {
        gpui_component::init(cx);
        apply_paper_theme(cx);
        let app = AppView::new().expect("open mocker.db");
        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    titlebar: Some(TitleBar::title_bar_options()),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(|_| app);
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("open window");
        })
        .detach();
    });
}
