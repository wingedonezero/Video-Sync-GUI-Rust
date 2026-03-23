//! Worker module — 1:1 port of `vsg_qt/worker/`.
//!
//! - Bridge: `bridges/worker_signals.rs` → `signals.py` (WorkerSignals QObject)
//! - `runner.rs` → `runner.py` (JobWorker background execution)

pub mod runner;
