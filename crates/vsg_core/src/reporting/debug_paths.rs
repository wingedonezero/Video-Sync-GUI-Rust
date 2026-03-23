//! Debug output path resolution — 1:1 port of `vsg_core/reporting/debug_paths.py`.

use std::path::{Path, PathBuf};

use crate::models::settings::AppSettings;

/// Resolved paths for all debug outputs for a single job — `DebugOutputPaths`
#[derive(Debug, Clone)]
pub struct DebugOutputPaths {
    pub output_dir: PathBuf,
    pub job_name: String,
    pub is_batch: bool,
    pub debug_root: PathBuf,
    pub ocr_debug_dir: Option<PathBuf>,
    pub frame_audit_dir: Option<PathBuf>,
    pub visual_verify_dir: Option<PathBuf>,
    pub neural_verify_dir: Option<PathBuf>,
}

impl DebugOutputPaths {
    pub fn should_create_debug_root(&self) -> bool {
        self.ocr_debug_dir.is_some()
            || self.frame_audit_dir.is_some()
            || self.visual_verify_dir.is_some()
            || self.neural_verify_dir.is_some()
    }

    pub fn get_enabled_features(&self) -> Vec<&str> {
        let mut features = Vec::new();
        if self.ocr_debug_dir.is_some() { features.push("ocr_debug"); }
        if self.frame_audit_dir.is_some() { features.push("frame_audit"); }
        if self.visual_verify_dir.is_some() { features.push("visual_verify"); }
        if self.neural_verify_dir.is_some() { features.push("neural_verify"); }
        features
    }
}

/// Resolves debug output paths — `DebugPathResolver`
pub struct DebugPathResolver;

impl DebugPathResolver {
    pub fn resolve(
        output_dir: &Path,
        job_name: &str,
        is_batch: bool,
        settings: &AppSettings,
    ) -> DebugOutputPaths {
        let debug_root = output_dir.join("debug");

        let ocr_debug_dir = if settings.ocr_debug_output {
            if is_batch {
                Some(debug_root.join("ocr_debug").join(job_name))
            } else {
                Some(debug_root.join("ocr_debug"))
            }
        } else {
            None
        };

        let frame_audit_dir = if settings.video_verified_frame_audit {
            Some(debug_root.join("frame_audit"))
        } else {
            None
        };

        let visual_verify_dir = if settings.video_verified_visual_verify {
            Some(debug_root.join("visual_verify"))
        } else {
            None
        };

        let neural_verify_dir = if settings.neural_debug_report {
            Some(debug_root.join("neural_verify"))
        } else {
            None
        };

        DebugOutputPaths {
            output_dir: output_dir.to_path_buf(),
            job_name: job_name.to_string(),
            is_batch,
            debug_root,
            ocr_debug_dir,
            frame_audit_dir,
            visual_verify_dir,
            neural_verify_dir,
        }
    }

    pub fn sanitize_job_name(source1_path: &str) -> String {
        let stem = Path::new(source1_path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        stem.chars()
            .map(|c| {
                if c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | ' ') {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>()
            .trim_matches('_')
            .to_string()
    }
}
