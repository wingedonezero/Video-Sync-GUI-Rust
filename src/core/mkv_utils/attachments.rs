// src/core/mkv_utils/attachments.rs

use super::streams::list_attachments;
use crate::core::command_runner::CommandRunner;
use std::path::{Path, PathBuf};

/// Extract all attachments from `mkv` into `temp_dir`.
/// Returns a vector of extracted file paths (strings).
pub fn extract_attachments(mkv: &str, temp_dir: &Path, runner: &CommandRunner, role: &str) -> Vec<String> {
    let attachments = list_attachments(mkv, runner);
    if attachments.is_empty() {
        return vec![];
    }

    let mut specs: Vec<String> = vec![];
    let mut outs: Vec<String> = vec![];

    for a in attachments {
        let out_path = temp_dir.join(format!("{}_att_{}_{}", role, a.id, a.file_name));
        specs.push(format!("{}:{}", a.id, out_path.to_string_lossy()));
        outs.push(out_path.to_string_lossy().to_string());
    }

    let mut cmd: Vec<String> = vec!["mkvextract".into(), mkv.into(), "attachments".into()];
    cmd.extend(specs);
    let refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
    let _ = runner.run(&refs);

    outs
}
