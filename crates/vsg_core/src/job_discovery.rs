//! Job discovery — 1:1 port of `vsg_core/job_discovery.py`.

use std::collections::HashMap;
use std::path::Path;

/// Discover jobs based on source paths — `discover_jobs`
///
/// Source 1 is the reference for filename matching.
/// Supports single-source for remux-only mode.
pub fn discover_jobs(
    sources: &HashMap<String, String>,
) -> Result<Vec<HashMap<String, String>>, String> {
    let source1_path_str = sources
        .get("Source 1")
        .filter(|s| !s.is_empty())
        .ok_or("Source 1 (Reference) path cannot be empty.")?;

    let source1_path = Path::new(source1_path_str);
    if !source1_path.exists() {
        return Err(format!(
            "Source 1 path does not exist: {}",
            source1_path.display()
        ));
    }

    let other_sources: HashMap<&str, &Path> = sources
        .iter()
        .filter(|(k, v)| *k != "Source 1" && !v.is_empty())
        .map(|(k, v)| (k.as_str(), Path::new(v.as_str())))
        .collect();

    // --- Single File Mode ---
    if source1_path.is_file() {
        let mut job_sources = HashMap::new();
        job_sources.insert("Source 1".to_string(), source1_path_str.to_string());

        for (&key, &path) in &other_sources {
            if path.is_file() {
                job_sources.insert(key.to_string(), path.to_string_lossy().to_string());
            }
        }

        return Ok(vec![job_sources]);
    }

    // --- Batch (Folder) Mode ---
    if source1_path.is_dir() {
        for (&key, &path) in &other_sources {
            if path.is_file() {
                return Err(format!(
                    "If Source 1 is a folder, all other sources must also be folders or empty. \
                     {key} is a file."
                ));
            }
        }

        let video_extensions = [".mkv", ".mp4", ".m4v"];
        let mut ref_files: Vec<_> = std::fs::read_dir(source1_path)
            .map_err(|e| format!("Failed to read Source 1 directory: {e}"))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .map(|ext| {
                            video_extensions
                                .contains(&format!(".{}", ext.to_string_lossy().to_lowercase()).as_str())
                        })
                        .unwrap_or(false)
            })
            .collect();
        ref_files.sort();

        let mut jobs = Vec::new();
        for ref_file in ref_files {
            let mut job_sources = HashMap::new();
            job_sources.insert(
                "Source 1".to_string(),
                ref_file.to_string_lossy().to_string(),
            );

            if let Some(file_name) = ref_file.file_name() {
                for (&key, &path) in &other_sources {
                    let match_file = path.join(file_name);
                    if match_file.is_file() {
                        job_sources.insert(
                            key.to_string(),
                            match_file.to_string_lossy().to_string(),
                        );
                    }
                }
            }

            jobs.push(job_sources);
        }

        return Ok(jobs);
    }

    Err("Source 1 path is not a valid file or directory.".to_string())
}
