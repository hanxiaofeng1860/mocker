use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants},
    v_flex, Root,
};

struct Hello;

impl Render for Hello {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .child("Mocker")
            .child(Button::new("ok").primary().label("Let's Go!"))
    }
}

fn main() {
    Application::new().run(|cx| {
        gpui_component::init(cx);
        cx.spawn(async move |cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let view = cx.new(|_| Hello);
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("open window");
        })
        .detach();
    });
}
