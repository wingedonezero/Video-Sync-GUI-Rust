//! Neural confidence auditor — 1:1 port of `vsg_core/postprocess/auditors/neural_confidence.py`.
//!
//! Reports neural matching confidence from video-verified sync results.

use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::orchestrator::steps::context::Context;

use super::base::Auditor;

/// Reports neural matching confidence — `NeuralConfidenceAuditor`
pub struct NeuralConfidenceAuditor;

impl Auditor for NeuralConfidenceAuditor {
    fn run(
        &self,
        ctx: &Context,
        runner: &CommandRunner,
        _final_mkv_path: &Path,
        _final_mkvmerge_data: &serde_json::Value,
        _final_ffprobe_data: Option<&serde_json::Value>,
    ) -> i32 {
        let mut issues = 0;

        if ctx.video_verified_sources.is_empty() {
            return 0;
        }

        for (source, result) in &ctx.video_verified_sources {
            let details = match &result.details {
                Some(d) => d,
                None => continue,
            };

            let method = details
                .get("method")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if method != "neural" {
                continue;
            }

            let confidence = details
                .get("confidence")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);

            let match_count = details
                .get("match_count")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            let total_count = details
                .get("total_count")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            if confidence >= 0.8 {
                runner.log_message(&format!(
                    "  \u{2714} Neural confidence for {}: {:.1}% ({}/{} matches)",
                    source,
                    confidence * 100.0,
                    match_count,
                    total_count
                ));
            } else if confidence >= 0.5 {
                runner.log_message(&format!(
                    "  \u{26a0} Low neural confidence for {}: {:.1}% ({}/{} matches)",
                    source,
                    confidence * 100.0,
                    match_count,
                    total_count
                ));
                // Low confidence is informational, not an issue
            } else {
                runner.log_message(&format!(
                    "  \u{26a0} Very low neural confidence for {}: {:.1}% ({}/{} matches)",
                    source,
                    confidence * 100.0,
                    match_count,
                    total_count
                ));
                issues += 1;
            }

            // Report correction applied
            if let (Some(original), Some(corrected)) =
                (result.original_delay_ms, result.corrected_delay_ms)
            {
                let adjustment = corrected - original;
                if adjustment.abs() > 0.5 {
                    runner.log_message(&format!(
                        "  \u{2139} Neural correction for {}: {:.1}ms -> {:.1}ms \
                         (adjustment: {:.1}ms)",
                        source, original, corrected, adjustment
                    ));
                }
            }
        }

        issues
    }
}
