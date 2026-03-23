//! Pipeline audit trail — 1:1 port of `vsg_core/audit/trail.py`.
//!
//! Creates a JSON file in the job's temp folder that records every
//! timing-related value at each pipeline step. Atomic writes, append-only.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;
use serde_json::{json, Value};

/// Pipeline audit trail — `AuditTrail`
pub struct AuditTrail {
    temp_dir: PathBuf,
    file_path: PathBuf,
    data: Value,
}

impl AuditTrail {
    const VERSION: &'static str = "1.0";
    const FILENAME: &'static str = "pipeline_audit_trail.json";

    pub fn new(temp_dir: &Path, job_name: &str) -> Self {
        let file_path = temp_dir.join(Self::FILENAME);
        let data = json!({
            "_metadata": {
                "version": Self::VERSION,
                "created_at": Local::now().to_rfc3339(),
                "job_name": job_name,
                "temp_dir": temp_dir.to_string_lossy(),
                "output_file": null,
            },
            "sources": {},
            "analysis": {
                "correlations": {},
                "container_delays": {},
                "delay_calculations": {},
                "global_shift": {},
                "final_delays": {},
            },
            "extraction": { "tracks": [] },
            "stepping": {},
            "subtitle_processing": {},
            "mux": { "track_delays": [], "tokens": [] },
            "events": [],
        });

        let trail = Self {
            temp_dir: temp_dir.to_path_buf(),
            file_path,
            data,
        };
        trail.write();
        trail
    }

    /// Record a value at dot-separated path — `record`
    pub fn record(&mut self, path: &str, value: Value, merge: bool) {
        let parts: Vec<&str> = path.split('.').collect();
        let mut target = &mut self.data;

        for &part in &parts[..parts.len() - 1] {
            if !target.get(part).map(|v| v.is_object()).unwrap_or(false) {
                target[part] = json!({});
            }
            target = &mut target[part];
        }

        let final_key = parts[parts.len() - 1];
        if merge && target.get(final_key).map(|v| v.is_object()).unwrap_or(false) && value.is_object() {
            if let (Some(existing), Some(new_obj)) = (
                target[final_key].as_object_mut(),
                value.as_object(),
            ) {
                for (k, v) in new_obj {
                    existing.insert(k.clone(), v.clone());
                }
            }
        } else {
            target[final_key] = value;
        }

        self.write();
    }

    /// Append a value to a list at path — `append`
    pub fn append(&mut self, path: &str, value: Value) {
        let parts: Vec<&str> = path.split('.').collect();
        let mut target = &mut self.data;

        for &part in &parts[..parts.len() - 1] {
            if !target.get(part).map(|v| v.is_object()).unwrap_or(false) {
                target[part] = json!({});
            }
            target = &mut target[part];
        }

        let final_key = parts[parts.len() - 1];
        if !target.get(final_key).map(|v| v.is_array()).unwrap_or(false) {
            target[final_key] = json!([]);
        }
        if let Some(arr) = target[final_key].as_array_mut() {
            arr.push(value);
        }

        self.write();
    }

    /// Append a timestamped event — `append_event`
    pub fn append_event(&mut self, event_type: &str, message: &str, data: Option<Value>) {
        let mut event = json!({
            "timestamp": Local::now().to_rfc3339(),
            "type": event_type,
            "message": message,
        });
        if let Some(d) = data {
            event["data"] = d;
        }
        if let Some(events) = self.data["events"].as_array_mut() {
            events.push(event);
        }
        self.write();
    }

    /// Record source file info — `record_source`
    pub fn record_source(&mut self, source_key: &str, file_path: &str) {
        self.record(
            &format!("sources.{source_key}"),
            json!({
                "file_path": file_path,
                "recorded_at": Local::now().to_rfc3339(),
            }),
            true,
        );
    }

    /// Record correlation chunk — `record_correlation_chunk`
    #[allow(clippy::too_many_arguments)]
    pub fn record_correlation_chunk(
        &mut self,
        source_key: &str,
        chunk_idx: i32,
        start_s: f64,
        delay_ms: i32,
        raw_delay_ms: f64,
        match_pct: f64,
        accepted: bool,
    ) {
        self.append(
            &format!("analysis.correlations.{source_key}.chunks"),
            json!({
                "chunk_idx": chunk_idx,
                "start_s": (start_s * 1000.0).round() / 1000.0,
                "delay_ms": delay_ms,
                "raw_delay_ms": (raw_delay_ms * 1_000_000.0).round() / 1_000_000.0,
                "match_pct": (match_pct * 10000.0).round() / 10000.0,
                "accepted": accepted,
            }),
        );
    }

    /// Record delay calculation chain — `record_delay_calculation`
    #[allow(clippy::too_many_arguments)]
    pub fn record_delay_calculation(
        &mut self,
        source_key: &str,
        correlation_raw_ms: f64,
        correlation_rounded_ms: i32,
        container_delay_ms: f64,
        final_raw_ms: f64,
        final_rounded_ms: i32,
        selection_method: &str,
        accepted_windows: usize,
        total_windows: usize,
    ) {
        self.record(
            &format!("analysis.delay_calculations.{source_key}"),
            json!({
                "correlation": {
                    "raw_ms": (correlation_raw_ms * 1_000_000.0).round() / 1_000_000.0,
                    "rounded_ms": correlation_rounded_ms,
                    "selection_method": selection_method,
                    "accepted_windows": accepted_windows,
                    "total_windows": total_windows,
                },
                "container_delay_ms": (container_delay_ms * 1_000_000.0).round() / 1_000_000.0,
                "before_global_shift": {
                    "raw_ms": (final_raw_ms * 1_000_000.0).round() / 1_000_000.0,
                    "rounded_ms": final_rounded_ms,
                },
            }),
            false,
        );
    }

    /// Record global shift — `record_global_shift`
    pub fn record_global_shift(
        &mut self,
        most_negative_raw_ms: f64,
        most_negative_rounded_ms: i32,
        shift_raw_ms: f64,
        shift_rounded_ms: i32,
        sync_mode: &str,
    ) {
        self.record(
            "analysis.global_shift",
            json!({
                "sync_mode": sync_mode,
                "most_negative_delay": {
                    "raw_ms": (most_negative_raw_ms * 1_000_000.0).round() / 1_000_000.0,
                    "rounded_ms": most_negative_rounded_ms,
                },
                "calculated_shift": {
                    "raw_ms": (shift_raw_ms * 1_000_000.0).round() / 1_000_000.0,
                    "rounded_ms": shift_rounded_ms,
                },
            }),
            false,
        );
    }

    /// Record final delay — `record_final_delay`
    pub fn record_final_delay(
        &mut self,
        source_key: &str,
        raw_ms: f64,
        rounded_ms: i32,
        includes_global_shift: bool,
    ) {
        self.record(
            &format!("analysis.final_delays.{source_key}"),
            json!({
                "raw_ms": (raw_ms * 1_000_000.0).round() / 1_000_000.0,
                "rounded_ms": rounded_ms,
                "includes_global_shift": includes_global_shift,
            }),
            false,
        );
    }

    /// Record mux track delay — `record_mux_track_delay`
    #[allow(clippy::too_many_arguments)]
    pub fn record_mux_track_delay(
        &mut self,
        track_idx: i32,
        source: &str,
        track_type: &str,
        track_id: i32,
        final_delay_ms: i32,
        reason: &str,
        raw_delay_available_ms: Option<f64>,
        stepping_adjusted: bool,
        frame_adjusted: bool,
        sync_key: Option<&str>,
    ) {
        let mut entry = json!({
            "track_idx": track_idx,
            "source": source,
            "track_type": track_type,
            "track_id": track_id,
            "final_delay_ms": final_delay_ms,
            "reason": reason,
            "sync_key": sync_key,
            "flags": {
                "stepping_adjusted": stepping_adjusted,
                "frame_adjusted": frame_adjusted,
            },
        });
        if let Some(raw) = raw_delay_available_ms {
            entry["raw_delay_available_ms"] = json!((raw * 1_000_000.0).round() / 1_000_000.0);
        }
        self.append("mux.track_delays", entry);
    }

    /// Record mux tokens — `record_mux_tokens`
    pub fn record_mux_tokens(&mut self, tokens: &[String]) {
        self.record("mux.tokens", json!(tokens), false);
        let preview = if tokens.len() > 20 {
            format!("{}...", tokens[..20].join(" "))
        } else {
            tokens.join(" ")
        };
        self.record("mux.command_preview", json!(preview), false);
    }

    /// Get the file path — `get_path`
    pub fn get_path(&self) -> &Path {
        &self.file_path
    }

    /// Finalize the trail — `finalize`
    pub fn finalize(&mut self, output_file: Option<&str>, success: bool) {
        self.data["_metadata"]["finalized_at"] = json!(Local::now().to_rfc3339());
        self.data["_metadata"]["success"] = json!(success);
        if let Some(path) = output_file {
            self.data["_metadata"]["output_file"] = json!(path);
        }
        self.write();
    }

    /// Atomic write to disk — `_write`
    fn write(&self) {
        let _ = fs::create_dir_all(&self.temp_dir);
        let json_str = serde_json::to_string_pretty(&self.data).unwrap_or_default();

        // Atomic write: temp file + rename
        let temp_path = self.temp_dir.join("audit_.tmp");
        if fs::write(&temp_path, &json_str).is_ok() {
            let _ = fs::rename(&temp_path, &self.file_path);
        }
    }
}
