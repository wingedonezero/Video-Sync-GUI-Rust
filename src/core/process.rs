// src/core/process.rs

use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use crate::core::config::AppConfig;

#[derive(Debug)]
pub struct CommandResult {
    pub stdout: String,
    pub exit_code: i32,
}

pub struct CommandRunner {
    config: AppConfig,
    log_sender: mpsc::Sender<String>,
}

impl CommandRunner {
    pub fn new(config: AppConfig, log_sender: mpsc::Sender<String>) -> Self {
        CommandRunner { config, log_sender }
    }

    pub async fn send_log(&self, msg: &str) {
        self.log_sender.send(msg.to_string()).await.ok();
    }

    pub async fn run(&self, program: &str, args: &[&str]) -> Result<CommandResult, String> {
        let command_str = format!("{} {}", program, args.join(" "));
        self.log_sender.send(format!("$ {}", command_str)).await.ok();

        let mut cmd = Command::new(program);
        cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| e.to_string())?;

        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut full_stdout = String::new();
        let mut last_progress = -1isize;
        let progress_step = self.config.log_progress_step as isize;

        loop {
            tokio::select! {
                result = stdout_reader.next_line() => {
                    match result {
                        Ok(Some(line)) => {
                            // Faithful port of the compact logging logic for progress bars
                            if self.config.log_compact && line.starts_with("Progress: ") {
                                if let Ok(pct) = line.trim_start_matches("Progress: ").trim_end_matches('%').parse::<isize>() {
                                    if last_progress < 0 || pct >= last_progress + progress_step || pct == 100 {
                                        self.log_sender.send(line.clone()).await.ok();
                                        last_progress = pct;
                                    }
                                }
                            } else {
                                self.log_sender.send(line.clone()).await.ok();
                            }
                            full_stdout.push_str(&line);
                            full_stdout.push('\n');
                        }
                        Ok(None) => break, // stdout closed
                        Err(_) => break, // Error reading line
                    }
                },
                result = stderr_reader.next_line() => {
                    if let Ok(Some(line)) = result {
                        self.log_sender.send(format!("[STDERR] {}", line)).await.ok();
                    }
                }
            }
        }

        let status = child.wait().await.map_err(|e| e.to_string())?;
        Ok(CommandResult {
            stdout: full_stdout,
            exit_code: status.code().unwrap_or(1),
        })
    }
}
