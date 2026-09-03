use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    v_flex, ActiveTheme as _, TitleBar,
};

use crate::llm::{LlmError, ModelClient};
use crate::service::AppService;
use crate::store::Store;

#[derive(Clone, Debug)]
pub enum Screen {
    Empty,
    Home,
    NewProject,
    Work { project_id: String },
    Import { project_id: String },
    Settings { back: Box<Screen> },
}

impl Screen {
    fn placeholder(&self) -> SharedString {
        match self {
            Self::Empty => "Empty".into(),
            Self::Home => "Home".into(),
            Self::NewProject => "NewProject".into(),
            Self::Work { project_id } => format!("Work {project_id}").into(),
            Self::Import { project_id } => format!("Import {project_id}").into(),
            Self::Settings { .. } => "Settings".into(),
        }
    }
}

struct Unconfigured;

impl ModelClient for Unconfigured {
    fn complete_json(&self, _prompt: &str) -> Result<String, LlmError> {
        Err(LlmError::Request("not configured".into()))
    }
}

pub struct AppView {
    #[allow(dead_code)] // Task 9+ screens call into the service.
    service: AppService,
    screen: Screen,
}

impl AppView {
    pub fn new() -> Result<Self> {
        let path = mocker_db_path()?;
        let store = Arc::new(Mutex::new(Store::open(path)?));
        let service = AppService::new(store, Unconfigured);
        let screen = match service.list_projects() {
            Ok(projects) if projects.is_empty() => Screen::Empty,
            _ => Screen::Home,
        };
        Ok(Self { service, screen })
    }

    fn open_settings(&mut self) {
        if matches!(self.screen, Screen::Settings { .. }) {
            return;
        }
        let back = self.screen.clone();
        self.screen = Screen::Settings {
            back: Box::new(back),
        };
    }
}

fn mocker_db_path() -> Result<std::path::PathBuf> {
    // Spec: ~/Library/Application Support/Mocker/mocker.db
    let dirs = directories::ProjectDirs::from("", "", "Mocker")
        .ok_or_else(|| anyhow!("cannot resolve Application Support"))?;
    Ok(dirs.data_dir().join("mocker.db"))
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                TitleBar::new().child("Mocker").child(
                    Button::new("settings")
                        .ghost()
                        .label("设置")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open_settings();
                            cx.notify();
                        })),
                ),
            )
            .child(
                div()
                    .id("body")
                    .flex_1()
                    .p_4()
                    .child(self.screen.placeholder()),
            )
    }
}
