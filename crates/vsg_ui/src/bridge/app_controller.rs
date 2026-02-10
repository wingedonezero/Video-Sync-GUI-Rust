//! Main application controller - CXX-Qt bridge between Qt UI and Rust logic

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::runtime::Runtime;
use vsg_core::config::ConfigManager;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qproperty(i32, ready)]
        type AppController = super::AppControllerRust;

        /// Browse for a file or directory
        #[qinvokable]
        fn browse_file(self: Pin<&mut AppController>, title: QString) -> QString;

        /// Start analyzing sources (analyze-only mode)
        #[qinvokable]
        fn analyze_sources(
            self: Pin<&mut AppController>,
            source1: QString,
            source2: QString,
            source3: QString,
        );

        /// Open settings dialog (stub for now)
        #[qinvokable]
        fn open_settings(self: Pin<&mut AppController>);

        /// Open job queue dialog (stub for now)
        #[qinvokable]
        fn open_job_queue(self: Pin<&mut AppController>);

        /// Emitted when a log message should be displayed
        #[qsignal]
        fn log_message(self: Pin<&mut AppController>, message: QString);

        /// Emitted when progress changes (0-100)
        #[qsignal]
        fn progress_update(self: Pin<&mut AppController>, percent: i32);

        /// Emitted when status changes
        #[qsignal]
        fn status_update(self: Pin<&mut AppController>, status: QString);

        /// Emitted when analysis completes
        #[qsignal]
        fn analysis_complete(
            self: Pin<&mut AppController>,
            source2_delay: i64,
            source3_delay: i64,
        );
    }
}

/// Rust implementation of the AppController
pub struct AppControllerRust {
    /// Ready property (placeholder)
    ready: i32,
    /// Configuration manager
    config: Arc<Mutex<ConfigManager>>,
    /// Tokio runtime for async tasks
    runtime: Arc<Runtime>,
}

impl Default for AppControllerRust {
    fn default() -> Self {
        // Initialize config manager
        let config_path = PathBuf::from(".config/settings.toml");
        let mut config_manager = ConfigManager::new(&config_path);
        if let Err(e) = config_manager.load_or_create() {
            eprintln!("Warning: Failed to load config: {}. Using defaults.", e);
        }

        // Create tokio runtime for async tasks
        let runtime = Runtime::new().expect("Failed to create tokio runtime");

        Self {
            ready: 1,
            config: Arc::new(Mutex::new(config_manager)),
            runtime: Arc::new(runtime),
        }
    }
}

impl qobject::AppController {
    /// Browse for a file or directory using native dialog
    pub fn browse_file(self: Pin<&mut Self>, title: qobject::QString) -> qobject::QString {
        // Use rfd for native file dialog
        let title_str = title.to_string();
        if let Some(path) = rfd::FileDialog::new().set_title(&title_str).pick_file() {
            return path.display().to_string().into();
        }

        qobject::QString::default()
    }

    /// Start analyzing sources (stub for now)
    pub fn analyze_sources(
        mut self: Pin<&mut Self>,
        source1: qobject::QString,
        source2: qobject::QString,
        source3: qobject::QString,
    ) {
        // Log what we received
        let log_msg = format!(
            "Analyze sources called:\n  Source 1: {}\n  Source 2: {}\n  Source 3: {}",
            source1.to_string(),
            source2.to_string(),
            source3.to_string()
        );
        self.as_mut().log_message(log_msg.into());

        let status_msg = "Analysis not yet implemented";
        self.as_mut().status_update(status_msg.into());

        // TODO: Implement actual analysis
        // This will be wired up once we have settings working
        // For now, just show that the button works
    }

    /// Open settings dialog (stub for now)
    pub fn open_settings(mut self: Pin<&mut Self>) {
        let log_msg = "Settings dialog coming next!";
        self.as_mut().log_message(log_msg.into());

        let status_msg = "Settings - Not yet implemented";
        self.as_mut().status_update(status_msg.into());
    }

    /// Open job queue dialog (stub for now)
    pub fn open_job_queue(mut self: Pin<&mut Self>) {
        let log_msg = "Job queue dialog not yet implemented";
        self.as_mut().log_message(log_msg.into());

        let status_msg = "Job Queue - Not yet implemented";
        self.as_mut().status_update(status_msg.into());
    }
}
