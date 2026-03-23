//! Output writer — 1:1 port of `vsg_core/pipeline_components/output_writer.py`.

use std::path::{Path, PathBuf};

use crate::io::runner::CommandRunner;
use crate::models::settings::AppSettings;

/// Writes output files and mkvmerge configuration — `OutputWriter`
pub struct OutputWriter;

impl OutputWriter {
    /// Writes mkvmerge options to a JSON file — `write_mkvmerge_options`
    pub fn write_mkvmerge_options(
        tokens: &[String],
        temp_dir: &Path,
        settings: &AppSettings,
        runner: &CommandRunner,
    ) -> Result<String, String> {
        let opts_path = temp_dir.join("opts.json");

        let json_str = serde_json::to_string(tokens)
            .map_err(|e| format!("Failed to serialize mkvmerge options: {e}"))?;

        std::fs::write(&opts_path, &json_str)
            .map_err(|e| format!("Failed to write mkvmerge options file: {e}"))?;

        // Optional logging
        if settings.log_show_options_json {
            let pretty = serde_json::to_string_pretty(tokens).unwrap_or_default();
            runner.log_message(&format!(
                "--- mkvmerge options (json) ---\n{pretty}\n-------------------------------"
            ));
        }

        if settings.log_show_options_pretty {
            let pretty = tokens.join(" \\\n  ");
            runner.log_message(&format!(
                "--- mkvmerge options (pretty) ---\n{pretty}\n-------------------------------"
            ));
        }

        Ok(opts_path.to_string_lossy().to_string())
    }

    /// Prepares the final output path — `prepare_output_path`
    pub fn prepare_output_path(output_dir: &Path, source1_filename: &str) -> PathBuf {
        output_dir.join(source1_filename)
    }
}
