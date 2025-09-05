// src/core/process.rs
use std::collections::VecDeque;
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

        let mut tail_buffer: VecDeque<String> = VecDeque::with_capacity(self.config.log_error_tail as usize + 1);

        loop {
            tokio::select! {
                result = stdout_reader.next_line() => {
                    match result {
                        Ok(Some(line)) => {
                            if self.config.log_compact {
                                if line.starts_with("Progress: ") {
                                    if let Ok(pct) = line.trim_start_matches("Progress: ").trim_end_matches('%').parse::<isize>() {
                                        if last_progress < 0 || pct >= last_progress + progress_step || pct == 100 {
                                            self.log_sender.send(line.clone()).await.ok();
                                            last_progress = pct;
                                        }
                                    }
                                } else {
                                    self.log_sender.send(line.clone()).await.ok();
                                }
                                if self.config.log_error_tail > 0 {
                                    if tail_buffer.len() == self.config.log_error_tail as usize {
                                        tail_buffer.pop_front();
                                    }
                                    tail_buffer.push_back(line.clone());
                                }
                            } else {
                                self.log_sender.send(line.clone()).await.ok();
                            }
                            full_stdout.push_str(&line);
                            full_stdout.push('\n');
                        }
                        Ok(None) => break,
                        Err(_) => break,
                    }
                },
                result = stderr_reader.next_line() => {
                    if let Ok(Some(line)) = result {
                        let log_line = format!("[STDERR] {}", line);
                        self.log_sender.send(log_line.clone()).await.ok();
                        if self.config.log_error_tail > 0 {
                            if tail_buffer.len() == self.config.log_error_tail as usize {
                                tail_buffer.pop_front();
                            }
                            tail_buffer.push_back(log_line);
                        }
                    }
                }
            }
        }

        let status = child.wait().await.map_err(|e| e.to_string())?;

        if !status.success() && self.config.log_compact && self.config.log_error_tail > 0 {
            self.log_sender.send("\n--- Last output on error ---".to_string()).await.ok();
            for line in tail_buffer {
                self.log_sender.send(line).await.ok();
            }
            self.log_sender.send("--------------------------".to_string()).await.ok();
        }

        Ok(CommandResult {
            stdout: full_stdout,
            exit_code: status.code().unwrap_or(1),
        })
    }
}
