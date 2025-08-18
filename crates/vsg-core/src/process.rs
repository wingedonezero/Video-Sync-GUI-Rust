use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader};
use anyhow::Result;
use crate::errors::VsgError;
use tracing::info;

/// Return absolute path to an executable on PATH (or specific path if given)
/// Errors with VsgError::ToolMissing when not found.
pub fn which_tool(name: &str) -> Result<String> {
    Ok(which::which(name)
        .map_err(|_| VsgError::ToolMissing(name.to_string()))?
        .into_os_string()
        .into_string()
        .map_err(|_| VsgError::ToolMissing(name.to_string()))?)
}

/// Stream stdout/stderr line-by-line to tracing and return exit code.
pub fn run_compact(cmd: &mut Command) -> Result<i32> {
    let mut child = cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let out_reader = BufReader::new(stdout);
    let err_reader = BufReader::new(stderr);

    for line in out_reader.lines() {
        if let Ok(l) = line { info!("{}", l); }
    }
    for line in err_reader.lines() {
        if let Ok(l) = line { info!("{}", l); }
    }

    let status = child.wait()?;
    let code = status.code();
    if !status.success() {
        return Err(VsgError::ProcessFailed { tool: format!("{:?}", cmd), code }.into());
    }
    Ok(code.unwrap_or(0))
}

/// Run a command and capture stdout fully (stderr streamed to logs).
/// Accepts &mut Command to mirror run_compact usage.
pub fn run_capture(cmd: &mut Command) -> Result<String> {
    let mut child = cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    // Stream stderr to logs to aid debugging (stdout captured below).
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(l) = line {
                info!("{}", l);
            }
        }
    });

    let out = {
        use std::io::Read;
        let mut buf = String::new();
        let mut r = BufReader::new(stdout);
        let _ = r.read_to_string(&mut buf);
        buf
    };

    let status = child.wait()?;
    if !status.success() {
        return Err(VsgError::ProcessFailed { tool: format!("{:?}", cmd), code: status.code() }.into());
    }
    Ok(out)
}
