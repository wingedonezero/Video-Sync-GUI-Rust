//! Command runner — 1:1 port of `vsg_core/io/runner.py`.
//!
//! Wrapper for running external command-line processes (mkvmerge, ffmpeg, etc.).

use std::collections::{HashMap, VecDeque};
use std::process::{Command, Stdio};

use chrono::Local;

use crate::models::settings::AppSettings;

/// Log callback type — receives formatted log lines.
pub type LogCallback = Box<dyn Fn(&str) + Send + Sync>;

/// Executes external commands and streams output — `CommandRunner`
pub struct CommandRunner {
    settings: AppSettings,
    log: LogCallback,
}

impl CommandRunner {
    pub fn new(settings: AppSettings, log: LogCallback) -> Self {
        Self { settings, log }
    }

    /// Formats and sends a timestamped message to the log callback — `_log_message`
    pub fn log_message(&self, message: &str) {
        let ts = Local::now().format("%H:%M:%S");
        let line = format!("[{ts}] {message}");
        (self.log)(&line);
    }

    /// Executes a command and handles logging based on configuration — `run()`
    ///
    /// Returns captured stdout as a String.
    /// Returns None on failure (non-zero exit code or execution error).
    pub fn run(
        &self,
        cmd: &[&str],
        tool_paths: &HashMap<String, String>,
    ) -> Option<String> {
        self.run_with_options(cmd, tool_paths, false, None)
    }

    /// Run with binary output mode — returns raw bytes.
    pub fn run_binary(
        &self,
        cmd: &[&str],
        tool_paths: &HashMap<String, String>,
        input_data: Option<&[u8]>,
    ) -> Option<Vec<u8>> {
        if cmd.is_empty() {
            return None;
        }

        let tool_name = cmd[0];
        let resolved = tool_paths
            .get(tool_name)
            .filter(|p| !p.is_empty())
            .map(|p| p.as_str())
            .unwrap_or(tool_name);

        let mut full_cmd: Vec<String> = vec![resolved.to_string()];
        full_cmd.extend(cmd[1..].iter().map(|s| s.to_string()));

        let pretty_cmd = full_cmd
            .iter()
            .map(|c| shell_quote(c))
            .collect::<Vec<_>>()
            .join(" ");
        self.log_message(&format!("$ {pretty_cmd}"));

        let mut command = Command::new(&full_cmd[0]);
        command.args(&full_cmd[1..]);
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped()); // Separate stderr in binary mode

        if input_data.is_some() {
            command.stdin(Stdio::piped());
        }

        let child = match command.spawn() {
            Ok(c) => c,
            Err(e) => {
                self.log_message(&format!("[!] Failed to execute command: {e}"));
                return None;
            }
        };

        let output = if let Some(input) = input_data {
            use std::io::Write;
            let mut child = child;
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(input);
            }
            match child.wait_with_output() {
                Ok(o) => o,
                Err(e) => {
                    self.log_message(&format!("[!] Failed to execute command: {e}"));
                    return None;
                }
            }
        } else {
            match child.wait_with_output() {
                Ok(o) => o,
                Err(e) => {
                    self.log_message(&format!("[!] Failed to execute command: {e}"));
                    return None;
                }
            }
        };

        // Log stderr separately in binary mode
        if !output.stderr.is_empty() {
            let stderr_text = String::from_utf8_lossy(&output.stderr);
            let trimmed = stderr_text.trim();
            if !trimmed.is_empty() {
                self.log_message(&format!("[ffmpeg stderr] {trimmed}"));
            }
        }

        if !output.status.success() {
            let rc = output.status.code().unwrap_or(-1);
            self.log_message(&format!("[!] Command failed with exit code {rc}"));
            return None;
        }

        Some(output.stdout)
    }

    /// Full run implementation with all options — `run()` from Python.
    pub fn run_with_options(
        &self,
        cmd: &[&str],
        tool_paths: &HashMap<String, String>,
        is_binary: bool,
        input_data: Option<&[u8]>,
    ) -> Option<String> {
        if is_binary {
            // Binary mode delegates to run_binary and converts
            return self
                .run_binary(cmd, tool_paths, input_data)
                .map(|bytes| String::from_utf8_lossy(&bytes).to_string());
        }

        if cmd.is_empty() {
            return None;
        }

        let tool_name = cmd[0];
        // Use `or` logic: dict.get returns None if key exists with empty value
        let resolved = tool_paths
            .get(tool_name)
            .filter(|p| !p.is_empty())
            .map(|p| p.as_str())
            .unwrap_or(tool_name);

        let mut full_cmd: Vec<String> = vec![resolved.to_string()];
        full_cmd.extend(cmd[1..].iter().map(|s| s.to_string()));

        let pretty_cmd = full_cmd
            .iter()
            .map(|c| shell_quote(c))
            .collect::<Vec<_>>()
            .join(" ");
        self.log_message(&format!("$ {pretty_cmd}"));

        let compact = self.settings.log_compact;
        let tail_ok = self.settings.log_tail_lines;
        let err_tail = self.settings.log_error_tail;
        let prog_step = self.settings.log_progress_step.max(1);

        let mut command = Command::new(&full_cmd[0]);
        command.args(&full_cmd[1..]);
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        if input_data.is_some() {
            command.stdin(Stdio::piped());
        }

        let child = match command.spawn() {
            Ok(c) => c,
            Err(e) => {
                self.log_message(&format!("[!] Failed to execute command: {e}"));
                return None;
            }
        };

        let output = if let Some(input) = input_data {
            use std::io::Write;
            let mut child = child;
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(input);
            }
            match child.wait_with_output() {
                Ok(o) => o,
                Err(e) => {
                    self.log_message(&format!("[!] Failed to execute command: {e}"));
                    return None;
                }
            }
        } else {
            match child.wait_with_output() {
                Ok(o) => o,
                Err(e) => {
                    self.log_message(&format!("[!] Failed to execute command: {e}"));
                    return None;
                }
            }
        };

        let rc = output.status.code().unwrap_or(0);

        // Combine stdout + stderr for text mode (matches Python's subprocess.STDOUT)
        let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        if !stderr_str.is_empty() {
            if !combined.is_empty() && !combined.ends_with('\n') {
                combined.push('\n');
            }
            combined.push_str(&stderr_str);
        }

        let out_lines: Vec<&str> = combined.lines().collect();

        if compact {
            let capacity = tail_ok.max(err_tail).max(1) as usize;
            let mut tail_buffer: VecDeque<&str> = VecDeque::with_capacity(capacity);
            let mut last_prog: i32 = -1;

            for line in &out_lines {
                if line.starts_with("Progress: ") {
                    if let Some(pct) = parse_progress_pct(line) {
                        if last_prog < 0
                            || pct >= last_prog + prog_step
                            || pct == 100
                        {
                            self.log_message(&format!("Progress: {pct}%"));
                            last_prog = pct;
                        }
                    }
                } else {
                    if tail_buffer.len() >= capacity {
                        tail_buffer.pop_front();
                    }
                    tail_buffer.push_back(line);
                }
            }

            if rc != 0 {
                self.log_message(&format!("[!] Command failed with exit code {rc}"));
                if err_tail > 0 && !tail_buffer.is_empty() {
                    let skip = tail_buffer
                        .len()
                        .saturating_sub(err_tail as usize);
                    let error_lines: Vec<&str> =
                        tail_buffer.iter().skip(skip).copied().collect();
                    if !error_lines.is_empty() {
                        self.log_message(&format!(
                            "[stderr/tail]\n{}",
                            error_lines.join("\n")
                        ));
                    }
                }
                return None;
            }

            if tail_ok > 0 && !tail_buffer.is_empty() {
                let skip = tail_buffer
                    .len()
                    .saturating_sub(tail_ok as usize);
                let success_lines: Vec<&str> =
                    tail_buffer.iter().skip(skip).copied().collect();
                if !success_lines.is_empty() {
                    self.log_message(&format!(
                        "[stdout/tail]\n{}",
                        success_lines.join("\n")
                    ));
                }
            }
        } else {
            for line in &out_lines {
                self.log_message(line);
            }

            if rc != 0 {
                self.log_message(&format!("[!] Command failed with exit code {rc}"));
                return None;
            }
        }

        Some(combined)
    }
}

/// Parse a progress percentage from a line like "Progress: 42%"
fn parse_progress_pct(line: &str) -> Option<i32> {
    let trimmed = line.trim();
    let last_word = trimmed.split_whitespace().last()?;
    let num_str = last_word.trim_end_matches('%');
    num_str.parse::<i32>().ok()
}

/// Simple shell quoting for logging — similar to shlex.quote()
fn shell_quote(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    // If it contains spaces, quotes, or special chars, wrap in single quotes
    if s.contains(|c: char| c.is_whitespace() || "\"'\\$`!#&|;(){}[]<>?*~".contains(c))
    {
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_progress_works() {
        assert_eq!(parse_progress_pct("Progress: 42%"), Some(42));
        assert_eq!(parse_progress_pct("Progress: 100%"), Some(100));
        assert_eq!(parse_progress_pct("not progress"), None);
    }

    #[test]
    fn shell_quote_basic() {
        assert_eq!(shell_quote("simple"), "simple");
        assert_eq!(shell_quote("has space"), "'has space'");
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn shell_quote_special_chars() {
        assert_eq!(shell_quote("file (1).mkv"), "'file (1).mkv'");
        assert_eq!(shell_quote("it's"), "\"it'\\''s\"".replace('"', "'"));
    }
}
