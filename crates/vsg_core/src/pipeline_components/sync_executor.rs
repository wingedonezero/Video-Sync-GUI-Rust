//! Sync executor — 1:1 port of `vsg_core/pipeline_components/sync_executor.py`.
//!
//! Handles merge execution and post-processing finalization.

use std::collections::HashMap;
use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::models::settings::AppSettings;
use crate::postprocess::finalizer::{check_if_rebasing_is_needed, finalize_merged_file};

/// Executes sync merges and finalizes output — `SyncExecutor`
pub struct SyncExecutor;

impl SyncExecutor {
    /// Executes mkvmerge with the provided options file — `execute_merge`
    pub fn execute_merge(
        mkvmerge_options_path: &str,
        tool_paths: &HashMap<String, String>,
        runner: &CommandRunner,
    ) -> bool {
        let at_path = format!("@{mkvmerge_options_path}");
        runner.run(&["mkvmerge", &at_path], tool_paths).is_some()
    }

    /// Finalizes the merged output file — `finalize_output`
    pub fn finalize_output(
        temp_output_path: &Path,
        final_output_path: &Path,
        settings: &AppSettings,
        tool_paths: &HashMap<String, String>,
        runner: &CommandRunner,
    ) -> Result<(), String> {
        let normalize_enabled = settings.post_mux_normalize_timestamps;

        if normalize_enabled && check_if_rebasing_is_needed(temp_output_path, runner, tool_paths) {
            finalize_merged_file(temp_output_path, final_output_path, runner, settings, tool_paths);
            Ok(())
        } else {
            // Simple move
            std::fs::rename(temp_output_path, final_output_path)
                .or_else(|_| {
                    std::fs::copy(temp_output_path, final_output_path)
                        .map_err(|e| format!("Failed to copy output: {e}"))?;
                    let _ = std::fs::remove_file(temp_output_path);
                    Ok(())
                })
        }
    }
}
