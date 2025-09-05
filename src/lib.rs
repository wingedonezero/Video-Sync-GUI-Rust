// src/lib.rs

pub mod core;
pub mod ui;

use iced::widget::container;
use iced::{
    executor, subscription, window, Application, Command, Element, Length, Settings, Size, Theme,
    Subscription,
};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::mpsc;

use crate::core::config::AppConfig;
use crate::core::job_discovery;
use crate::core::mkv_utils::{self, Track};
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

    // State for managing modals and jobs
    manual_selection: Option<ManualSelection>,
    options_dialog: Option<OptionsDialog>,
    active_jobs: Vec<Job>,
    initial_layout: Option<Vec<TrackSelection>>,
}

#[derive(Debug, Clone)]
pub enum Message {
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

    StartJob(bool), // bool is for `and_merge`
    JobsDiscovered(Result<Vec<Job>, String>, bool),

    OpenSettings,
    OptionsMessage(options_dialog::DialogMessage),
    ManualSelectionMessage(manual_selection_dialog::DialogMessage),

    // Messages produced by the background job Subscription
    JobStarted(String), // name
    JobLog(String),
    JobProgress(f32),
    JobFinished(String), // summary
    BatchFinished,
}

impl Application for VsgApp {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
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
            active_jobs: Vec::new(),
            initial_layout: None,
        };
        (app, Command::none())
    }

    fn title(&self) -> String {
        String::from("Video Sync & Merge - Rust Edition")
    }

    fn view(&self) -> Element<Message> {
        // ... (this function is unchanged from the previous version)
        // If your `ui::main_window::view` returns the whole main UI, render it here:
        let base = crate::ui::main_window::view(self);

        // Overlay the Options dialog if open
        if let Some(d) = &self.options_dialog {
            // If you prefer a layered overlay, you can implement a stack in your main view.
            // For now, showing the dialog is enough.
            return crate::ui::options_dialog::view(d);
        }

        base
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::RefPathChanged(path) => self.ref_path = path,
            Message::SecPathChanged(path) => self.sec_path = path,
            // ... (other UI control messages are unchanged)

            // ---------- Options dialog wiring (Open / Save / Cancel / live updates) ----------
            Message::OpenSettings => {
                self.options_dialog = Some(OptionsDialog::new(self.config.clone()));
                return Command::none();
            }

            Message::OptionsMessage(options_dialog::DialogMessage::Save) => {
                if let Some(dialog) = self.options_dialog.take() {
                    self.config = dialog.pending_config;
                    self.config.save();
                    self.config.ensure_dirs_exist();

                    // Keep UI flags in sync with config immediately
                    self.archive_logs = self.config.archive_logs;
                    self.auto_apply_strict = self.config.auto_apply_strict;

                    self.status_text = "Settings saved.".to_string();
                }
                return Command::none();
            }

            Message::OptionsMessage(options_dialog::DialogMessage::Cancel) => {
                self.options_dialog = None;
                self.status_text = "Settings unchanged.".to_string();
                return Command::none();
            }

            Message::OptionsMessage(other) => {
                if let Some(dialog) = &mut self.options_dialog {
                    dialog.update(other);
                }
                return Command::none();
            }
            // -------------------------------------------------------------------------------

            Message::StartJob(and_merge) => {
                if self.is_running {
                    return Command::none();
                }
                self.log_output.clear();
                self.progress = 0.0;
                self.status_text = "Discovering jobs...".to_string();

                let ref_path = self.ref_path.clone();
                let sec_path = self.sec_path.clone();
                let ter_path = self.ter_path.clone();

                return Command::perform(
                    async move { job_discovery::discover_jobs(&ref_path, &sec_path, &ter_path) },
                                        move |res| Message::JobsDiscovered(res, and_merge),
                );
            }

            Message::JobsDiscovered(Ok(jobs), and_merge) => {
                if jobs.is_empty() {
                    self.status_text = "No matching jobs found.".to_string();
                    return Command::none();
                }

                self.status_text = format!("Found {} job(s).", jobs.len());
                self.active_jobs = jobs;

                if and_merge {
                    let first_job = self.active_jobs[0].clone();
                    self.manual_selection = Some(ManualSelection::new(first_job));
                    return self.manual_selection.as_mut().unwrap().on_load();
                } else {
                    self.is_running = true;
                }
            }

            Message::JobsDiscovered(Err(e), _) => self.status_text = format!("Error: {}", e),

            Message::ManualSelectionMessage(dialog_msg) => {
                if let Some(dialog_state) = &mut self.manual_selection {
                    match dialog_state.update(dialog_msg) {
                        Some(manual_selection_dialog::DialogResult::Ok(layout)) => {
                            self.manual_selection = None;
                            self.initial_layout = Some(layout);
                            self.is_running = true; // This will activate the subscription
                        }
                        Some(manual_selection_dialog::DialogResult::Cancel) => {
                            self.manual_selection = None;
                            self.status_text = "Ready".to_string();
                        }
                        None => {}
                    }
                }
            }

            Message::JobStarted(name) => self.status_text = format!("Processing: {}", name),
            Message::JobLog(log) => self.log_output.push(log),
            Message::JobProgress(val) => self.progress = val,
            Message::JobFinished(summary) => self.log_output.push(summary),
            Message::BatchFinished => {
                self.is_running = false;
                self.active_jobs.clear();
                self.initial_layout = None;
                self.status_text = "Batch Finished".to_string();
                self.progress = 100.0;
            }

            _ => {} // Other messages
        }
        Command::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        if self.is_running {
            JobWorker::subscription(
                self.config.clone(),
                                    self.active_jobs.clone(),
                                    self.initial_layout.clone(), // This is `None` for analyze-only
                                    self.auto_apply_layout,
                                    self.auto_apply_strict,
            )
        } else {
            Subscription::none()
        }
    }

    fn theme(&self) -> Self::Theme {
        Theme::Oxocarbon
    }
}

// ... (open_file_or_dir is unchanged)

// --- Background Job Worker ---

struct JobWorker;

impl JobWorker {
    fn subscription(
        config: AppConfig,
        jobs: Vec<Job>,
        initial_layout: Option<Vec<TrackSelection>>,
        auto_apply: bool,
        auto_apply_strict: bool,
    ) -> Subscription<Message> {
        subscription::unfold(
            "job-worker",
            WorkerState::new(config, jobs, initial_layout, auto_apply, auto_apply_strict),
                             |mut state| async move {
                                 if let Some(job) = state.jobs.pop() {
                                     let (sender, mut receiver) = mpsc::channel(100);
                                     let pipeline = JobPipeline::new(state.config.clone(), sender.clone());

                                     sender
                                     .send(Message::JobStarted(job.ref_file.clone()))
                                     .await
                                     .ok();

                                     let layout_result = if let Some(ref initial) = state.initial_layout {
                                         // This is a merge job
                                         get_layout_for_job(&mut state, &job, initial, &sender).await
                                     } else {
                                         // This is analyze-only
                                         Ok(vec![])
                                     };

                                     match layout_result {
                                         Ok(current_layout) => {
                                             let pipeline_future =
                                             pipeline.run_job(&job, state.initial_layout.is_some(), &current_layout);

                                             tokio::select! {
                                                 result = pipeline_future => {
                                                     let summary = match result {
                                                         Ok(s) => format!("[SUCCESS] {}", s),
                             Err(e) => format!("[FAILURE] {}", e),
                                                     };
                                                     sender.send(Message::JobFinished(summary)).await.ok();
                                                 }
                                                 Some(msg) = receiver.recv() => {
                                                     return (Some(msg), state);
                                                 }
                                             }
                                         }
                                         Err(e) => {
                                             sender
                                             .send(Message::JobFinished(format!(
                                                 "[ERROR] Failed to prepare layout: {}",
                                                 e
                                             )))
                                             .await
                                             .ok();
                                         }
                                     }
                                     (receiver.recv().await, state)
                                 } else {
                                     (Some(Message::BatchFinished), state)
                                 }
                             },
        )
    }
}

struct WorkerState {
    config: AppConfig,
    jobs: Vec<Job>,
    initial_layout: Option<Vec<TrackSelection>>,
    auto_apply: bool,
    auto_apply_strict: bool,
    // State for auto-apply
    last_layout_template: Option<Vec<TrackSelection>>,
    last_signature: Option<HashMap<String, usize>>,
}

impl WorkerState {
    fn new(
        config: AppConfig,
        mut jobs: Vec<Job>,
        initial_layout: Option<Vec<TrackSelection>>,
        auto_apply: bool,
        auto_apply_strict: bool,
    ) -> Self {
        jobs.reverse(); // So we can pop from the end
        Self {
            config,
            jobs,
            initial_layout,
            auto_apply,
            auto_apply_strict,
            last_layout_template: None,
            last_signature: None,
        }
    }
}

fn generate_track_signature(tracks: &[Track], strict: bool) -> HashMap<String, usize> {
    let mut signature = HashMap::new();
    for track in tracks {
        let key = if strict {
            format!(
                "{}_{}_{}_{}",
                track.r#type,
                track.source,
                track
                .properties
                .language
                .as_deref()
                .unwrap_or("und"),
                    track
                    .properties
                    .codec_id
                    .as_deref()
                    .unwrap_or("N/A")
            )
        } else {
            format!("{}_{}", track.r#type, track.source)
        };
        *signature.entry(key).or_insert(0) += 1;
    }
    signature
}

async fn get_layout_for_job(
    state: &mut WorkerState,
    job: &Job,
    initial_layout: &[TrackSelection],
    _sender: &mpsc::Sender<Message>,
) -> Result<Vec<TrackSelection>, String> {
    // For now, use the initial layout unchanged (as before).
    Ok(initial_layout.to_vec())
}
