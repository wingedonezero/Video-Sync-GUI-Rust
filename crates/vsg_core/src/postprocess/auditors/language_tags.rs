//! Language tags auditor — 1:1 port of `vsg_core/postprocess/auditors/language_tags.py`.
//!
//! Verifies that language tags in the final MKV match the merge plan.

use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::orchestrator::steps::context::Context;

use super::base::Auditor;

/// Verifies language tags match the merge plan — `LanguageTagsAuditor`
pub struct LanguageTagsAuditor;

impl Auditor for LanguageTagsAuditor {
    fn run(
        &self,
        ctx: &Context,
        runner: &CommandRunner,
        _final_mkv_path: &Path,
        final_mkvmerge_data: &serde_json::Value,
        _final_ffprobe_data: Option<&serde_json::Value>,
    ) -> i32 {
        let mut issues = 0;

        let plan_items = match &ctx.extracted_items {
            Some(items) => items,
            None => return 0,
        };

        let tracks = match final_mkvmerge_data.get("tracks").and_then(|t| t.as_array()) {
            Some(t) => t,
            None => {
                runner.log_message("  \u{26a0} Could not read tracks from final MKV metadata");
                return 1;
            }
        };

        for (i, plan_item) in plan_items.iter().enumerate() {
            if i >= tracks.len() {
                break;
            }

            let track = &tracks[i];
            let props = track
                .get("properties")
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            let actual_lang = props
                .get("language")
                .and_then(|v| v.as_str())
                .unwrap_or("und");

            // Determine expected language
            let expected_lang = if !plan_item.custom_lang.is_empty() {
                plan_item.custom_lang.as_str()
            } else {
                plan_item.track.props.lang.as_str()
            };

            if actual_lang != expected_lang {
                // Also check language_ietf for BCP47 tags
                let actual_ietf = props
                    .get("language_ietf")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if actual_ietf != expected_lang {
                    runner.log_message(&format!(
                        "  \u{26a0} Track {}: language mismatch (expected='{}', actual='{}')",
                        i, expected_lang, actual_lang
                    ));
                    issues += 1;
                }
            }
        }

        if issues == 0 {
            runner.log_message("  \u{2714} Language tags verified");
        }

        issues
    }
}
