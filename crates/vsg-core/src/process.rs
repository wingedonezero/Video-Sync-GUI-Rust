
use anyhow::{Result, Context};
use std::process::{Command, Stdio};

pub fn find_in_path(name: &str) -> Result<std::path::PathBuf> {
    Ok(which::which(name).context(format!("tool not found in PATH: {name}"))?)
}

pub fn run_quiet(cmd: &mut Command) -> Result<std::process::Output> {
    let program = format!("{:?}", cmd);
    let out = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).output()
        .with_context(|| format!("spawn failed: {program}"))?;
    Ok(out)
}
