// src/lib.rs

pub mod core;
pub mod ui;

use iced::widget::container;
use iced::{executor, Application, Command, Element, Length, Settings, Size, Theme};
use std::path::PathBuf;

use crate::core::config::AppConfig;
use crate::core::job_discovery;
use crate::core::pipeline::{Job, JobPipeline, TrackSelection};
use crate::ui::manual_selection_dialog::{self, ManualSelection};
use crate::ui::options_dialog::{self, OptionsDialog};

pub fn run() -> iced::Result {
    VsgApp::run(Settings {
        window: iced::window::Settings {
            size: Size::new(1000.0, 750.0),
                min_size: Some(Size::new(800.0, 600.0)),
                resizable: true,
                ..Default::default()
        },
        ..Default::default()
    })
}

pub struct VsgApp {
    config: AppConfig,
    ref_path: String,
    sec_path: String,
    ter_path: String,
    auto_apply_layout: bool,
    auto_apply_strict: bool,
    archive_logs: bool,
    status_text: String,
    progress: f32,
    sec_delay_text: String,
    ter_delay_text: String,
    log_output: Vec<String>,
    is_running: bool,

    // State for managing modals
    manual_selection: Option<ManualSelection>,
    options_dialog: Option<OptionsDialog>,
}

#[derive(Debug, Clone)]
pub enum Message {
    // UI controls
    RefPathChanged(String),
    SecPathChanged(String),
    TerPathChanged(String),
    BrowseRef,
    BrowseSec,
    BrowseTer,
    RefFileSelected(Option<PathBuf>),
    SecFileSelected(Option<PathBuf>),
    TerFileSelected(Option<PathBuf>),
    AutoApplyToggled(bool),
    AutoApplyStrictToggled(bool),
    ArchiveLogsToggled(bool),

    // Actions
    StartJob(bool), // bool is for `and_merge`
    JobsDiscovered(Result<Vec<Job>, String>, bool),

    // Dialog Messages
    OpenSettings,
    OptionsMessage(options_dialog::DialogMessage),
    ManualSelectionMessage(manual_selection_dialog::DialogMessage),

    // Job Lifecycle
    RunBatch(Vec<Job>, Vec<TrackSelection>),
    JobLog(String),
    JobProgress(f32),
    BatchFinished(String),
}

impl Application for VsgApp {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme; // UPDATED
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        let config = AppConfig::load();
        config.ensure_dirs_exist();

        let app = Self {
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
        };
        (app, Command::none())
    }

    fn title(&self) -> String {
        String::from("Video Sync & Merge - Rust Edition")
    }

    fn view(&self) -> Element<Message> {
        let main_content = ui::main_window::view(self);

        let modal_overlay = if let Some(dialog_state) = &self.options_dialog {
            Some(ui::options_dialog::view(dialog_state))
        } else if let Some(dialog_state) = &self.manual_selection {
            Some(ui::manual_selection_dialog::view(dialog_state).map(Message::ManualSelectionMessage))
        } else {
            None
        };

        if let Some(modal) = modal_overlay {
            modal
        } else {
            main_content
        }
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            // --- UI Controls ---
            Message::RefPathChanged(path) => self.ref_path = path,
            Message::SecPathChanged(path) => self.sec_path = path,
            Message::TerPathChanged(path) => self.ter_path = path,
            Message::RefFileSelected(Some(path)) => self.ref_path = path.to_string_lossy().into_owned(),
            Message::SecFileSelected(Some(path)) => self.sec_path = path.to_string_lossy().into_owned(),
            Message::TerFileSelected(Some(path)) => self.ter_path = path.to_string_lossy().into_owned(),
            Message::BrowseRef => return Command::perform(open_file_or_dir(), Message::RefFileSelected),
            Message::BrowseSec => return Command::perform(open_file_or_dir(), Message::SecFileSelected),
            Message::BrowseTer => return Command::perform(open_file_or_dir(), Message::TerFileSelected),
            Message::AutoApplyToggled(val) => self.auto_apply_layout = val,
            Message::AutoApplyStrictToggled(val) => self.auto_apply_strict = val,
            Message::ArchiveLogsToggled(val) => self.archive_logs = val,

            // --- Dialog Triggers & Handling ---
            Message::OpenSettings => {
                self.options_dialog = Some(OptionsDialog::new(self.config.clone()));
            }
            Message::OptionsMessage(msg) => {
                if let Some(dialog) = &mut self.options_dialog {
                    match msg {
                        options_dialog::DialogMessage::Save => {
                            self.config = dialog.pending_config.clone();
                            self.config.save();
                            self.options_dialog = None;
                        },
                        options_dialog::DialogMessage::Cancel => {
                            self.options_dialog = None;
                        },
                        _ => dialog.update(msg),
                    }
                }
            }
            Message::ManualSelectionMessage(dialog_msg) => {
                if let Some(dialog_state) = &mut self.manual_selection {
                    match dialog_state.update(dialog_msg) {
                        Some(manual_selection_dialog::DialogResult::Ok(layout)) => {
                            self.manual_selection = None;
                            self.status_text = format!("Layout received with {} tracks. Ready to run.", layout.len());
                            // TODO: Actually start the batch run here
                        },
                        Some(manual_selection_dialog::DialogResult::Cancel) => {
                            self.manual_selection = None;
                            self.status_text = "Ready".to_string();
                            self.is_running = false;
                        },
                        None => {} // Dialog is still open
                    }
                }
            }

            // --- Job Lifecycle ---
            Message::StartJob(and_merge) => {
                if self.is_running { return Command::none(); }
                self.is_running = true;
                self.status_text = "Discovering jobs...".to_string();
                self.log_output.clear();

                let ref_path = self.ref_path.clone();
                let sec_path = self.sec_path.clone();
                let ter_path = self.ter_path.clone();

                return Command::perform(async move {
                    job_discovery::discover_jobs(&ref_path, &sec_path, &ter_path)
                }, move |res| Message::JobsDiscovered(res, and_merge));
            }

            Message::JobsDiscovered(Ok(jobs), and_merge) => {
                if jobs.is_empty() {
                    self.status_text = "No matching jobs found.".to_string();
                    self.is_running = false;
                    return Command::none();
                }

                self.status_text = format!("Found {} job(s).", jobs.len());
                if and_merge {
                    let first_job = jobs[0].clone();
                    self.manual_selection = Some(ManualSelection::new(first_job));
                    return self.manual_selection.as_mut().unwrap().on_load();
                }
            }

            Message::JobsDiscovered(Err(e), _) => {
                self.status_text = format!("Error: {}", e);
                self.is_running = false;
            }

            _ => {}
        }
        Command::none()
    }

    fn theme(&self) -> Self::Theme {
        Theme::Oxocarbon // UPDATED
    }
}

async fn open_file_or_dir() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
    .set_title("Select a file or directory")
    .pick_file()
    .await
    .map(|h| h.path().to_path_buf())
}
