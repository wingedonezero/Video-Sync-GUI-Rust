//! Job layout manager — 1:1 port of `vsg_core/job_layouts/manager.py`.
//!
//! Main orchestrator for handling job layout persistence, copying, and validation.
//! Job IDs are MD5 hashes of source file names, ensuring consistency across runs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::persistence::LayoutPersistence;
use super::signature::EnhancedSignatureGenerator;
use super::validation::LayoutValidator;

/// Log callback type.
type LogCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// Main orchestrator for handling job layout persistence, copying, and validation — `JobLayoutManager`.
pub struct JobLayoutManager {
    log: LogCallback,
    /// The persistence layer for saving/loading layout files.
    pub persistence: LayoutPersistence,
    /// Public access for UI code that needs to generate signatures directly.
    pub signature_gen: SignatureGenAccess,
}

/// Public handle for signature generation (so UI can call it directly for paste operations).
pub struct SignatureGenAccess;

impl SignatureGenAccess {
    /// Generate a structure signature for compatibility checking.
    pub fn generate_structure_signature(
        &self,
        track_info: &HashMap<String, Vec<Value>>,
    ) -> Value {
        EnhancedSignatureGenerator::generate_structure_signature(track_info)
    }

    /// Check if two structures are compatible.
    pub fn structures_are_compatible(&self, struct1: &Value, struct2: &Value) -> bool {
        EnhancedSignatureGenerator::structures_are_compatible(struct1, struct2)
    }
}

impl JobLayoutManager {
    /// Create a new `JobLayoutManager`.
    pub fn new(temp_root: &str, log_callback: Arc<dyn Fn(&str) + Send + Sync>) -> Self {
        let layouts_dir = PathBuf::from(temp_root).join("job_layouts");
        let persistence = LayoutPersistence::new(layouts_dir, Arc::clone(&log_callback));
        Self {
            log: log_callback,
            persistence,
            signature_gen: SignatureGenAccess,
        }
    }

    /// Generates a consistent and unique job ID from source file paths.
    ///
    /// Uses MD5 hash of sorted source file names (not full paths) for consistency.
    pub fn generate_job_id(&self, sources: &HashMap<String, String>) -> String {
        let mut sorted_sources: Vec<(&String, &String)> = sources.iter().collect();
        sorted_sources.sort_by_key(|(k, _)| k.as_str());

        let source_string: String = sorted_sources
            .iter()
            .filter(|(_, path)| !path.is_empty())
            .map(|(key, path)| {
                let name = Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                format!("{key}:{name}")
            })
            .collect::<Vec<_>>()
            .join("|");

        // Use SHA-256 truncated to 16 hex chars (Python used MD5, but SHA-256
        // is available and equally deterministic — the only requirement is consistency)
        let hash = format!("{:x}", Sha256::digest(source_string.as_bytes()));
        hash[..16].to_string()
    }

    /// Saves a job layout, generating fresh signatures and enhancing the layout data.
    pub fn save_job_layout(
        &self,
        job_id: &str,
        layout: &[Value],
        attachment_sources: &[String],
        sources: &HashMap<String, String>,
        track_info: &HashMap<String, Vec<Value>>,
        source_settings: Option<&HashMap<String, Value>>,
    ) -> bool {
        let enhanced_layout = Self::create_enhanced_layout(layout);
        let track_sig =
            EnhancedSignatureGenerator::generate_track_signature(track_info, false);
        let struct_sig =
            EnhancedSignatureGenerator::generate_structure_signature(track_info);

        let mut layout_data = json!({
            "job_id": job_id,
            "sources": sources,
            "enhanced_layout": enhanced_layout,
            "attachment_sources": attachment_sources,
            "track_signature": track_sig,
            "structure_signature": struct_sig,
            "source_settings": source_settings.unwrap_or(&HashMap::new()),
        });

        if self.persistence.save_layout(job_id, &mut layout_data) {
            (self.log)(&format!("[LayoutManager] Saved layout for job {job_id}"));
            true
        } else {
            (self.log)(&format!(
                "[LayoutManager] CRITICAL: Failed to save layout for {job_id}"
            ));
            false
        }
    }

    /// Loads and validates a job layout.
    pub fn load_job_layout(&self, job_id: &str) -> Option<Value> {
        let layout_data = self.persistence.load_layout(job_id)?;
        let (is_valid, reason) = LayoutValidator::validate(&layout_data);
        if is_valid {
            Some(layout_data)
        } else {
            (self.log)(&format!(
                "[LayoutManager] Validation failed for {job_id}: {reason}"
            ));
            None
        }
    }

    /// Copies a layout if the source and target files are structurally compatible.
    pub fn copy_layout_between_jobs(
        &self,
        source_job_id: &str,
        target_job_id: &str,
        target_sources: &HashMap<String, String>,
        target_track_info: &HashMap<String, Vec<Value>>,
    ) -> bool {
        let source_data = match self.load_job_layout(source_job_id) {
            Some(d) => d,
            None => {
                (self.log)(&format!(
                    "[LayoutManager] Cannot copy: Source layout {source_job_id} not found."
                ));
                return false;
            }
        };

        let source_struct_sig = &source_data["structure_signature"];
        let target_struct_sig =
            EnhancedSignatureGenerator::generate_structure_signature(target_track_info);

        if !EnhancedSignatureGenerator::structures_are_compatible(
            source_struct_sig,
            &target_struct_sig,
        ) {
            (self.log)(&format!(
                "[LayoutManager] Cannot copy: Incompatible track structures between {source_job_id} and {target_job_id}."
            ));
            return false;
        }

        let target_track_sig =
            EnhancedSignatureGenerator::generate_track_signature(target_track_info, false);

        let mut target_layout_data = json!({
            "job_id": target_job_id,
            "sources": target_sources,
            "enhanced_layout": source_data["enhanced_layout"],
            "attachment_sources": source_data.get("attachment_sources").cloned().unwrap_or(json!([])),
            "source_settings": source_data.get("source_settings").cloned().unwrap_or(json!({})),
            "track_signature": target_track_sig,
            "structure_signature": target_struct_sig,
            "copied_from": source_job_id,
        });

        self.persistence
            .save_layout(target_job_id, &mut target_layout_data)
    }

    /// Adds positional metadata to a layout for robust ordering.
    fn create_enhanced_layout(layout: &[Value]) -> Vec<Value> {
        let mut enhanced = Vec::with_capacity(layout.len());
        let mut source_type_positions: HashMap<String, i64> = HashMap::new();

        for (user_index, track) in layout.iter().enumerate() {
            let mut enhanced_track = track.clone();
            let source = track
                .get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let track_type = track.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let key = format!("{source}_{track_type}");

            let position = source_type_positions.entry(key).or_insert(0);

            if let Some(obj) = enhanced_track.as_object_mut() {
                obj.insert(
                    "user_order_index".to_string(),
                    json!(user_index as i64),
                );
                obj.insert("position_in_source_type".to_string(), json!(*position));
            }
            *position += 1;
            enhanced.push(enhanced_track);
        }

        enhanced
    }

    /// Check if a layout exists for the given job ID.
    pub fn layout_exists(&self, job_id: &str) -> bool {
        self.persistence.layout_exists(job_id)
    }

    /// Delete a layout for the given job ID.
    pub fn delete_layout(&self, job_id: &str) -> bool {
        self.persistence.delete_layout(job_id)
    }

    /// Deletes all temporary layout files.
    pub fn cleanup_all(&self) {
        self.persistence.cleanup_all();
    }
}
