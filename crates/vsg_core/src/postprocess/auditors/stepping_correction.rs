//! Stepping correction auditor — 1:1 port of `vsg_core/postprocess/auditors/stepping_correction.py`.
//!
//! Verifies that stepping corrections were applied correctly.

use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::models::enums::TrackType;
use crate::orchestrator::steps::context::Context;

use super::base::Auditor;

/// Verifies stepping corrections — `SteppingCorrectionAuditor`
pub struct SteppingCorrectionAuditor;

impl Auditor for SteppingCorrectionAuditor {
    fn run(
        &self,
        ctx: &Context,
        runner: &CommandRunner,
        _final_mkv_path: &Path,
        _final_mkvmerge_data: &serde_json::Value,
        _final_ffprobe_data: Option<&serde_json::Value>,
    ) -> i32 {
        let mut issues = 0;

        let plan_items = match &ctx.extracted_items {
            Some(items) => items,
            None => return 0,
        };

        // Report stepping sources
        if !ctx.stepping_sources.is_empty() {
            runner.log_message(&format!(
                "  \u{2139} Stepping correction applied to: {}",
                ctx.stepping_sources.join(", ")
            ));
        }

        // Report sources where stepping was detected but disabled
        for source in &ctx.stepping_detected_disabled {
            runner.log_message(&format!(
                "  \u{26a0} Stepping detected in {} but correction is disabled",
                source
            ));
        }

        // Report sources where stepping was detected but skipped (source separation)
        for source in &ctx.stepping_detected_separated {
            runner.log_message(&format!(
                "  \u{26a0} Stepping detected in {} but skipped (source separation active)",
                source
            ));
        }

        // Check stepping-adjusted subtitle tracks
        let stepping_sub_count = plan_items
            .iter()
            .filter(|item| {
                item.track.track_type == TrackType::Subtitles && item.stepping_adjusted
            })
            .count();

        if stepping_sub_count > 0 {
            runner.log_message(&format!(
                "  \u{2714} {} subtitle track(s) stepping-adjusted",
                stepping_sub_count
            ));
        }

        // Report quality issues from stepping correction
        for qi in &ctx.stepping_quality_issues {
            let icon = match qi.severity.as_str() {
                "error" => "\u{274c}",
                "warning" => "\u{26a0}",
                _ => "\u{2139}",
            };

            runner.log_message(&format!(
                "  {} Stepping quality ({}): {}",
                icon, qi.source, qi.message
            ));

            if qi.severity == "error" {
                issues += 1;
            }
        }

        // Verify segment flags metadata
        for (source_key, segment_entry) in &ctx.segment_flags {
            let mode = segment_entry
                .correction_mode
                .as_deref()
                .unwrap_or("unknown");

            let cluster_count = segment_entry.valid_clusters.len();
            let invalid_count = segment_entry.invalid_clusters.len();

            runner.log_message(&format!(
                "  \u{2139} Stepping metadata for {}: mode={}, clusters={}, invalid={}",
                source_key, mode, cluster_count, invalid_count
            ));

            // Check for audit metadata
            if let Some(audit_meta) = &segment_entry.audit_metadata {
                for entry in audit_meta {
                    let status = entry
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let message = entry
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    if status == "fail" {
                        runner.log_message(&format!(
                            "  \u{26a0} Stepping audit fail ({}): {}",
                            source_key, message
                        ));
                        issues += 1;
                    }
                }
            }
        }

        // Verify EDLs were created for stepping sources
        for source in &ctx.stepping_sources {
            if !ctx.stepping_edls.contains_key(source) {
                runner.log_message(&format!(
                    "  \u{26a0} No EDL found for stepping source: {}",
                    source
                ));
                issues += 1;
            }
        }

        if issues == 0 && !ctx.stepping_sources.is_empty() {
            runner.log_message("  \u{2714} Stepping corrections verified");
        }

        issues
    }
}
