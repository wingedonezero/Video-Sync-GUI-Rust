// src/vsg_core/process.rs

use crate::config::Config;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

/// A function to execute external commands and stream their output.
///
/// # Arguments
/// * `config` - A reference to the application configuration.
/// * `program` - The executable to run (e.g., "mkvmerge").
/// * `args` - A slice of string arguments for the program.
/// * `log_callback` - A thread-safe closure that receives log messages.
///
/// # Returns
/// A `Result` containing the complete captured stdout string on success,
/// or an `anyhow::Error` on failure.
pub fn run_command<F>(
    config: &Config,
    program: &str,
    args: &[&str],
    log_callback: Arc<Mutex<F>>,
) -> anyhow::Result<String>
where
F: FnMut(String) + Send + 'static,
{
    let mut child = Command::new(program)
    .args(args)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|e| anyhow::anyhow!("Failed to spawn command '{}': {}", program, e))?;

    let stdout = child.stdout.take().expect("Failed to open stdout");
    let stderr = child.stderr.take().expect("Failed to open stderr");

    let mut full_output = String::new();
    let full_output_arc = Arc::new(Mutex::new(full_output));
    let reader_output_arc = Arc::clone(&full_output_arc);

    let log_clone = Arc::clone(&log_callback);
    let config_clone = Arc::new(config.clone()); // Assuming Config is Clone

    let stdout_thread = std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut last_prog = -1;

        for line in reader.lines() {
            if let Ok(line) = line {
                reader_output_arc.lock().unwrap().push_str(&line);
                reader_output_arc.lock().unwrap().push('\n');

                if config_clone.log_compact && line.starts_with("Progress: ") {
                    if let Some(pct_str) = line.split('%').next().and_then(|s| s.split_whitespace().last()) {
                        if let Ok(pct) = pct_str.parse::<i32>() {
                            if last_prog == -1 || pct >= last_prog + config_clone.log_progress_step as i32 || pct == 100 {
                                log_clone.lock().unwrap()(format!("Progress: {}%", pct));
                                last_prog = pct;
                            }
                        }
                    }
                } else {
                    log_clone.lock().unwrap()(line);
                }
            }
        }
    });

    let stderr_thread = std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(line) = line {
                log_callback.lock().unwrap()(line);
            }
        }
    });

    stdout_thread.join().expect("Stdout thread panicked");
    stderr_thread.join().expect("Stderr thread panicked");

    let status = child.wait()?;

    if status.success() {
        let final_output = Arc::try_unwrap(full_output_arc)
        .expect("Failed to unwrap Arc")
        .into_inner()
        .expect("Failed to get inner value of Mutex");
        Ok(final_output)
    } else {
        Err(anyhow::anyhow!(
            "Command '{}' failed with exit code {}",
            program,
            status.code().unwrap_or(-1)
        ))
    }
}

// We need to derive Clone for Config
impl Clone for Config {
    fn clone(&self) -> Self {
        Self {
            last_ref_path: self.last_ref_path.clone(),
            last_sec_path: self.last_sec_path.clone(),
            last_ter_path: self.last_ter_path.clone(),
            output_folder: self.output_folder.clone(),
            temp_root: self.temp_root.clone(),
            videodiff_path: self.videodiff_path.clone(),
            analysis_mode: self.analysis_mode.clone(),
            analysis_lang_ref: self.analysis_lang_ref.clone(),
            analysis_lang_sec: self.analysis_lang_sec.clone(),
            analysis_lang_ter: self.analysis_lang_ter.clone(),
            scan_chunk_count: self.scan_chunk_count,
            scan_chunk_duration: self.scan_chunk_duration,
            min_match_pct: self.min_match_pct,
            videodiff_error_min: self.videodiff_error_min,
            videodiff_error_max: self.videodiff_error_max,
            rename_chapters: self.rename_chapters,
            apply_dialog_norm_gain: self.apply_dialog_norm_gain,
            snap_chapters: self.snap_chapters,
            snap_mode: self.snap_mode.clone(),
            snap_threshold_ms: self.snap_threshold_ms,
            snap_starts_only: self.snap_starts_only,
            log_compact: self.log_compact,
            log_autoscroll: self.log_autoscroll,
            log_error_tail: self.log_error_tail,
            log_tail_lines: self.log_tail_lines,
            log_progress_step: self.log_progress_step,
            log_show_options_pretty: self.log_show_options_pretty,
            log_show_options_json: self.log_show_options_json,
            disable_track_statistics_tags: self.disable_track_statistics_tags,
            archive_logs: self.archive_logs,
            auto_apply_strict: self.auto_apply_strict,
        }
    }
}
