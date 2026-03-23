//! Attachments auditor — 1:1 port of `vsg_core/postprocess/auditors/attachments.py`.
//!
//! Verifies that font attachments were included in the final MKV.

use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::orchestrator::steps::context::Context;

use super::base::Auditor;

/// Verifies font attachments were included — `AttachmentsAuditor`
pub struct AttachmentsAuditor;

impl Auditor for AttachmentsAuditor {
    fn run(
        &self,
        ctx: &Context,
        runner: &CommandRunner,
        _final_mkv_path: &Path,
        final_mkvmerge_data: &serde_json::Value,
        _final_ffprobe_data: Option<&serde_json::Value>,
    ) -> i32 {
        let mut issues = 0;

        let planned_attachments = match &ctx.attachments {
            Some(a) if !a.is_empty() => a,
            _ => {
                // No attachments planned, nothing to verify
                return 0;
            }
        };

        let final_attachments = match final_mkvmerge_data
            .get("attachments")
            .and_then(|a| a.as_array())
        {
            Some(a) => a,
            None => {
                runner.log_message(&format!(
                    "  \u{26a0} Expected {} attachments but found none in final MKV",
                    planned_attachments.len()
                ));
                return planned_attachments.len() as i32;
            }
        };

        // Build set of actual attachment filenames
        let actual_names: Vec<String> = final_attachments
            .iter()
            .filter_map(|a| {
                a.get("file_name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_lowercase())
            })
            .collect();

        // Check each planned attachment exists
        for planned_path in planned_attachments {
            let filename = Path::new(planned_path)
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or(planned_path)
                .to_lowercase();

            if !actual_names.contains(&filename) {
                runner.log_message(&format!(
                    "  \u{26a0} Missing attachment: {}",
                    filename
                ));
                issues += 1;
            }
        }

        if issues == 0 {
            runner.log_message(&format!(
                "  \u{2714} All {} attachments verified",
                planned_attachments.len()
            ));
        }

        issues
    }
}
