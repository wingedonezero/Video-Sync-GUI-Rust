//! Main application window component.
//!
//! Matches the PySide main window layout:
//! - Settings button (top)
//! - Main Workflow group (job queue button, archive logs checkbox)
//! - Quick Analysis group (3 source inputs with browse, analyze button)
//! - Status bar with progress
//! - Latest Job Results (delay labels)
//! - Log output

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use gtk::gdk;
use gtk::prelude::*;
use relm4::prelude::*;

use vsg_core::config::{ConfigManager, Settings};
use vsg_core::logging::{GuiLogCallback, JobLogger, LogConfig, LogLevel};
use vsg_core::models::JobSpec;
use vsg_core::orchestrator::steps::AnalyzeStep;
use vsg_core::orchestrator::{create_standard_pipeline, CancelHandle, Context, JobState, Pipeline};

use crate::job_queue::{JobQueueDialog, JobQueueMsg, JobQueueOutput};
use crate::settings::{SettingsDialog, SettingsMsg, SettingsOutput};

/// Messages for the main application window.
#[derive(Debug)]
pub enum AppMsg {
    /// Browse for a source file (index 0=Source1, 1=Source2, 2=Source3)
    BrowseSource(usize),
    /// A source path was selected from the file chooser
    SourceSelected(usize, PathBuf),
    /// Start analyze-only
    AnalyzeOnly,
    /// Cancel running analysis
    CancelAnalysis,
    /// Open settings dialog
    OpenSettings,
    /// Settings dialog returned updated settings
    SettingsApplied(Box<Settings>),
    /// Open job queue dialog
    OpenJobQueue,
    /// Job queue requested processing start
    JobQueueStartProcessing(Vec<vsg_core::jobs::JobQueueEntry>),
    /// Append a message to the log
    Log(String),
    /// Update status label
    #[allow(dead_code)]
    Status(String),
    /// Update progress bar (0-100)
    Progress(u32),
    /// Analysis completed (or failed)
    AnalysisDone(Result<AnalysisResult, String>),
    /// One batch job completed
    BatchJobDone {
        job_index: usize,
        total_jobs: usize,
        job_name: String,
        result: Result<BatchJobResult, String>,
    },
    /// Entire batch finished
    BatchComplete,
}

/// Result from a completed analysis.
#[derive(Debug)]
pub struct AnalysisResult {
    pub source2_delay: i64,
    pub source3_delay: i64,
}

/// Result from a completed batch job.
#[derive(Debug)]
pub struct BatchJobResult {
    pub source2_delay: i64,
    pub source3_delay: i64,
    pub output_path: Option<PathBuf>,
}

/// Main application state.
pub struct App {
    /// Configuration manager
    config: ConfigManager,
    /// Base directory (parent of the binary)
    base_dir: PathBuf,
    /// Source file paths
    sources: [String; 3],
    /// Log buffer
    log_text: String,
    /// Status message
    status: String,
    /// Progress percentage
    progress: u32,
    /// Archive logs checkbox state
    archive_logs: bool,
    /// Delay results
    source2_delay: Option<i64>,
    source3_delay: Option<i64>,
    /// Settings dialog child component
    settings_dialog: Controller<SettingsDialog>,
    /// Job queue dialog child component
    job_queue_dialog: Controller<JobQueueDialog>,
    /// Cancel handle for running analysis
    cancel_handle: Option<CancelHandle>,
    /// Whether a job is currently running
    running: bool,
}

/// Resolve the base directory from the binary location.
fn resolve_base_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

/// Format a delay value for display.
fn format_delay(delay: Option<i64>) -> String {
    match delay {
        Some(d) => format!("{d} ms"),
        None => "\u{2014}".to_string(),
    }
}

#[relm4::component(pub)]
impl SimpleComponent for App {
    type Init = ();
    type Input = AppMsg;
    type Output = ();

    view! {
        gtk::ApplicationWindow {
            set_title: Some("Video/Audio Sync & Merge - Rust Edition"),
            set_default_width: 1000,
            set_default_height: 600,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 6,
                set_margin_all: 8,

                // Top row: Settings button
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 6,

                    gtk::Button {
                        set_label: "Settings...",
                        connect_clicked => AppMsg::OpenSettings,
                    },
                },

                // Main Workflow group
                gtk::Frame {
                    set_label: Some("Main Workflow"),

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 6,
                        set_margin_all: 8,

                        gtk::Button {
                            set_label: "Open Job Queue for Merging...",
                            add_css_class: "suggested-action",
                            connect_clicked => AppMsg::OpenJobQueue,
                        },

                        #[name = "archive_logs_check"]
                        gtk::CheckButton {
                            set_label: Some("Archive logs to a zip file on batch completion"),
                            #[watch]
                            set_active: model.archive_logs,
                        },
                    },
                },

                // Quick Analysis group
                gtk::Frame {
                    set_label: Some("Quick Analysis (Analyze Only)"),

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 4,
                        set_margin_all: 8,

                        // Source 1
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 6,

                            gtk::Label {
                                set_label: "Source 1 (Reference):",
                                set_width_request: 160,
                                set_xalign: 0.0,
                            },

                            #[name = "source1_entry"]
                            gtk::Entry {
                                set_hexpand: true,
                                set_placeholder_text: Some("Path to reference file"),
                                #[watch]
                                set_text: &model.sources[0],
                            },

                            gtk::Button {
                                set_label: "Browse...",
                                connect_clicked => AppMsg::BrowseSource(0),
                            },
                        },

                        // Source 2
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 6,

                            gtk::Label {
                                set_label: "Source 2:",
                                set_width_request: 160,
                                set_xalign: 0.0,
                            },

                            #[name = "source2_entry"]
                            gtk::Entry {
                                set_hexpand: true,
                                set_placeholder_text: Some("Path to secondary file"),
                                #[watch]
                                set_text: &model.sources[1],
                            },

                            gtk::Button {
                                set_label: "Browse...",
                                connect_clicked => AppMsg::BrowseSource(1),
                            },
                        },

                        // Source 3
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 6,

                            gtk::Label {
                                set_label: "Source 3:",
                                set_width_request: 160,
                                set_xalign: 0.0,
                            },

                            #[name = "source3_entry"]
                            gtk::Entry {
                                set_hexpand: true,
                                set_placeholder_text: Some("Path to tertiary file (optional)"),
                                #[watch]
                                set_text: &model.sources[2],
                            },

                            gtk::Button {
                                set_label: "Browse...",
                                connect_clicked => AppMsg::BrowseSource(2),
                            },
                        },

                        // Analyze / Cancel buttons (right-aligned)
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_halign: gtk::Align::End,
                            set_spacing: 8,

                            gtk::Button {
                                set_label: "Cancel",
                                #[watch]
                                set_visible: model.running,
                                add_css_class: "destructive-action",
                                connect_clicked => AppMsg::CancelAnalysis,
                            },

                            gtk::Button {
                                set_label: "Analyze Only",
                                #[watch]
                                set_sensitive: !model.running,
                                connect_clicked => AppMsg::AnalyzeOnly,
                            },
                        },
                    },
                },

                // Status row
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,

                    gtk::Label {
                        set_label: "Status:",
                    },

                    #[name = "status_label"]
                    gtk::Label {
                        set_hexpand: true,
                        set_xalign: 0.0,
                        #[watch]
                        set_label: &model.status,
                    },

                    #[name = "progress_bar"]
                    gtk::ProgressBar {
                        set_width_request: 200,
                        set_show_text: true,
                        #[watch]
                        set_fraction: model.progress as f64 / 100.0,
                    },
                },

                // Latest Job Results group
                gtk::Frame {
                    set_label: Some("Latest Job Results"),

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 12,
                        set_margin_all: 8,

                        gtk::Label {
                            set_label: "Source 2 Delay:",
                        },

                        #[name = "source2_delay_label"]
                        gtk::Label {
                            #[watch]
                            set_label: &format_delay(model.source2_delay),
                        },

                        gtk::Label {
                            set_label: "Source 3 Delay:",
                            set_margin_start: 20,
                        },

                        #[name = "source3_delay_label"]
                        gtk::Label {
                            #[watch]
                            set_label: &format_delay(model.source3_delay),
                        },
                    },
                },

                // Log group
                gtk::Frame {
                    set_label: Some("Log"),
                    set_vexpand: true,

                    gtk::ScrolledWindow {
                        set_vexpand: true,
                        set_margin_all: 4,

                        #[name = "log_view"]
                        gtk::TextView {
                            set_editable: false,
                            set_monospace: true,
                            set_wrap_mode: gtk::WrapMode::WordChar,
                            set_cursor_visible: false,
                        },
                    },
                },
            },
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // Initialize config
        let base_dir = resolve_base_dir();
        let config_path = base_dir.join(".config").join("settings.toml");
        let mut config = ConfigManager::new(&config_path);
        if let Err(e) = config.load_or_create() {
            eprintln!("Warning: Failed to load config: {e}. Using defaults.");
        }
        if let Err(e) = config.ensure_dirs_exist() {
            eprintln!("Warning: Failed to create directories: {e}");
        }

        let archive_logs = config.settings().logging.archive_logs;

        // Create settings dialog child component
        let settings_dialog = SettingsDialog::builder()
            .launch(root.clone().upcast::<gtk::Window>())
            .forward(sender.input_sender(), |output| match output {
                SettingsOutput::Applied(settings) => AppMsg::SettingsApplied(settings),
            });

        // Create job queue dialog child component
        let job_queue_dialog = JobQueueDialog::builder()
            .launch(root.clone().upcast::<gtk::Window>())
            .forward(sender.input_sender(), |output| match output {
                JobQueueOutput::StartProcessing(jobs) => AppMsg::JobQueueStartProcessing(jobs),
                JobQueueOutput::Log(msg) => AppMsg::Log(msg),
            });

        let model = App {
            config,
            base_dir,
            sources: [String::new(), String::new(), String::new()],
            log_text: String::new(),
            status: "Ready".to_string(),
            progress: 0,
            archive_logs,
            source2_delay: None,
            source3_delay: None,
            settings_dialog,
            job_queue_dialog,
            cancel_handle: None,
            running: false,
        };

        let widgets = view_output!();

        // Set up drag-and-drop for source entries
        let entries = [
            &widgets.source1_entry,
            &widgets.source2_entry,
            &widgets.source3_entry,
        ];
        for (index, entry) in entries.into_iter().enumerate() {
            let drop_target =
                gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
            let s = sender.input_sender().clone();
            drop_target.connect_drop(move |_target, value, _x, _y| {
                if let Ok(file_list) = value.get::<gdk::FileList>() {
                    if let Some(file) = file_list.files().first() {
                        if let Some(path) = file.path() {
                            s.emit(AppMsg::SourceSelected(index, path));
                            return true;
                        }
                    }
                }
                false
            });
            entry.add_controller(drop_target);
        }

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            AppMsg::BrowseSource(index) => {
                let sender = sender.clone();
                let root = relm4::main_application().active_window();
                let dialog = gtk::FileDialog::builder()
                    .title(match index {
                        0 => "Select Reference File",
                        1 => "Select Secondary File",
                        _ => "Select Tertiary File",
                    })
                    .modal(true)
                    .build();

                dialog.open(root.as_ref(), gtk::gio::Cancellable::NONE, move |result| {
                    if let Ok(file) = result {
                        if let Some(path) = file.path() {
                            sender.input(AppMsg::SourceSelected(index, path));
                        }
                    }
                });
            }
            AppMsg::SourceSelected(index, path) => {
                self.sources[index] = path.display().to_string();
            }
            AppMsg::OpenSettings => {
                self.settings_dialog
                    .emit(SettingsMsg::Show(Box::new(self.config.settings().clone())));
            }
            AppMsg::SettingsApplied(settings) => {
                *self.config.settings_mut() = *settings;
                if let Err(e) = self.config.save() {
                    sender.input(AppMsg::Log(format!("[ERROR] Failed to save settings: {e}")));
                } else {
                    sender.input(AppMsg::Log("Settings saved.".into()));
                }
                self.archive_logs = self.config.settings().logging.archive_logs;
            }
            AppMsg::OpenJobQueue => {
                self.job_queue_dialog.emit(JobQueueMsg::Show);
            }
            AppMsg::JobQueueStartProcessing(jobs) => {
                if self.running {
                    sender.input(AppMsg::Log(
                        "[WARNING] Processing already running.".into(),
                    ));
                    return;
                }
                self.running = true;
                self.progress = 0;
                self.status = format!("Processing {} job(s)...", jobs.len());
                sender.input(AppMsg::Log(format!(
                    "Starting batch processing: {} job(s)",
                    jobs.len()
                )));

                let settings = self.config.settings().clone();
                let base_dir = self.base_dir.clone();
                let log_sender = sender.input_sender().clone();
                let progress_sender = sender.input_sender().clone();
                let done_sender = sender.input_sender().clone();

                std::thread::spawn(move || {
                    let total = jobs.len();

                    // Determine output dir
                    let output_base = base_dir.join(&settings.paths.output_folder);
                    let output_dir = if total > 1 {
                        // Batch: subfolder from source parent folder name
                        if let Some(src1) = jobs[0].sources.get("Source 1") {
                            if let Some(parent_name) =
                                src1.parent().and_then(|p| p.file_name())
                            {
                                output_base.join(parent_name)
                            } else {
                                output_base.clone()
                            }
                        } else {
                            output_base.clone()
                        }
                    } else {
                        output_base.clone()
                    };

                    for (idx, job) in jobs.iter().enumerate() {
                        let job_num = idx + 1;
                        let job_name = job
                            .sources
                            .get("Source 1")
                            .and_then(|p| p.file_stem())
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| format!("job_{job_num}"));

                        log_sender.emit(AppMsg::Log(format!(
                            "=== Job {job_num}/{total}: {job_name} ==="
                        )));

                        // Build JobSpec from JobQueueEntry
                        let mut job_spec = JobSpec::new(job.sources.clone());
                        if let Some(ref layout) = job.layout {
                            job_spec.manual_layout = Some(layout.to_job_spec_layout());
                            job_spec.attachment_sources = layout.attachment_sources.clone();
                        }

                        // Create per-job work dir
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        let work_dir = base_dir
                            .join(&settings.paths.temp_root)
                            .join(format!("job_{}_{}", job_name, timestamp));

                        // Create logger
                        let ls = log_sender.clone();
                        let gui_cb: GuiLogCallback = Box::new(move |msg: &str| {
                            ls.emit(AppMsg::Log(msg.to_string()));
                        });
                        let log_config = LogConfig {
                            level: LogLevel::Info,
                            compact: settings.logging.compact,
                            progress_step: settings.logging.progress_step,
                            error_tail: settings.logging.error_tail as usize,
                            show_timestamps: true,
                        };
                        let logger = match JobLogger::new(
                            &job_name,
                            &output_dir,
                            log_config,
                            Some(gui_cb),
                        ) {
                            Ok(l) => Arc::new(l),
                            Err(e) => {
                                done_sender.emit(AppMsg::BatchJobDone {
                                    job_index: idx,
                                    total_jobs: total,
                                    job_name: job_name.clone(),
                                    result: Err(format!("Failed to create logger: {e}")),
                                });
                                continue;
                            }
                        };

                        // Create pipeline (full merge if layout present, analyze-only if not)
                        let pipeline = if job.layout.is_some() {
                            create_standard_pipeline()
                        } else {
                            Pipeline::new().with_step(AnalyzeStep::new())
                        };

                        // Progress callback
                        let ps = progress_sender.clone();
                        let idx_for_progress = idx;
                        let total_for_progress = total;
                        let progress_cb: vsg_core::orchestrator::ProgressCallback =
                            Box::new(move |_step, percent, _msg| {
                                let batch_progress = ((idx_for_progress as u32 * 100 + percent)
                                    / total_for_progress as u32)
                                    .min(100);
                                ps.emit(AppMsg::Progress(batch_progress));
                            });

                        let ctx = Context::new(
                            job_spec,
                            settings.clone(),
                            job_name.clone(),
                            work_dir,
                            output_dir.clone(),
                            logger.clone(),
                        )
                        .with_progress_callback(progress_cb);
                        let mut state = JobState::new(&job.id);

                        match pipeline.run(&ctx, &mut state) {
                            Ok(_) => {
                                let s2 = state
                                    .delays()
                                    .and_then(|d| d.source_delays_ms.get("Source 2").copied())
                                    .unwrap_or(0);
                                let s3 = state
                                    .delays()
                                    .and_then(|d| d.source_delays_ms.get("Source 3").copied())
                                    .unwrap_or(0);
                                let output_path =
                                    state.mux.as_ref().map(|m| m.output_path.clone());
                                done_sender.emit(AppMsg::BatchJobDone {
                                    job_index: idx,
                                    total_jobs: total,
                                    job_name: job_name.clone(),
                                    result: Ok(BatchJobResult {
                                        source2_delay: s2,
                                        source3_delay: s3,
                                        output_path,
                                    }),
                                });
                            }
                            Err(e) => {
                                done_sender.emit(AppMsg::BatchJobDone {
                                    job_index: idx,
                                    total_jobs: total,
                                    job_name: job_name.clone(),
                                    result: Err(e.to_string()),
                                });
                            }
                        }

                        logger.flush();
                    }
                    done_sender.emit(AppMsg::BatchComplete);
                });
            }
            AppMsg::AnalyzeOnly => {
                // Validation
                if self.sources[0].is_empty() || self.sources[1].is_empty() {
                    sender.input(AppMsg::Log(
                        "[ERROR] Source 1 and Source 2 are required.".into(),
                    ));
                    return;
                }
                if self.running {
                    sender.input(AppMsg::Log("[WARNING] Analysis already running.".into()));
                    return;
                }

                self.running = true;
                self.progress = 0;
                self.status = "Analyzing...".to_string();

                // Build JobSpec
                let mut sources_map = HashMap::new();
                sources_map.insert("Source 1".to_string(), PathBuf::from(&self.sources[0]));
                sources_map.insert("Source 2".to_string(), PathBuf::from(&self.sources[1]));
                if !self.sources[2].is_empty() {
                    sources_map.insert("Source 3".to_string(), PathBuf::from(&self.sources[2]));
                }
                let job_spec = JobSpec::new(sources_map);

                // Clone settings and paths for the thread
                let settings = self.config.settings().clone();
                let work_dir = self
                    .base_dir
                    .join(&settings.paths.temp_root)
                    .join("analyze");
                let output_dir = self.base_dir.join(&settings.paths.output_folder);

                // Job name from Source 1 filename stem (matches Python behavior)
                let job_name = PathBuf::from(&self.sources[0])
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "analyze_only".to_string());

                // Sender clones for callbacks
                let log_sender = sender.input_sender().clone();
                let progress_sender = sender.input_sender().clone();
                let done_sender = sender.input_sender().clone();

                // Build logger callback
                let gui_callback: GuiLogCallback = Box::new(move |msg: &str| {
                    log_sender.emit(AppMsg::Log(msg.to_string()));
                });

                // Build log config
                let log_config = LogConfig {
                    level: LogLevel::Info,
                    compact: settings.logging.compact,
                    progress_step: settings.logging.progress_step,
                    error_tail: settings.logging.error_tail as usize,
                    show_timestamps: true,
                };

                // Build progress callback
                let progress_callback: vsg_core::orchestrator::ProgressCallback =
                    Box::new(move |_step: &str, percent: u32, _msg: &str| {
                        progress_sender.emit(AppMsg::Progress(percent));
                    });

                // Create pipeline and get cancel handle
                let pipeline = Pipeline::new().with_step(AnalyzeStep::new());
                let cancel_handle = pipeline.cancel_handle();
                self.cancel_handle = Some(cancel_handle);

                // Spawn background thread
                std::thread::spawn(move || {
                    // Create logger — log file goes to output_dir (matches Python behavior)
                    let logger = match JobLogger::new(
                        &job_name,
                        &output_dir,
                        log_config,
                        Some(gui_callback),
                    ) {
                        Ok(l) => Arc::new(l),
                        Err(e) => {
                            done_sender.emit(AppMsg::AnalysisDone(Err(format!(
                                "Failed to create logger: {e}"
                            ))));
                            return;
                        }
                    };

                    // Create context
                    let ctx = Context::new(
                        job_spec,
                        settings,
                        job_name,
                        work_dir,
                        output_dir,
                        logger.clone(),
                    )
                    .with_progress_callback(progress_callback);

                    // Create job state and run
                    let mut state = JobState::new("analyze_only");

                    match pipeline.run(&ctx, &mut state) {
                        Ok(_) => {
                            let (s2, s3) = if let Some(delays) = state.delays() {
                                (
                                    delays
                                        .source_delays_ms
                                        .get("Source 2")
                                        .copied()
                                        .unwrap_or(0),
                                    delays
                                        .source_delays_ms
                                        .get("Source 3")
                                        .copied()
                                        .unwrap_or(0),
                                )
                            } else {
                                (0, 0)
                            };
                            done_sender.emit(AppMsg::AnalysisDone(Ok(AnalysisResult {
                                source2_delay: s2,
                                source3_delay: s3,
                            })));
                        }
                        Err(e) => {
                            done_sender.emit(AppMsg::AnalysisDone(Err(e.to_string())));
                        }
                    }

                    logger.flush();
                });
            }
            AppMsg::CancelAnalysis => {
                if let Some(handle) = &self.cancel_handle {
                    handle.cancel();
                    sender.input(AppMsg::Log("Cancellation requested...".into()));
                    self.status = "Cancelling...".to_string();
                }
            }
            AppMsg::AnalysisDone(result) => {
                self.running = false;
                self.cancel_handle = None;

                match result {
                    Ok(analysis) => {
                        self.source2_delay = Some(analysis.source2_delay);
                        self.source3_delay = Some(analysis.source3_delay);
                        self.status = "Analysis complete".to_string();
                        self.progress = 100;
                        sender.input(AppMsg::Log(format!(
                            "Analysis complete: Source 2 delay = {}ms, Source 3 delay = {}ms",
                            analysis.source2_delay, analysis.source3_delay
                        )));
                    }
                    Err(e) => {
                        self.status = "Analysis failed".to_string();
                        self.progress = 0;
                        sender.input(AppMsg::Log(format!("[ERROR] Analysis failed: {e}")));
                    }
                }
            }
            AppMsg::BatchJobDone {
                job_index,
                total_jobs,
                job_name,
                result,
            } => match result {
                Ok(r) => {
                    self.status = format!(
                        "Job {}/{} complete: {}",
                        job_index + 1,
                        total_jobs,
                        job_name
                    );
                    let output_info = r
                        .output_path
                        .as_ref()
                        .map(|p| format!(", Output: {}", p.display()))
                        .unwrap_or_default();
                    sender.input(AppMsg::Log(format!(
                        "[SUCCESS] {} — S2: {}ms, S3: {}ms{}",
                        job_name, r.source2_delay, r.source3_delay, output_info
                    )));
                    self.source2_delay = Some(r.source2_delay);
                    self.source3_delay = Some(r.source3_delay);
                }
                Err(e) => {
                    self.status = format!(
                        "Job {}/{} failed: {}",
                        job_index + 1,
                        total_jobs,
                        job_name
                    );
                    sender.input(AppMsg::Log(format!("[ERROR] {} — {}", job_name, e)));
                }
            },
            AppMsg::BatchComplete => {
                self.running = false;
                self.progress = 100;
                self.status = "Batch complete.".to_string();
                self.cancel_handle = None;
                sender.input(AppMsg::Log("=== All jobs finished ===".into()));
            }
            AppMsg::Log(message) => {
                if !self.log_text.is_empty() {
                    self.log_text.push('\n');
                }
                self.log_text.push_str(&message);
            }
            AppMsg::Status(status) => {
                self.status = status;
            }
            AppMsg::Progress(pct) => {
                self.progress = pct;
            }
        }
    }

    /// Manual view updates for widgets that can't use #[watch]
    fn post_view() {
        // Update log text view (TextBuffer can't be driven by #[watch])
        let buffer = log_view.buffer();
        let current = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        if current.as_str() != self.log_text {
            buffer.set_text(&self.log_text);
            // Auto-scroll to bottom
            let mut end = buffer.end_iter();
            log_view.scroll_to_iter(&mut end, 0.0, false, 0.0, 1.0);
        }
    }
}
