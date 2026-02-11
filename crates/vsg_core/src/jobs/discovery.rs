//! Job discovery from source files.
//!
//! Discovers and creates processing jobs from source file paths. Supports two modes:
//!
//! 1. **Single File Mode**: Source 1 is a file. Creates one job with the exact
//!    sources provided. Other sources can be files (used directly) or omitted
//!    for remux-only mode.
//!
//! 2. **Batch Folder Mode**: Source 1 is a folder. Scans for video files
//!    (.mkv, .mp4, .m4v) and creates multiple jobs by matching filenames
//!    across all source folders. If Source 1 is a folder, all other sources
//!    must also be folders (or empty).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::types::JobQueueEntry;

/// Supported video extensions for batch folder scanning.
const VIDEO_EXTENSIONS: &[&str] = &["mkv", "mp4", "m4v"];

/// Generate a unique job ID.
fn generate_job_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    let suffix: u32 = rand::random::<u32>() % 10000;
    format!("job_{}_{:04}", timestamp, suffix)
}

/// Simple random number generator for job IDs (no external dependency).
mod rand {
    use std::cell::Cell;
    use std::time::{SystemTime, UNIX_EPOCH};

    thread_local! {
        static SEED: Cell<u64> = Cell::new(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(12345)
        );
    }

    pub fn random<T: From<u32>>() -> T {
        SEED.with(|seed| {
            let mut x = seed.get();
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            seed.set(x);
            T::from((x & 0xFFFFFFFF) as u32)
        })
    }
}

/// Derive a job name from the primary source path.
fn derive_job_name(source1: &Path) -> String {
    source1
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Unnamed Job".to_string())
}

/// Check if a path has a supported video extension.
fn is_video_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| VIDEO_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Discover jobs from source paths.
///
/// # Modes
///
/// **Single File Mode** (Source 1 is a file):
/// Creates one job with the exact sources provided. Source 1 is required.
/// Other sources are optional — if omitted, the job is still created (remux-only mode).
///
/// **Batch Folder Mode** (Source 1 is a folder):
/// Scans Source 1 folder for video files (.mkv, .mp4, .m4v). For each video file found,
/// creates a job and attempts to match by filename in other source folders.
/// If Source 1 is a folder, all other sources must also be folders or empty.
///
/// # Arguments
///
/// * `sources` - Map of source keys ("Source 1", "Source 2", etc.) to file/folder paths
///
/// # Returns
///
/// Vector of discovered jobs.
pub fn discover_jobs(sources: &HashMap<String, PathBuf>) -> Result<Vec<JobQueueEntry>, String> {
    let source1 = sources
        .get("Source 1")
        .ok_or("Source 1 (Reference) path cannot be empty.")?;

    if source1.as_os_str().is_empty() {
        return Err("Source 1 (Reference) path cannot be empty.".into());
    }

    if !source1.exists() {
        return Err(format!(
            "Source 1 path does not exist: {}",
            source1.display()
        ));
    }

    // Collect other source paths (non-empty ones)
    let other_sources: HashMap<&String, &PathBuf> = sources
        .iter()
        .filter(|(key, path)| *key != "Source 1" && !path.as_os_str().is_empty())
        .collect();

    // --- Single File Mode ---
    if source1.is_file() {
        let mut job_sources = HashMap::new();
        job_sources.insert("Source 1".to_string(), source1.clone());

        for (key, path) in &other_sources {
            if path.is_file() {
                if !path.exists() {
                    return Err(format!(
                        "{} file not found: {}",
                        key,
                        path.display()
                    ));
                }
                job_sources.insert(key.to_string(), (*path).clone());
            } else if path.is_dir() {
                // If another source is a folder in single file mode,
                // try to find a matching filename in that folder
                if let Some(file_name) = source1.file_name() {
                    let match_file = path.join(file_name);
                    if match_file.is_file() {
                        job_sources.insert(key.to_string(), match_file);
                    }
                }
            }
        }

        let job_id = generate_job_id();
        let job_name = derive_job_name(source1);
        let job = JobQueueEntry::new(job_id, job_name, job_sources);

        tracing::info!(
            "Discovered 1 job (single file): '{}' with {} source(s)",
            job.name,
            job.sources.len()
        );

        return Ok(vec![job]);
    }

    // --- Batch Folder Mode ---
    if source1.is_dir() {
        // Validate: all other sources must be folders or empty
        for (key, path) in &other_sources {
            if path.is_file() {
                return Err(format!(
                    "If Source 1 is a folder, all other sources must also be folders or empty. \
                     {} is a file: {}",
                    key,
                    path.display()
                ));
            }
            if !path.as_os_str().is_empty() && !path.exists() {
                return Err(format!(
                    "{} folder not found: {}",
                    key,
                    path.display()
                ));
            }
        }

        // Scan Source 1 folder for video files
        let mut ref_files: Vec<PathBuf> = std::fs::read_dir(source1)
            .map_err(|e| format!("Failed to read Source 1 directory: {e}"))?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_file() && is_video_file(path))
            .collect();

        ref_files.sort();

        if ref_files.is_empty() {
            return Err(format!(
                "No video files (.mkv, .mp4, .m4v) found in Source 1 folder: {}",
                source1.display()
            ));
        }

        let mut jobs = Vec::new();

        for ref_file in &ref_files {
            let mut job_sources = HashMap::new();
            job_sources.insert("Source 1".to_string(), ref_file.clone());

            // Match by filename in other source folders
            if let Some(file_name) = ref_file.file_name() {
                for (key, folder) in &other_sources {
                    if folder.is_dir() {
                        let match_file = folder.join(file_name);
                        if match_file.is_file() {
                            job_sources.insert(key.to_string(), match_file);
                        }
                    }
                }
            }

            let job_id = generate_job_id();
            let job_name = derive_job_name(ref_file);
            jobs.push(JobQueueEntry::new(job_id, job_name, job_sources));
        }

        tracing::info!(
            "Discovered {} job(s) (batch folder) from {}",
            jobs.len(),
            source1.display()
        );

        return Ok(jobs);
    }

    Err(format!(
        "Source 1 is not a valid file or directory: {}",
        source1.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::{NamedTempFile, TempDir};

    #[test]
    fn discover_requires_source1() {
        let empty: HashMap<String, PathBuf> = HashMap::new();
        assert!(discover_jobs(&empty).is_err());
    }

    #[test]
    fn single_file_mode_source1_only() {
        // Source 1 only (remux-only mode)
        let mut file1 = NamedTempFile::new().unwrap();
        writeln!(file1, "test").unwrap();

        let mut sources = HashMap::new();
        sources.insert("Source 1".to_string(), file1.path().to_path_buf());

        let result = discover_jobs(&sources).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].sources.len(), 1);
    }

    #[test]
    fn single_file_mode_two_sources() {
        let mut file1 = NamedTempFile::new().unwrap();
        let mut file2 = NamedTempFile::new().unwrap();
        writeln!(file1, "test").unwrap();
        writeln!(file2, "test").unwrap();

        let mut sources = HashMap::new();
        sources.insert("Source 1".to_string(), file1.path().to_path_buf());
        sources.insert("Source 2".to_string(), file2.path().to_path_buf());

        let result = discover_jobs(&sources).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].sources.len(), 2);
    }

    #[test]
    fn batch_folder_mode_creates_jobs() {
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();

        // Create matching video files in both folders
        fs::write(dir1.path().join("episode01.mkv"), "video1").unwrap();
        fs::write(dir1.path().join("episode02.mkv"), "video2").unwrap();
        fs::write(dir1.path().join("readme.txt"), "not a video").unwrap();
        fs::write(dir2.path().join("episode01.mkv"), "video1b").unwrap();
        fs::write(dir2.path().join("episode02.mkv"), "video2b").unwrap();

        let mut sources = HashMap::new();
        sources.insert("Source 1".to_string(), dir1.path().to_path_buf());
        sources.insert("Source 2".to_string(), dir2.path().to_path_buf());

        let result = discover_jobs(&sources).unwrap();
        assert_eq!(result.len(), 2); // 2 mkv files
        // Each job should have 2 sources (matched by filename)
        for job in &result {
            assert_eq!(job.sources.len(), 2);
        }
    }

    #[test]
    fn batch_folder_mode_partial_matches() {
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();

        fs::write(dir1.path().join("ep01.mkv"), "v1").unwrap();
        fs::write(dir1.path().join("ep02.mkv"), "v2").unwrap();
        // dir2 only has ep01
        fs::write(dir2.path().join("ep01.mkv"), "v1b").unwrap();

        let mut sources = HashMap::new();
        sources.insert("Source 1".to_string(), dir1.path().to_path_buf());
        sources.insert("Source 2".to_string(), dir2.path().to_path_buf());

        let result = discover_jobs(&sources).unwrap();
        assert_eq!(result.len(), 2);
        // ep01 should have 2 sources, ep02 should have only 1
        let ep01 = result.iter().find(|j| j.name == "ep01").unwrap();
        let ep02 = result.iter().find(|j| j.name == "ep02").unwrap();
        assert_eq!(ep01.sources.len(), 2);
        assert_eq!(ep02.sources.len(), 1);
    }

    #[test]
    fn batch_folder_mode_rejects_file_source2() {
        let dir1 = TempDir::new().unwrap();
        fs::write(dir1.path().join("ep01.mkv"), "v1").unwrap();

        let mut file2 = NamedTempFile::new().unwrap();
        writeln!(file2, "test").unwrap();

        let mut sources = HashMap::new();
        sources.insert("Source 1".to_string(), dir1.path().to_path_buf());
        sources.insert("Source 2".to_string(), file2.path().to_path_buf());

        let result = discover_jobs(&sources);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must also be folders"));
    }

    #[test]
    fn batch_folder_mode_no_videos() {
        let dir1 = TempDir::new().unwrap();
        fs::write(dir1.path().join("readme.txt"), "not video").unwrap();

        let mut sources = HashMap::new();
        sources.insert("Source 1".to_string(), dir1.path().to_path_buf());

        let result = discover_jobs(&sources);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No video files"));
    }

    #[test]
    fn job_id_is_unique() {
        let id1 = generate_job_id();
        let id2 = generate_job_id();
        assert_ne!(id1, id2);
    }
}
