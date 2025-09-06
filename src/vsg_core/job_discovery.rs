// src/vsg_core/job_discovery.rs

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Job {
    pub ref_file: PathBuf,
    pub sec_file: Option<PathBuf>,
    pub ter_file: Option<PathBuf>,
}

/// Discovers jobs based on input paths.
pub fn discover_jobs(
    ref_path_str: &str,
    sec_path_str: Option<&str>,
    ter_path_str: Option<&str>,
) -> Result<Vec<Job>> {
    let ref_path = PathBuf::from(ref_path_str);
    if !ref_path.exists() {
        return Err(anyhow!("Reference path does not exist: {}", ref_path.display()));
    }

    let sec_path = sec_path_str.map(PathBuf::from);
    let ter_path = ter_path_str.map(PathBuf::from);

    // --- Single File Mode ---
    if ref_path.is_file() {
        let job = Job {
            ref_file: ref_path,
            sec_file: sec_path.filter(|p| p.is_file()),
            ter_file: ter_path.filter(|p| p.is_file()),
        };
        return Ok(vec![job]);
    }

    // --- Batch (Folder) Mode ---
    if ref_path.is_dir() {
        let mut jobs = Vec::new();
        let video_extensions = ["mkv", "mp4", "m4v"];

        for entry in std::fs::read_dir(ref_path)? {
            let entry = entry?;
            let ref_file = entry.path();

            if ref_file.is_file() {
                if let Some(ext) = ref_file.extension().and_then(|s| s.to_str()) {
                    if !video_extensions.contains(&ext) {
                        continue;
                    }
                } else {
                    continue;
                }

                let file_name = ref_file.file_name().unwrap();
                let mut job = Job { ref_file, sec_file: None, ter_file: None };

                if let Some(sec_dir) = &sec_path {
                    let sec_match = sec_dir.join(file_name);
                    if sec_match.is_file() {
                        job.sec_file = Some(sec_match);
                    }
                }

                if let Some(ter_dir) = &ter_path {
                    let ter_match = ter_dir.join(file_name);
                    if ter_match.is_file() {
                        job.ter_file = Some(ter_match);
                    }
                }

                // A job is only valid if it has at least one other file to sync against.
                if job.sec_file.is_some() || job.ter_file.is_some() {
                    jobs.push(job);
                }
            }
        }
        jobs.sort_by(|a, b| a.ref_file.cmp(&b.ref_file));
        return Ok(jobs);
    }

    Err(anyhow!("Reference path is not a valid file or directory."))
}
