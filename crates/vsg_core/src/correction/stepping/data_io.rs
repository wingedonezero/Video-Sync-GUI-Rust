//! Stepping data I/O — 1:1 port of `vsg_core/correction/stepping/data_io.py`.
//!
//! Serialize / deserialize dense analysis data for the stepping correction pipeline.
//! When the analysis step detects stepping, it saves the full `Vec<ChunkResult>`
//! and cluster diagnostics to a JSON file in the job's temp directory. The stepping
//! correction step reads this file instead of re-scanning from scratch.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::analysis::types::{ChunkResult, ClusterDiagnostic};

use super::types::SteppingData;

// ---------------------------------------------------------------------------
// JSON serialization types (intermediate)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct WindowJson {
    delay_ms: i32,
    raw_delay_ms: f64,
    match_pct: f64,
    start_s: f64,
    accepted: bool,
}

#[derive(Serialize, Deserialize)]
struct ClusterJson {
    cluster_id: i32,
    mean_delay_ms: f64,
    std_delay_ms: f64,
    chunk_count: usize,
    chunk_numbers: Vec<i32>,
    raw_delays: Vec<f64>,
    time_range: (f64, f64),
    mean_match_pct: f64,
    min_match_pct: f64,
}

#[derive(Serialize, Deserialize)]
struct SteppingDataJson {
    source_key: String,
    track_id: i32,
    windows: Vec<WindowJson>,
    clusters: Vec<ClusterJson>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Persist dense analysis data to `{temp_dir}/stepping_data/{source}_{track}.json` — `save_stepping_data`
pub fn save_stepping_data(
    temp_dir: &Path,
    source_key: &str,
    track_id: i32,
    chunk_results: &[ChunkResult],
    cluster_details: &[ClusterDiagnostic],
) -> Result<PathBuf, String> {
    let out_dir = temp_dir.join("stepping_data");
    fs::create_dir_all(&out_dir)
        .map_err(|e| format!("Failed to create stepping_data dir: {e}"))?;

    // Sanitise source_key for file-name safety (spaces -> underscores)
    let safe_source = source_key.replace(' ', "_");
    let out_path = out_dir.join(format!("{safe_source}_{track_id}.json"));

    let payload = SteppingDataJson {
        source_key: source_key.to_string(),
        track_id,
        windows: chunk_results
            .iter()
            .map(|r| WindowJson {
                delay_ms: r.delay_ms,
                raw_delay_ms: r.raw_delay_ms,
                match_pct: r.match_pct,
                start_s: r.start_s,
                accepted: r.accepted,
            })
            .collect(),
        clusters: cluster_details
            .iter()
            .map(|c| ClusterJson {
                cluster_id: c.cluster_id,
                mean_delay_ms: c.mean_delay_ms,
                std_delay_ms: c.std_delay_ms,
                chunk_count: c.chunk_count,
                chunk_numbers: c.chunk_numbers.clone(),
                raw_delays: c.raw_delays.clone(),
                time_range: c.time_range,
                mean_match_pct: c.mean_match_pct,
                min_match_pct: c.min_match_pct,
            })
            .collect(),
    };

    let json_str = serde_json::to_string_pretty(&payload)
        .map_err(|e| format!("JSON serialization failed: {e}"))?;

    fs::write(&out_path, json_str)
        .map_err(|e| format!("Failed to write stepping data: {e}"))?;

    Ok(out_path)
}

/// Read a previously-saved JSON and reconstitute typed objects — `load_stepping_data`
pub fn load_stepping_data(path: &str) -> Result<SteppingData, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read stepping data from {path}: {e}"))?;

    let raw: SteppingDataJson = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse stepping data JSON: {e}"))?;

    let windows: Vec<ChunkResult> = raw
        .windows
        .into_iter()
        .map(|w| ChunkResult {
            delay_ms: w.delay_ms,
            raw_delay_ms: w.raw_delay_ms,
            match_pct: w.match_pct,
            start_s: w.start_s,
            accepted: w.accepted,
        })
        .collect();

    let clusters: Vec<ClusterDiagnostic> = raw
        .clusters
        .into_iter()
        .map(|c| ClusterDiagnostic {
            cluster_id: c.cluster_id,
            mean_delay_ms: c.mean_delay_ms,
            std_delay_ms: c.std_delay_ms,
            chunk_count: c.chunk_count,
            chunk_numbers: c.chunk_numbers,
            raw_delays: c.raw_delays,
            time_range: c.time_range,
            mean_match_pct: c.mean_match_pct,
            min_match_pct: c.min_match_pct,
        })
        .collect();

    Ok(SteppingData {
        source_key: raw.source_key,
        track_id: raw.track_id,
        windows,
        clusters,
    })
}
