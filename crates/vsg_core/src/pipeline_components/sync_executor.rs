//! Sync executor — 1:1 port of `vsg_core/pipeline_components/sync_executor.py`.
//!
//! Handles merge execution and post-processing finalization.

use std::collections::HashMap;
use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::models::settings::AppSettings;

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
    ///
    /// Handles timestamp normalization if enabled and needed, otherwise
    /// simply moves the file to its final location.
    pub fn finalize_output(
        temp_output_path: &Path,
        final_output_path: &Path,
        _settings: &AppSettings,
        _tool_paths: &HashMap<String, String>,
        _runner: &CommandRunner,
    ) -> Result<(), String> {
        // TODO: When postprocess module is ported, add timestamp normalization check:
        // if settings.post_mux_normalize_timestamps && check_if_rebasing_is_needed(...) {
        //     finalize_merged_file(...)
        // } else {
        //     move file
        // }
        std::fs::rename(temp_output_path, final_output_path)
            .or_else(|_| {
                // rename fails across filesystems, fall back to copy+delete
                std::fs::copy(temp_output_path, final_output_path)
                    .map_err(|e| format!("Failed to copy output: {e}"))?;
                let _ = std::fs::remove_file(temp_output_path);
                Ok(())
            })
    }
}
