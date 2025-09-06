// src/lib.rs

pub mod core;
pub mod ui;

use iced::widget::container;
use iced::{
    executor, window, Element, Settings, Size, Subscription, Task, Theme,
};
use std::collections::HashMap;

use crate::core::config::AppConfig;
use crate::core::job_discovery;
use crate::core::mkv_utils::Track;
use crate::core::pipeline::{Job, JobPipeline, TrackSelection};
use crate::ui::manual_selection_dialog::{self, ManualSelection};
use crate::ui::options_dialog::{self, OptionsDialog};

pub fn run() -> iced::Result {
    iced::application("Video Sync & Merge - Rust Edition", update, view)
    .theme(theme)
    .subscription(subscription)
    .window(window::Settings {
        size: Size::new(1000.0, 750.0),
            min_size: Some(Size::new(800.0, 600.0)),
            resizable: true,
            ..Default::default()
    })
    .run()
}

#[derive(Default)]
pub struct VsgApp {
    config: AppConfig,
    pub ref_path: String,
    pub sec_path: String,
    pub ter_path: String,
    pub auto_apply_layout: bool,
    pub auto_apply_strict: bool,
    pub archive_logs: bool,
    pub status_text: String,
    pub progress: f32,
    pub sec_delay_text: String,
    pub ter_delay_text: String,
    pub log_output: Vec<String>,
    pub is_running: bool,

    // State for managing modals and jobs
    pub manual_selection: Option<ManualSelection>,
    pub options_dialog: Option<OptionsDialog>,
    pub active_jobs: Vec<Job>,
    pub initial_layout: Option<Vec<TrackSelection>>,
}

#[derive(Debug, Clone)]
pub enum Message {
    RefPathChanged(String),
    SecPathChanged(String),
    TerPathChanged(String),
    BrowseRef,
    BrowseSec,
    BrowseTer,
    RefFileSelected(Option<std::path::PathBuf>),
    SecFileSelected(Option<std::path::PathBuf>),
    TerFileSelected(Option<std::path::PathBuf>),
    AutoApplyToggled(bool),
    AutoApplyStrictToggled(bool),
    ArchiveLogsToggled(bool),

    StartJob(bool), // bool is for `and_merge`
    JobsDiscovered(Result<Vec<Job>, String>, bool),

    OpenSettings,
    OptionsMessage(options_dialog::DialogMessage),
    ManualSelectionMessage(manual_selection_dialog::DialogMessage),

    // Background job messages (same as before; we will rehook the stream next step)
    JobStarted(String),  // name
    JobLog(String),
    JobProgress(f32),
    JobFinished(String), // summary
    BatchFinished,
}

// ---------- State initialization (moved from Application::new) ----------
fn init_state() -> VsgApp {
    let config = AppConfig::load();
    config.ensure_dirs_exist();

    VsgApp {
        ref_path: config.last_ref_path.clone(),
        sec_path: config.last_sec_path.clone(),
        ter_path: config.last_ter_path.clone(),
        auto_apply_layout: false,
        auto_apply_strict: config.auto_apply_strict,
        archive_logs: config.archive_logs,
        status_text: "Ready".to_string(),
        progress: 0.0,
        sec_delay_text: "—".to_string(),
        ter_delay_text: "—".to_string(),
        log_output: vec!["Welcome to Video Sync & Merge!".to_string()],
        config,
        is_running: false,
        manual_selection: None,
        options_dialog: None,
        active_jobs: Vec::new(),
        initial_layout: None,
    }
}

// ---------- Functional API: update/view/subscription/title/theme ----------

fn update(state: &mut VsgApp, message: Message) -> Task<Message> {
    // Lazy-initialize once (iced 0.13 uses Default + functional run)
    // This ensures we load config even if Default was used
    if state.status_text.is_empty() && state.log_output.is_empty() {
        *state = init_state();
    }

    match message {
        Message::RefPathChanged(path) => {
            state.ref_path = path;
            Task::none()
        }
        Message::SecPathChanged(path) => {
            state.sec_path = path;
            Task::none()
        }
        Message::TerPathChanged(path) => {
            state.ter_path = path;
            Task::none()
        }

        // ---------- Options dialog wiring ----------
        Message::OpenSettings => {
            state.options_dialog = Some(OptionsDialog::new(state.config.clone()));
            Task::none()
        }

        Message::OptionsMessage(options_dialog::DialogMessage::Save) => {
            if let Some(dialog) = state.options_dialog.take() {
                state.config = dialog.pending_config;
                state.config.save();
                state.config.ensure_dirs_exist();

                // Keep UI flags in sync with config immediately
                state.archive_logs = state.config.archive_logs;
                state.auto_apply_strict = state.config.auto_apply_strict;

                state.status_text = "Settings saved.".to_string();
            }
            Task::none()
        }

        Message::OptionsMessage(options_dialog::DialogMessage::Cancel) => {
            state.options_dialog = None;
            state.status_text = "Settings unchanged.".to_string();
            Task::none()
        }

        Message::OptionsMessage(other) => {
            if let Some(dialog) = &mut state.options_dialog {
                dialog.update(other);
            }
            Task::none()
        }
        // -------------------------------------------

        Message::StartJob(and_merge) => {
            if state.is_running {
                return Task::none();
            }
            state.log_output.clear();
            state.progress = 0.0;
            state.status_text = "Discovering jobs...".to_string();

            let ref_path = state.ref_path.clone();
            let sec_path = state.sec_path.clone();
            let ter_path = state.ter_path.clone();

            Task::perform(
                async move { job_discovery::discover_jobs(&ref_path, &sec_path, &ter_path) },
                          move |res| Message::JobsDiscovered(res, and_merge),
            )
        }

        Message::JobsDiscovered(Ok(jobs), and_merge) => {
            if jobs.is_empty() {
                state.status_text = "No matching jobs found.".to_string();
                return Task::none();
            }

            state.status_text = format!("Found {} job(s).", jobs.len());
            state.active_jobs = jobs;

            if and_merge {
                let first_job = state.active_jobs[0].clone();
                state.manual_selection = Some(ManualSelection::new(first_job));
                // Kick off async load
                return state.manual_selection.as_mut().unwrap().on_load();
            } else {
                state.is_running = true;
                // NOTE: We will re-enable the streaming worker via Subscription in the next step.
                Task::none()
            }
        }

        Message::JobsDiscovered(Err(e), _) => {
            state.status_text = format!("Error: {}", e);
            Task::none()
        }

        Message::ManualSelectionMessage(dialog_msg) => {
            if let Some(dialog_state) = &mut state.manual_selection {
                match dialog_state.update(dialog_msg) {
                    Some(manual_selection_dialog::DialogResult::Ok(layout)) => {
                        state.manual_selection = None;
                        state.initial_layout = Some(layout);
                        state.is_running = true; // This will activate the subscription in next step
                    }
                    Some(manual_selection_dialog::DialogResult::Cancel) => {
                        state.manual_selection = None;
                        state.status_text = "Ready".to_string();
                    }
                    None => {}
                }
            }
            Task::none()
        }

        Message::JobStarted(name) => {
            state.status_text = format!("Processing: {}", name);
            Task::none()
        }
        Message::JobLog(log) => {
            state.log_output.push(log);
            Task::none()
        }
        Message::JobProgress(val) => {
            state.progress = val;
            Task::none()
        }
        Message::JobFinished(summary) => {
            state.log_output.push(summary);
            Task::none()
        }
        Message::BatchFinished => {
            state.is_running = false;
            state.active_jobs.clear();
            state.initial_layout = None;
            state.status_text = "Batch Finished".to_string();
            state.progress = 100.0;
            Task::none()
        }

        // Placeholders; your other UI messages (Browse*, toggles, etc.) route here if needed.
        _ => Task::none(),
    }
}

fn view(state: &VsgApp) -> Element<Message> {
    // Render main window
    let base = crate::ui::main_window::view(state);

    // Modal: Options dialog
    if let Some(d) = &state.options_dialog {
        return crate::ui::options_dialog::view(d);
    }
    base
}

fn subscription(state: &VsgApp) -> Subscription<Message> {
    // TEMP: return none so we compile cleanly with iced 0.13.
    // Next step: migrate your previous `subscription::unfold` worker
    // to `Subscription::run` to restore streaming logs/progress.
    if state.is_running {
        // TODO: implement with `Subscription::run` (next step)
        Subscription::none()
    } else {
        Subscription::none()
    }
}

fn theme(_state: &VsgApp) -> Theme {
    Theme::Oxocarbon
}
