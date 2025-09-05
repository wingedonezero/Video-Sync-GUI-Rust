// src/core/process.rs

use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// A simple struct to hold the result of a command execution.
#[derive(Debug)]
pub struct CommandResult {
    pub stdout: String,
    pub exit_code: i32,
}

/// Asynchronously runs external commands and streams their output.
#[derive(Debug)]
pub struct CommandRunner {
    // A channel to send log messages back to the UI thread.
    log_sender: mpsc::Sender<String>,
}

impl CommandRunner {
    /// Creates a new CommandRunner.
    pub fn new(log_sender: mpsc::Sender<String>) -> Self {
        CommandRunner { log_sender }
    }

    /// Runs a command, streams its output to the logger, and returns the full stdout.
    pub async fn run(&self, program: &str, args: &[&str]) -> Result<CommandResult, String> {
        let command_str = format!("{} {}", program, args.join(" "));
        self.log_sender.send(format!("$ {}", command_str)).await.ok();

        let mut cmd = Command::new(program);
        cmd.args(args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped()); // Capture stderr as well

        let mut child = cmd.spawn().map_err(|e| e.to_string())?;

        // We combine stdout and stderr into one stream for logging.
        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut full_stdout = String::new();

        loop {
            tokio::select! {
                // Read a line from stdout
                result = stdout_reader.next_line() => {
                    match result {
                        Ok(Some(line)) => {
                            self.log_sender.send(line.clone()).await.ok();
                            full_stdout.push_str(&line);
                            full_stdout.push('\n');
                        }
                        Ok(None) => break, // stdout is closed
                        Err(e) => {
                            self.log_sender.send(format!("Error reading stdout: {}", e)).await.ok();
                            break;
                        }
                    }
                },
                // Read a line from stderr
                result = stderr_reader.next_line() => {
                    match result {
                        Ok(Some(line)) => {
                            // We can prefix stderr lines to distinguish them in the log
                            self.log_sender.send(format!("[STDERR] {}", line)).await.ok();
                        }
                        Ok(None) => {}, // stderr can close before stdout
                        Err(e) => {
                            self.log_sender.send(format!("Error reading stderr: {}", e)).await.ok();
                        }
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
