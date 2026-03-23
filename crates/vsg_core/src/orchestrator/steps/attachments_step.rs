//! Attachments step — 1:1 port of `vsg_core/orchestrator/steps/attachments_step.py`.

use std::path::Path;

use crate::extraction::attachments::extract_attachments;
use crate::io::runner::CommandRunner;

use super::context::Context;

/// Extracts attachments from all sources — `AttachmentsStep`
pub struct AttachmentsStep;

impl AttachmentsStep {
    /// Run the attachments step.
    pub fn run(&self, ctx: &mut Context, runner: &CommandRunner) -> Result<(), String> {
        if !ctx.and_merge || ctx.attachment_sources.is_empty() {
            ctx.attachments = Some(Vec::new());
            self.add_replacement_fonts(ctx, runner);
            return Ok(());
        }

        let mut all_attachments: Vec<String> = Vec::new();

        for source_key in &ctx.attachment_sources.clone() {
            if let Some(source_file) = ctx.sources.get(source_key) {
                runner.log_message(&format!("Extracting attachments from {source_key}..."));
                let attachments_from_source = extract_attachments(
                    source_file,
                    &ctx.temp_dir,
                    runner,
                    &ctx.tool_paths,
                    source_key,
                );
                all_attachments.extend(attachments_from_source);
            }
        }

        ctx.attachments = Some(all_attachments);
        self.add_replacement_fonts(ctx, runner);

        Ok(())
    }

    /// Copy replacement fonts from Font Manager to temp directory — `_add_replacement_fonts`
    fn add_replacement_fonts(&self, ctx: &mut Context, runner: &CommandRunner) {
        let items = match &ctx.extracted_items {
            Some(items) => items,
            None => return,
        };

        // Collect all font replacement paths
        let mut replacement_files: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        for item in items {
            if let Some(ref replacements) = item.font_replacements {
                for repl_data in replacements.values() {
                    if let Some(ref font_file) = repl_data.font_file_path {
                        if !font_file.is_empty() {
                            replacement_files.insert(font_file.clone());
                        }
                    }
                }
            }
        }

        if replacement_files.is_empty() {
            return;
        }

        runner.log_message(&format!(
            "[Font] Copying {} replacement font(s)...",
            replacement_files.len()
        ));

        let fonts_temp_dir = ctx.temp_dir.join("replacement_fonts");
        let _ = std::fs::create_dir_all(&fonts_temp_dir);

        let mut copied_fonts: Vec<String> = Vec::new();
        for font_file in &replacement_files {
            let src_path = Path::new(font_file);
            if src_path.exists() {
                if let Some(file_name) = src_path.file_name() {
                    let dst_path = fonts_temp_dir.join(file_name);
                    match std::fs::copy(src_path, &dst_path) {
                        Ok(_) => {
                            copied_fonts.push(dst_path.to_string_lossy().to_string());
                            runner.log_message(&format!(
                                "[Font] Copied: {}",
                                file_name.to_string_lossy()
                            ));
                        }
                        Err(e) => {
                            runner.log_message(&format!(
                                "[Font] WARNING: Failed to copy {}: {e}",
                                file_name.to_string_lossy()
                            ));
                        }
                    }
                }
            } else {
                runner.log_message(&format!(
                    "[Font] WARNING: Font file not found: {font_file}"
                ));
            }
        }

        if !copied_fonts.is_empty() {
            let attachments = ctx.attachments.get_or_insert_with(Vec::new);
            let count = copied_fonts.len();
            attachments.extend(copied_fonts);
            runner.log_message(&format!(
                "[Font] Added {count} replacement font(s) to attachments."
            ));
        }
    }
}
