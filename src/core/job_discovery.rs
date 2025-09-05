// src/core/job_discovery.rs

use std::path::{Path, PathBuf};
use crate::core::pipeline::Job;

const VALID_EXTENSIONS: &[&str] = &["mkv", "mp4", "m4v"];

/// Discovers jobs based on input paths, supporting single file or batch (directory) mode.
pub fn discover_jobs(
    ref_path_str: &str,
    sec_path_str: &str,
    ter_path_str: &str,
) -> Result<Vec<Job>, String> {
    if ref_path_str.is_empty() {
        return Err("Reference path cannot be empty.".to_string());
    }
    let ref_path = Path::new(ref_path_str);
    if !ref_path.exists() {
        return Err(format!("Reference path does not exist: {}", ref_path_str));
    }

    let sec_path = if sec_path_str.is_empty() { None } else { Some(Path::new(sec_path_str)) };
    let ter_path = if ter_path_str.is_empty() { None } else { Some(Path::new(ter_path_str)) };

    // --- Single File Mode ---
    if ref_path.is_file() {
        let sec_file = sec_path.filter(|p| p.is_file()).map(|p| p.to_string_lossy().into_owned());
        let ter_file = ter_path.filter(|p| p.is_file()).map(|p| p.to_string_lossy().into_owned());

        return Ok(vec![Job {
            ref_file: ref_path_str.to_string(),
                  sec_file,
                  ter_file,
        }]);
    }

    // --- Batch (Directory) Mode ---
    if ref_path.is_dir() {
        if sec_path.map_or(false, |p| p.is_file()) || ter_path.map_or(false, |p| p.is_file()) {
            return Err("If Reference is a folder, Secondary and Tertiary must also be folders or empty.".to_string());
        }

        let mut jobs = Vec::new();
        let ref_dir_iter = std::fs::read_dir(ref_path).map_err(|e| e.to_string())?;

        for entry in ref_dir_iter {
            let ref_file_path = match entry {
                Ok(e) => e.path(),
                Err(_) => continue,
            };

            if ref_file_path.is_file() {
                let extension = ref_file_path.extension()
                .and_then(|s| s.to_str())
                .unwrap_or("");

                if VALID_EXTENSIONS.contains(&extension) {
                    let file_name = ref_file_path.file_name().unwrap();
                    let mut has_match = false;

                    let sec_file = sec_path.map(|p| p.join(file_name)).filter(|p| p.is_file());
                    if sec_file.is_some() { has_match = true; }

                    let ter_file = ter_path.map(|p| p.join(file_name)).filter(|p| p.is_file());
                    if ter_file.is_some() { has_match = true; }

                    if has_match {
                        jobs.push(Job {
                            ref_file: ref_file_path.to_string_lossy().into_owned(),
                                  sec_file: sec_file.map(|p| p.to_string_lossy().into_owned()),
                                  ter_file: ter_file.map(|p| p.to_string_lossy().into_owned()),
                        });
                    }
                }
            }
        }

        jobs.sort_by(|a, b| a.ref_file.cmp(&b.ref_file)); // Ensure consistent order
        return Ok(jobs);
    }

    Err("Reference path is not a valid file or directory.".to_string())
}
