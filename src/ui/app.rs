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

use super::home;
use super::new_project::{self, NewProjectForm};

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
    pub(super) service: AppService,
    pub(super) screen: Screen,
    pub(super) new_project: Option<NewProjectForm>,
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
        Ok(Self {
            service,
            screen,
            new_project: None,
        })
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

    pub(super) fn go_new_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_project = Some(NewProjectForm::new(window, cx));
        self.screen = Screen::NewProject;
    }

    pub(super) fn go_home_or_empty(&mut self) {
        self.new_project = None;
        self.screen = match self.service.list_projects() {
            Ok(projects) if projects.is_empty() => Screen::Empty,
            _ => Screen::Home,
        };
    }

    pub(super) fn open_work(&mut self, project_id: String) {
        self.new_project = None;
        self.screen = Screen::Work { project_id };
    }

    fn render_body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if matches!(self.screen, Screen::NewProject) && self.new_project.is_none() {
            self.new_project = Some(NewProjectForm::new(window, cx));
        }
        match &self.screen {
            Screen::Empty => home::empty(cx),
            Screen::Home => home::home(&self.service, cx),
            Screen::NewProject => match self.new_project.as_ref() {
                Some(form) => new_project::view(form, cx),
                None => div().into_any_element(),
            },
            Screen::Work { project_id } => work_stub(project_id, cx),
            other => div().child(other.placeholder()).into_any_element(),
        }
    }
}

fn work_stub(project_id: &str, cx: &mut Context<AppView>) -> AnyElement {
    v_flex()
        .gap_3()
        .child(
            Button::new("back-work")
                .ghost()
                .label("← 项目")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.go_home_or_empty();
                    cx.notify();
                })),
        )
        .child(format!("Work {project_id}"))
        .into_any_element()
}

fn mocker_db_path() -> Result<std::path::PathBuf> {
    // Spec: ~/Library/Application Support/Mocker/mocker.db
    let dirs = directories::ProjectDirs::from("", "", "Mocker")
        .ok_or_else(|| anyhow!("cannot resolve Application Support"))?;
    Ok(dirs.data_dir().join("mocker.db"))
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_5()
                    .child(self.render_body(window, cx)),
            )
    }
}
