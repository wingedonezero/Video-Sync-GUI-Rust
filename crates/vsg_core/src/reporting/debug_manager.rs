//! Debug output lifecycle management — 1:1 port of `vsg_core/reporting/debug_manager.py`.

use std::fs;
use std::io::Write;
use std::path::Path;

use crate::models::settings::AppSettings;

use super::debug_paths::{DebugOutputPaths, DebugPathResolver};

/// Manages debug output directories and archiving — `DebugOutputManager`
pub struct DebugOutputManager {
    output_dir: std::path::PathBuf,
    is_batch: bool,
    settings: AppSettings,
    job_paths: std::collections::HashMap<String, DebugOutputPaths>,
}

impl DebugOutputManager {
    pub fn new(output_dir: &Path, is_batch: bool, settings: AppSettings) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            is_batch,
            settings,
            job_paths: std::collections::HashMap::new(),
        }
    }

    pub fn register_job(&mut self, job_name: &str) -> DebugOutputPaths {
        let paths = DebugPathResolver::resolve(
            &self.output_dir, job_name, self.is_batch, &self.settings,
        );
        self.job_paths.insert(job_name.to_string(), paths.clone());

        if paths.should_create_debug_root() {
            Self::create_directories(&paths);
        }
        paths
    }

    fn create_directories(paths: &DebugOutputPaths) {
        let _ = fs::create_dir_all(&paths.debug_root);
        if let Some(ref dir) = paths.ocr_debug_dir { let _ = fs::create_dir_all(dir); }
        if let Some(ref dir) = paths.frame_audit_dir { let _ = fs::create_dir_all(dir); }
        if let Some(ref dir) = paths.visual_verify_dir { let _ = fs::create_dir_all(dir); }
    }

    pub fn finalize_batch(&self, log: &dyn Fn(&str)) {
        if !self.is_batch || self.job_paths.is_empty() {
            return;
        }

        let sample_paths = match self.job_paths.values().next() {
            Some(p) => p,
            None => return,
        };

        if !sample_paths.debug_root.exists() {
            return;
        }

        log(&format!(
            "[DebugManager] Archiving debug outputs in {}",
            sample_paths.debug_root.display()
        ));

        for feature_name in sample_paths.get_enabled_features() {
            self.zip_debug_feature(&sample_paths.debug_root, feature_name, log);
        }

        log("[DebugManager] Debug output archiving complete");
    }

    fn zip_debug_feature(&self, debug_root: &Path, feature_name: &str, log: &dyn Fn(&str)) {
        let feature_dir = debug_root.join(feature_name);
        if !feature_dir.exists() {
            return;
        }

        // Check for actual files
        let has_files = walkdir_has_files(&feature_dir);
        if !has_files {
            log(&format!("[DebugManager] {feature_name}/ has no files, skipping archive"));
            let _ = fs::remove_dir_all(&feature_dir);
            return;
        }

        let zip_path = debug_root.join(format!("{feature_name}.zip"));

        match create_zip(&feature_dir, &zip_path) {
            Ok(()) => {
                let _ = fs::remove_dir_all(&feature_dir);
                log(&format!(
                    "[DebugManager] Created archive: {}",
                    zip_path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
            Err(e) => {
                log(&format!("[DebugManager] ERROR: Failed to archive {feature_name}/: {e}"));
            }
        }
    }

    pub fn has_any_debug_enabled(&self) -> bool {
        self.settings.ocr_debug_output
            || self.settings.video_verified_frame_audit
            || self.settings.video_verified_visual_verify
    }

    pub fn get_job_paths(&self, job_name: &str) -> Option<&DebugOutputPaths> {
        self.job_paths.get(job_name)
    }
}

fn walkdir_has_files(dir: &Path) -> bool {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                return true;
            }
            if path.is_dir() && walkdir_has_files(&path) {
                return true;
            }
        }
    }
    false
}

fn create_zip(source_dir: &Path, zip_path: &Path) -> Result<(), String> {
    // Simple zip implementation using std::io
    // For production, consider the `zip` crate, but for now we use a basic approach
    let file = fs::File::create(zip_path)
        .map_err(|e| format!("Failed to create zip: {e}"))?;
    let mut zip = zip::ZipWriter::new(file);

    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    fn add_dir_to_zip(
        zip: &mut zip::ZipWriter<fs::File>,
        dir: &Path,
        base: &Path,
        options: zip::write::SimpleFileOptions,
    ) -> Result<(), String> {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let rel = path.strip_prefix(base).unwrap_or(&path);
                if path.is_file() {
                    zip.start_file(rel.to_string_lossy(), options)
                        .map_err(|e| format!("Zip error: {e}"))?;
                    let data = fs::read(&path)
                        .map_err(|e| format!("Read error: {e}"))?;
                    zip.write_all(&data)
                        .map_err(|e| format!("Write error: {e}"))?;
                } else if path.is_dir() {
                    add_dir_to_zip(zip, &path, base, options)?;
                }
            }
        }
        Ok(())
    }

    add_dir_to_zip(&mut zip, source_dir, source_dir, options)?;
    zip.finish().map_err(|e| format!("Zip finish error: {e}"))?;
    Ok(())
}
