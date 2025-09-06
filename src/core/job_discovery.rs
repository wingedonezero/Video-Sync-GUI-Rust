// src/core/job_discovery.rs
//
// Rust port of vsg_core/job_discovery.py
// - Discovers jobs based on input paths (files or directories)
// - Single-file mode: always returns exactly one job (sec/ter optional)
// - Batch mode (folder): matches files by identical filename; only include
//   a job if at least one of SEC/TER has a matching file present
// - Valid video extensions: .mkv, .mp4, .m4v
// - Errors mirror Python semantics

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Job {
    pub ref_path: String,
    pub sec: Option<String>,
    pub ter: Option<String>,
}

fn is_video_ext(p: &Path) -> bool {
    match p.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) {
        Some(ext) if ext == "mkv" || ext == "mp4" || ext == "m4v" => true,
        _ => false,
    }
}

/// Discovers jobs based on the three paths. Returns an error string on invalid usage.
/// Mirrors Python's `discover_jobs`.
pub fn discover_jobs(
    ref_path_str: &str,
    sec_path_str: Option<&str>,
    ter_path_str: Option<&str>,
) -> Result<Vec<Job>, String> {
    if ref_path_str.trim().is_empty() {
        return Err("Reference path cannot be empty.".to_string());
    }

    let ref_path = PathBuf::from(ref_path_str);
    if !ref_path.exists() {
        return Err(format!("Reference path does not exist: {}", ref_path.display()));
    }

    let sec_path = sec_path_str.map(PathBuf::from);
    let ter_path = ter_path_str.map(PathBuf::from);

    // ---- Single File Mode ----
    if ref_path.is_file() {
        let sec_file = match &sec_path {
            Some(p) if p.is_file() => Some(p.to_string_lossy().to_string()),
            _ => None,
        };
        let ter_file = match &ter_path {
            Some(p) if p.is_file() => Some(p.to_string_lossy().to_string()),
            _ => None,
        };
        let job = Job {
            ref_path: ref_path.to_string_lossy().to_string(),
            sec: sec_file,
            ter: ter_file,
        };
        return Ok(vec![job]);
    }

    // ---- Batch (Folder) Mode ----
    if ref_path.is_dir() {
        // If REF is folder, SEC/TER must be folders or empty (not files)
        if sec_path.as_ref().map(|p| p.is_file()).unwrap_or(false)
            || ter_path.as_ref().map(|p| p.is_file()).unwrap_or(false)
            {
                return Err(
                    "If Reference is a folder, Secondary and Tertiary must also be folders or empty."
                    .to_string(),
                );
            }

            let mut jobs: Vec<Job> = Vec::new();

        // List ref files with valid video extensions
        let mut ref_files: Vec<PathBuf> = Vec::new();
        match fs::read_dir(&ref_path) {
            Ok(entries) => {
                for ent in entries.flatten() {
                    let p = ent.path();
                    if p.is_file() && is_video_ext(&p) {
                        ref_files.push(p);
                    }
                }
            }
            Err(e) => return Err(format!("Failed to read directory {}: {}", ref_path.display(), e)),
        }
        ref_files.sort_by_key(|p| p.file_name().map(|s| s.to_os_string()).unwrap_or_default());

        for ref_file in ref_files {
            let file_name = match ref_file.file_name() {
                Some(n) => n,
                None => continue,
            };

            let sec_candidate = sec_path
            .as_ref()
            .map(|dir| dir.join(file_name))
            .filter(|p| p.is_file())
            .map(|p| p.to_string_lossy().to_string());

            let ter_candidate = ter_path
            .as_ref()
            .map(|dir| dir.join(file_name))
            .filter(|p| p.is_file())
            .map(|p| p.to_string_lossy().to_string());

            // Only include if at least one of sec/ter exists
            if sec_candidate.is_some() || ter_candidate.is_some() {
                jobs.push(Job {
                    ref_path: ref_file.to_string_lossy().to_string(),
                          sec: sec_candidate,
                          ter: ter_candidate,
                });
            }
        }

        // Empty jobs in folder mode is not an error (UI handles "No Jobs Found")
        return Ok(jobs);
    }

    Err("Reference path is not a valid file or directory.".to_string())
}
