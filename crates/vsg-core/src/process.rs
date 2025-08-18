
use anyhow::{Result, Context};
use std::process::{Command, Stdio};

pub fn find_in_path(name: &str) -> Result<std::path::PathBuf> {
    Ok(which::which(name).context(format!("tool not found in PATH: {name}"))?)
}

pub fn run_quiet(mut cmd: Command) -> Result<std::process::Output> {
    let display = format!("{:?}", cmd);
    let out = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).output()
        .with_context(|| format!("spawn failed: {display}"))?;
    Ok(out)
}

pub fn must_succeed(out: std::process::Output, context: &str) -> Result<std::process::Output> {
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("{context}: exit={:?}, stderr={}", out.status.code(), stderr);
    }
    Ok(out)
}
