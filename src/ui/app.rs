use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    v_flex, ActiveTheme as _, Root, Sizable as _, TitleBar,
};

use crate::llm::{LlmError, ModelClient};
use crate::service::AppService;
use crate::store::Store;

use super::home;
use super::import_view::{self, ImportState};
use super::new_project::{self, NewProjectForm};
use super::settings::{self, SettingsState};
use super::style;
use super::workbench::{self, WorkbenchState};

#[derive(Clone, Debug)]
pub enum Screen {
    Empty,
    Home,
    NewProject,
    Work { project_id: String },
    Import { project_id: String },
    Settings { back: Box<Screen> },
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
    pub(super) settings: Option<SettingsState>,
    pub(super) work: Option<WorkbenchState>,
    pub(super) import: Option<ImportState>,
}

impl AppView {
    pub fn new() -> Result<Self> {
        let path = mocker_db_path()?;
        let store = Arc::new(Mutex::new(Store::open(path)?));
        let service = AppService::new(store, Unconfigured);
        settings::apply_saved_client(&service);
        let screen = match service.list_projects() {
            Ok(projects) if projects.is_empty() => Screen::Empty,
            _ => Screen::Home,
        };
        Ok(Self {
            service,
            screen,
            new_project: None,
            settings: None,
            work: None,
            import: None,
        })
    }

    pub(super) fn go_new_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_project = Some(NewProjectForm::new(window, cx));
        self.screen = Screen::NewProject;
    }

    pub(super) fn go_home_or_empty(&mut self) {
        self.new_project = None;
        self.work = None;
        self.import = None;
        self.screen = match self.service.list_projects() {
            Ok(projects) if projects.is_empty() => Screen::Empty,
            _ => Screen::Home,
        };
    }

    pub(super) fn open_work(&mut self, project_id: String) {
        self.new_project = None;
        if self
            .import
            .as_ref()
            .is_some_and(|s| s.project_id != project_id)
        {
            self.import = None;
        }
        if self
            .work
            .as_ref()
            .is_none_or(|w| w.project_id != project_id)
        {
            self.work = None;
        }
        self.screen = Screen::Work { project_id };
    }

    fn render_body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if matches!(self.screen, Screen::NewProject) && self.new_project.is_none() {
            self.new_project = Some(NewProjectForm::new(window, cx));
        }
        if matches!(self.screen, Screen::Settings { .. }) && self.settings.is_none() {
            self.settings = Some(SettingsState::new(&self.service, window, cx));
        }
        if let Screen::Work { project_id } = &self.screen {
            let pid = project_id.clone();
            if self.work.as_ref().is_none_or(|w| w.project_id != pid) {
                self.work = None;
                self.work = Some(WorkbenchState::new(&self.service, pid, window, cx));
            }
        }
        if let Screen::Import { project_id } = &self.screen {
            let pid = project_id.clone();
            if self.import.as_ref().is_none_or(|s| s.project_id != pid) {
                self.import = Some(ImportState::new(pid, window, cx));
            }
        }
        match &self.screen {
            Screen::Empty => home::empty(cx),
            Screen::Home => home::home(&self.service, cx),
            Screen::NewProject => match self.new_project.as_ref() {
                Some(form) => new_project::view(form, cx),
                None => div().into_any_element(),
            },
            Screen::Work { .. } => match self.work.as_ref() {
                Some(state) => workbench::view(state, &self.service, cx),
                None => div().into_any_element(),
            },
            Screen::Import { .. } => match self.import.as_ref() {
                Some(state) => import_view::view(state, &self.service, cx),
                None => div().into_any_element(),
            },
            Screen::Settings { .. } => match self.settings.as_ref() {
                Some(state) => settings::view(state, cx),
                None => div().into_any_element(),
            },
        }
    }
}

fn screen_anim_id(screen: &Screen) -> SharedString {
    match screen {
        Screen::Empty => "screen-empty".into(),
        Screen::Home => "screen-home".into(),
        Screen::NewProject => "screen-new-project".into(),
        Screen::Work { project_id } => format!("screen-work-{project_id}").into(),
        Screen::Import { project_id } => format!("screen-import-{project_id}").into(),
        Screen::Settings { .. } => "screen-settings".into(),
    }
}

fn mocker_db_path() -> Result<std::path::PathBuf> {
    // Spec: ~/Library/Application Support/Mocker/mocker.db
    let dirs = directories::ProjectDirs::from("", "", "Mocker")
        .ok_or_else(|| anyhow!("cannot resolve Application Support"))?;
    Ok(dirs.data_dir().join("mocker.db"))
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let anim_id = screen_anim_id(&self.screen);
        // Root itself does not paint Dialog/Sheet/Notification; the app view
        // must attach those layers or open_dialog is a no-op on screen.
        div()
            .relative()
            .size_full()
            .child(
                v_flex()
                    .size_full()
                    .font_family(cx.theme().font_family.clone())
                    .bg(cx.theme().background)
                    .text_color(cx.theme().foreground)
                    .child(
                        TitleBar::new().child("Mocker").child(
                            Button::new("settings")
                                .ghost()
                                .small()
                                .label("设置")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_settings(window, cx);
                                    cx.notify();
                                })),
                        ),
                    )
                    .child(
                        // Do not put buttons in overflow_y_scroll: GPUI scroll views
                        // steal mouse hits, so 新建项目 looked dead.
                        v_flex()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .px_6()
                            .py_5()
                            .child(style::fade_in(anim_id, self.render_body(window, cx))),
                    ),
            )
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}
