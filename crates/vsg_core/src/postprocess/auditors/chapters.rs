//! Chapters auditor — 1:1 port of `vsg_core/postprocess/auditors/chapters.py`.
//!
//! Verifies that chapters were preserved/processed correctly in the final MKV.

use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::orchestrator::steps::context::Context;

use super::base::Auditor;

/// Verifies chapters were preserved/processed correctly — `ChaptersAuditor`
pub struct ChaptersAuditor;

impl Auditor for ChaptersAuditor {
    fn run(
        &self,
        ctx: &Context,
        runner: &CommandRunner,
        _final_mkv_path: &Path,
        final_mkvmerge_data: &serde_json::Value,
        _final_ffprobe_data: Option<&serde_json::Value>,
    ) -> i32 {
        let mut issues = 0;

        // If no chapters XML was generated, nothing to verify
        let chapters_xml = match &ctx.chapters_xml {
            Some(xml) if !xml.is_empty() => xml,
            _ => return 0,
        };

        // Count expected chapters from XML (simple tag counting)
        let expected_count = chapters_xml.matches("<ChapterAtom>").count();

        // Get actual chapters from mkvmerge data
        let actual_chapters = final_mkvmerge_data
            .get("chapters")
            .and_then(|c| c.as_array());

        let actual_count = match actual_chapters {
            Some(chapters) => {
                // mkvmerge -J nests chapters under edition entries
                let mut count = 0;
                for edition in chapters {
                    if let Some(atoms) = edition
                        .get("num_entries")
                        .and_then(|v| v.as_i64())
                    {
                        count += atoms as usize;
                    } else if let Some(sub_chapters) =
                        edition.get("sub_chapters").and_then(|c| c.as_array())
                    {
                        count += sub_chapters.len();
                    }
                }
                count
            }
            None => 0,
        };

        if expected_count > 0 && actual_count == 0 {
            runner.log_message(&format!(
                "  \u{26a0} Expected {} chapters but none found in final MKV",
                expected_count
            ));
            issues += 1;
        } else if expected_count != actual_count && expected_count > 0 {
            runner.log_message(&format!(
                "  \u{26a0} Chapter count mismatch (expected={}, actual={})",
                expected_count, actual_count
            ));
            issues += 1;
        } else if actual_count > 0 {
            runner.log_message(&format!(
                "  \u{2714} Chapters verified ({} chapters)",
                actual_count
            ));
        }

        // Verify chapter timestamps are non-negative
        if let Some(chapters) = actual_chapters {
            for edition in chapters {
                if let Some(sub_chapters) =
                    edition.get("sub_chapters").and_then(|c| c.as_array())
                {
                    for chapter in sub_chapters {
                        let timestamp = chapter
                            .get("timestamp")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);

                        if timestamp < 0 {
                            runner.log_message(&format!(
                                "  \u{26a0} Negative chapter timestamp detected: {}ns",
                                timestamp
                            ));
                            issues += 1;
                        }
                    }
                }
            }
        }

        issues
    }
}
