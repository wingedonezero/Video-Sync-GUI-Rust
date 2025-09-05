// src/lib.rs

pub mod core;
pub mod ui;

use iced::{executor, Application, Command, Element, Settings, Size, Theme, window::{self, Id}};
use std::path::{Path, PathBuf};

use crate::core::config::AppConfig;
use crate::core::pipeline::{Job, JobPipeline};
use crate::core::job_discovery;
use crate::ui::manual_selection_dialog;


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
}

#[derive(Debug, Clone)]
pub enum Message {
    RefPathChanged(String),
    SecPathChanged(String),
    TerPathChanged(String),
    BrowseRef,
    BrowseSec,
    BrowseTer,
    AutoApplyToggled(bool),
    AutoApplyStrictToggled(bool),
    ArchiveLogsToggled(bool),
    AnalyzeOnlyClicked,
    AnalyzeAndMergeClicked,
    RefFileSelected(Option<PathBuf>),
    SecFileSelected(Option<PathBuf>),
    TerFileSelected(Option<PathBuf>),
    SettingsClicked,
    JobsDiscovered(Result<Vec<Job>, String>),
    JobFinished(Result<String, String>),
}

impl iced::multi_window::Application for VsgApp {
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
        };
        (app, Command::none())
    }

    fn title(&self, _window: Id) -> String {
        String::from("Video Sync & Merge - Rust Edition")
    }

    fn view(&self, _id: Id) -> Element<Message> {
        ui::main_window::view(self)
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::RefPathChanged(path) => self.ref_path = path,
            Message::SecPathChanged(path) => self.sec_path = path,
            Message::TerPathChanged(path) => self.ter_path = path,

            Message::BrowseRef => return Command::perform(open_file_or_dir(), Message::RefFileSelected),
            Message::BrowseSec => return Command::perform(open_file_or_dir(), Message::SecFileSelected),
            Message::BrowseTer => return Command::perform(open_file_or_dir(), Message::TerFileSelected),

            Message::RefFileSelected(Some(path)) => self.ref_path = path.to_string_lossy().into_owned(),
            Message::SecFileSelected(Some(path)) => self.sec_path = path.to_string_lossy().into_owned(),
            Message::TerFileSelected(Some(path)) => self.ter_path = path.to_string_lossy().into_owned(),

            Message::AutoApplyToggled(val) => self.auto_apply_layout = val,
            Message::AutoApplyStrictToggled(val) => self.auto_apply_strict = val,
            Message::ArchiveLogsToggled(val) => self.archive_logs = val,

            Message::AnalyzeOnlyClicked | Message::AnalyzeAndMergeClicked => {
                let and_merge = matches!(message, Message::AnalyzeAndMergeClicked);
                self.status_text = "Discovering jobs...".to_string();
                let ref_path = self.ref_path.clone();
                let sec_path = self.sec_path.clone();
                let ter_path = self.ter_path.clone();

                return Command::perform(async move {
                    job_discovery::discover_jobs(&ref_path, &sec_path, &ter_path)
                }, Message::JobsDiscovered(and_merge)); // Pass and_merge context
            }

            // This now needs to know whether to run a merge or not
            Message::JobsDiscovered(Ok(jobs), and_merge) => {
                if jobs.is_empty() {
                    self.status_text = "No matching jobs found.".to_string();
                } else {
                    self.status_text = format!("Found {} job(s). Running first one...", jobs.len());
                    let first_job = jobs.into_iter().next().unwrap();
                    let config = self.config.clone();
                    return Command::perform(run_job_task(first_job, config, and_merge), Message::JobFinished);
                }
            }
            Message::JobsDiscovered(Err(e), _) => {
                self.status_text = format!("Error: {}", e);
            }

            Message::SettingsClicked => {
                self.log_output.push("EVENT: Settings button clicked.".to_string());
            }

            Message::JobFinished(Ok(result)) => {
                self.status_text = "Job Finished Successfully".to_string();
                self.log_output.push("SUCCESS: Job finished.".to_string());
                self.log_output.push(result);
            }
            Message::JobFinished(Err(error)) => {
                self.status_text = "Job Failed".to_string();
                self.log_output.push(format!("ERROR: {}", error));
            }
            _ => {}
        }
        Command::none()
    }

    fn theme(&self) -> Self::Theme {
        Theme::Oxocarbon
    }
}

async fn run_job_task(job: Job, config: AppConfig, and_merge: bool) -> Result<String, String> {
    if !Path::new(&job.ref_file).exists() {
        return Err("Reference file path is empty or does not exist.".to_string());
    }
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    tokio::spawn(async move {
        while let Some(log_line) = rx.recv().await {
            println!("[LOG] {}", log_line);
        }
    });
    let pipeline = JobPipeline::new(config, tx);
    pipeline.run_job(&job, and_merge).await
}

async fn open_file_or_dir() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
    .set_title("Select a file or directory")
    .pick_file().await
    .map(|h| h.path().to_path_buf())
}
