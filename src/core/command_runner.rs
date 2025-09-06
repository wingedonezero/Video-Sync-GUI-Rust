// src/core/command_runner.rs
//
// Rust port of CommandRunner:
// - timestamped logging
// - compact vs verbose modes
// - progress throttling via "Progress: <N>%"
// - success/err tail sections
// - returns full stdout (merged with stderr) on success; None on failure

use chrono::Local;
use crossbeam_channel::{unbounded, Receiver, Select};
use serde_json::{Map as JsonMap, Value};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::thread;

pub struct CommandRunner {
    config: JsonMap<String, Value>,
    log_cb: Box<dyn Fn(&str) + Send + Sync>,
}

impl Clone for CommandRunner {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            log_cb: self.log_cb.clone(),
        }
    }
}

impl CommandRunner {
    pub fn new<F>(config: JsonMap<String, Value>, log_callback: F) -> Self
    where
    F: Fn(&str) + Send + Sync + 'static,
    {
        Self {
            config,
            log_cb: Box::new(log_callback),
        }
    }

    #[inline]
    pub fn log(&self, msg: &str) {
        let ts = Local::now().format("%H:%M:%S");
        (self.log_cb)(&format!("[{}] {}", ts, msg));
    }

    /// Execute a command. Returns full merged output on success; None on failure.
    /// `args` must be ["program", "arg1", ...]
    pub fn run(&self, args: &[&str]) -> Option<String> {
        if args.is_empty() {
            return None;
        }

        let compact = self.config.get("log_compact").and_then(|v| v.as_bool()).unwrap_or(true);
        let tail_ok: usize = self.config.get("log_tail_lines").and_then(|v| v.as_i64()).unwrap_or(0).max(0) as usize;
        let err_tail: usize = self.config.get("log_error_tail").and_then(|v| v.as_i64()).unwrap_or(20).max(0) as usize;
        let prog_step: i64 = self.config.get("log_progress_step").and_then(|v| v.as_i64()).unwrap_or(100).max(1);

        // Pretty-printed command for logs
        let pretty = args
        .iter()
        .map(|s| {
            if s.contains(' ') || s.contains('"') || s.contains('\'') {
                format!("{:?}", s)
            } else {
                s.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
        self.log(&format!("$ {}", pretty));

        // Spawn process
        let mut cmd = Command::new(args[0]);
        if args.len() > 1 {
            cmd.args(&args[1..]);
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                self.log(&format!("[!] Failed to execute command: {}", e));
                return None;
            }
        };

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        // Merge stdout & stderr by sending lines into a single channel
        let (tx, rx) = unbounded::<String>();

        // stdout thread
        let tx_out = tx.clone();
        let _t1 = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => { let _ = tx_out.send(l); }
                    Err(_) => break,
                }
            }
        });

        // stderr thread
        let tx_err = tx.clone();
        let _t2 = thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                match line {
                    Ok(l) => { let _ = tx_err.send(l); }
                    Err(_) => break,
                }
            }
        });

        // Consume lines
        let mut out_buf = String::new();
        let mut last_prog: i64 = -1;
        let mut tail_buffer: VecDeque<String> = VecDeque::with_capacity(err_tail.max(tail_ok).max(1));

        while let Ok(line) = rx.recv() {
            out_buf.push_str(&line);
            out_buf.push('\n');

            if compact {
                if line.starts_with("Progress: ") {
                    // parse percent
                    if let Some(pct_str) = line.split_whitespace().last() {
                        let pct_clean = pct_str.trim_end_matches('%');
                        if let Ok(pct) = pct_clean.parse::<i64>() {
                            if last_prog < 0 || pct >= last_prog + prog_step || pct == 100 {
                                self.log(&format!("Progress: {}%", pct));
                                last_prog = pct;
                            }
                        }
                    }
                } else {
                    if tail_buffer.len() == tail_buffer.capacity() && tail_buffer.capacity() > 0 {
                        tail_buffer.pop_front();
                    }
                    if tail_buffer.capacity() > 0 {
                        tail_buffer.push_back(line);
                    }
                }
            } else {
                self.log(&line);
            }
        }

        // Wait and get status
        let status = match child.wait() {
            Ok(s) => s,
            Err(e) => {
                self.log(&format!("[!] Command wait failed: {}", e));
                return None;
            }
        };
        let rc = status.code().unwrap_or(0);

        if rc != 0 {
            self.log(&format!("[!] Command failed with exit code {}", rc));
            if compact && err_tail > 0 && !tail_buffer.is_empty() {
                // last err_tail lines
                let take = tail_buffer.len().min(err_tail);
                let start = tail_buffer.len() - take;
                let mut tail = String::new();
                for l in tail_buffer.iter().skip(start) {
                    tail.push_str(l);
                    tail.push('\n');
                }
                if !tail.is_empty() {
                    self.log(&format!("[stderr/tail]\n{}", tail.trim_end()));
                }
            }
            return None;
        }

        if compact && tail_ok > 0 && !tail_buffer.is_empty() {
            let take = tail_buffer.len().min(tail_ok);
            let start = tail_buffer.len() - take;
            let mut tail = String::new();
            for l in tail_buffer.iter().skip(start) {
                tail.push_str(l);
                tail.push('\n');
            }
            if !tail.is_empty() {
                self.log(&format!("[stdout/tail]\n{}", tail.trim_end()));
            }
        }

        Some(out_buf)
    }
}
