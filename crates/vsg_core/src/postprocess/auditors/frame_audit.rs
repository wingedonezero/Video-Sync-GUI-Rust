//! Frame audit auditor — 1:1 port of `vsg_core/postprocess/auditors/frame_audit.py`.
//!
//! Reports frame audit results from video-verified subtitle sync.

use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::orchestrator::steps::context::Context;

use super::base::Auditor;

/// Reports frame audit results — `FrameAuditAuditor`
pub struct FrameAuditAuditor;

impl Auditor for FrameAuditAuditor {
    fn run(
        &self,
        ctx: &Context,
        runner: &CommandRunner,
        _final_mkv_path: &Path,
        _final_mkvmerge_data: &serde_json::Value,
        _final_ffprobe_data: Option<&serde_json::Value>,
    ) -> i32 {
        let mut issues = 0;

        if ctx.frame_audit_results.is_empty() {
            return 0;
        }

        for (source, audit_data) in &ctx.frame_audit_results {
            let passed = audit_data
                .get("passed")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let total_frames = audit_data
                .get("total_frames")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            let matched_frames = audit_data
                .get("matched_frames")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            let match_rate = if total_frames > 0 {
                (matched_frames as f64 / total_frames as f64) * 100.0
            } else {
                0.0
            };

            if passed {
                runner.log_message(&format!(
                    "  \u{2714} Frame audit for {}: {:.1}% match ({}/{} frames)",
                    source, match_rate, matched_frames, total_frames
                ));
            } else {
                runner.log_message(&format!(
                    "  \u{26a0} Frame audit FAILED for {}: {:.1}% match ({}/{} frames)",
                    source, match_rate, matched_frames, total_frames
                ));
                issues += 1;
            }

            // Report individual mismatches if present
            if let Some(mismatches) = audit_data.get("mismatches").and_then(|v| v.as_array()) {
                let max_report = 5;
                for mismatch in mismatches.iter().take(max_report) {
                    let frame_idx = mismatch
                        .get("frame_index")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    let time_s = mismatch
                        .get("time_s")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let score = mismatch
                        .get("score")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);

                    runner.log_message(&format!(
                        "    - Mismatch at frame {} ({:.2}s): score={:.3}",
                        frame_idx, time_s, score
                    ));
                }

                if mismatches.len() > max_report {
                    runner.log_message(&format!(
                        "    ... and {} more mismatches",
                        mismatches.len() - max_report
                    ));
                }
            }
        }

        issues
    }
}
