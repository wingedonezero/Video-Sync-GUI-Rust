// src/lib.rs

pub mod core;
pub mod ui;

use iced::{executor, Application, Command, Element, Settings, Size, Theme, window::{self, Id}};
use std::path::{Path, PathBuf};
use tokio::fs;

use crate::core::config::AppConfig;
use crate::core::pipeline::{Job, JobPipeline, TrackSelection};
use crate::core::job_discovery;
use crate::ui::manual_selection_dialog;
use crate::core::{mkv_utils, process};


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
    JobsDiscovered(Result<Vec<Job>, String>, bool),
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
                }, move |res| Message::JobsDiscovered(res, and_merge));
            }

            Message::JobsDiscovered(Ok(jobs), and_merge) => {
                if jobs.is_empty() {
                    self.status_text = "No matching jobs found.".to_string();
                } else if and_merge {
                    self.status_text = format!("Found {} job(s). Opening selection dialog...", jobs.len());
                    let first_job = jobs.into_iter().next().unwrap();
                    return window::spawn(
                        Id::new("manual_selection"),
                                         window::Settings { size: Size::new(1000.0, 600.0), ..Default::default() },
                                         move |_id| manual_selection_dialog::ManualSelection::new(first_job),
                    );
                } else {
                    self.status_text = format!("Found {} job(s). Running analysis on first one...", jobs.len());
                    self.log_output.push("INFO: Analysis-only job started.".to_string());
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
    let log_clone = tx.clone();
    tokio::spawn(async move { while let Some(log) = rx.recv().await { println!("[LOG] {}", log); }});

    // Create a temporary directory for the placeholder layout's extracted files
    let temp_dir = PathBuf::from(&config.temp_root).join(format!("layout_test_{}", chrono::Utc::now().timestamp()));
    fs::create_dir_all(&temp_dir).await.map_err(|e|e.to_string())?;

    let runner = process::CommandRunner::new(config.clone(), log_clone);
    let mut placeholder_layout = create_test_layout(&job, &runner, &temp_dir).await?;

    let pipeline = JobPipeline::new(config, tx);
    let result = pipeline.run_job(&job, and_merge, &mut placeholder_layout).await;

    fs::remove_dir_all(&temp_dir).await.ok();
    result
}

// Helper to create a detailed placeholder layout for testing the full pipeline
async fn create_test_layout(job: &Job, runner: &process::CommandRunner, temp_dir: &Path) -> Result<Vec<TrackSelection>, String> {
    let mut layout = Vec::new();

    let ref_info = mkv_utils::get_stream_info(runner, &job.ref_file).await?;
    let ref_tracks_to_extract: Vec<_> = ref_info.tracks.iter().cloned().collect();
    let ref_extracted = mkv_utils::extract_tracks(runner, Path::new(&job.ref_file), &ref_tracks_to_extract, temp_dir, "ref").await?;

    let mut ref_video_default = true;
    let mut ref_audio_default = true;
    for extracted in ref_extracted {
        let track_type = &extracted.original_track.r#type;
        let mut is_default = false;
        if track_type == "video" && ref_video_default {
            is_default = true;
            ref_video_default = false;
        } else if track_type == "audio" && ref_audio_default {
            is_default = true;
            ref_audio_default = false;
        }
        layout.push(TrackSelection {
            source: "REF".to_string(), extracted_path: extracted.path, is_default, is_forced: false,
                    apply_track_name: true, convert_to_ass: false, rescale: false, size_multiplier: 1.0,
                    original_track: extracted.original_track,
        });
    }

    if let Some(sec_file) = &job.sec_file {
        let sec_info = mkv_utils::get_stream_info(runner, sec_file).await?;
        let sec_extracted = mkv_utils::extract_tracks(runner, Path::new(sec_file), &sec_info.tracks, temp_dir, "sec").await?;
        for extracted in sec_extracted {
            layout.push(TrackSelection {
                source: "SEC".to_string(), extracted_path: extracted.path, is_default: false, is_forced: false,
                        apply_track_name: true, convert_to_ass: false, rescale: false, size_multiplier: 1.0,
                        original_track: extracted.original_track,
            });
        }
    }

    Ok(layout)
}

async fn open_file_or_dir() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
    .set_title("Select a file or directory")
    .pick_file().await
    .map(|h| h.path().to_path_buf())
}
