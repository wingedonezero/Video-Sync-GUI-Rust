//! Sync planner — 1:1 port of `vsg_core/pipeline_components/sync_planner.py`.
//!
//! Wraps the Orchestrator to provide a cleaner interface for sync planning.

use std::collections::HashMap;

use crate::models::context_types::ManualLayoutItem;
use crate::models::settings::AppSettings;
use crate::orchestrator::pipeline::Orchestrator;
use crate::orchestrator::steps::context::Context;

/// Plans sync operations by delegating to the Orchestrator — `SyncPlanner`
pub struct SyncPlanner;

impl SyncPlanner {
    /// Plans the sync operation — `plan_sync`
    #[allow(clippy::too_many_arguments)]
    pub fn plan_sync(
        settings: &AppSettings,
        tool_paths: &HashMap<String, String>,
        log_callback: Box<dyn Fn(&str) + Send + Sync>,
        progress_callback: Box<dyn Fn(f64) + Send + Sync>,
        sources: &HashMap<String, String>,
        and_merge: bool,
        output_dir: &str,
        manual_layout: Vec<ManualLayoutItem>,
        attachment_sources: Vec<String>,
        source_settings: HashMap<String, serde_json::Value>,
    ) -> Result<Context, String> {
        let orch = Orchestrator;
        orch.run(
            settings,
            tool_paths,
            log_callback,
            progress_callback,
            sources,
            and_merge,
            output_dir,
            manual_layout,
            attachment_sources,
            source_settings,
        )
    }
}
