//! Worker signals — 1:1 port of `vsg_qt/worker/signals.py`.
//!
//! Defines the signals available from a running worker thread.
//! In Python this was a QObject subclass with Signal attributes.
//! In CXX-Qt these are signals on a QObject bridge.

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// WorkerSignals QObject — emits cross-thread signals for UI updates.
        ///
        /// Matches Python's WorkerSignals(QObject) with 5 signals:
        /// - log(str) → log_message(QString)
        /// - progress(float) → progress_updated(f64)
        /// - status(str) → status_updated(QString)
        /// - finished_job(dict) → job_finished(QString) [JSON-serialized]
        /// - finished_all(list) → all_jobs_finished(QString) [JSON-serialized]
        #[qobject]
        #[qml_element]
        type WorkerSignals = super::WorkerSignalsRust;

        /// Emitted with log lines.
        #[qsignal]
        fn log_message(self: Pin<&mut WorkerSignals>, message: QString);

        /// Emitted with progress 0.0 to 1.0.
        #[qsignal]
        fn progress_updated(self: Pin<&mut WorkerSignals>, value: f64);

        /// Emitted with short status string.
        #[qsignal]
        fn status_updated(self: Pin<&mut WorkerSignals>, message: QString);

        /// Emitted with result for a single job (JSON-serialized PipelineResult).
        #[qsignal]
        fn job_finished(self: Pin<&mut WorkerSignals>, result_json: QString);

        /// Emitted with list of all results at batch end (JSON-serialized).
        #[qsignal]
        fn all_jobs_finished(self: Pin<&mut WorkerSignals>, results_json: QString);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

/// Backing Rust struct for WorkerSignals QObject.
#[derive(Default)]
pub struct WorkerSignalsRust {}
