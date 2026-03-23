//! Pipeline validation — 1:1 port of `vsg_core/orchestrator/validation.py`.

use std::path::Path;

use crate::models::enums::TrackType;

use super::steps::context::Context;

/// Raised when a pipeline step validation fails — `PipelineValidationError`
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct PipelineValidationError(pub String);

/// Validates that pipeline steps completed successfully — `StepValidator`
pub struct StepValidator;

impl StepValidator {
    /// Validates analysis step results — `validate_analysis`
    pub fn validate_analysis(ctx: &Context) -> Result<(), PipelineValidationError> {
        let delays = ctx.delays.as_ref().ok_or_else(|| {
            PipelineValidationError(
                "Analysis failed: No delays calculated. \
                 Check that audio correlation or VideoDiff completed successfully."
                    .to_string(),
            )
        })?;

        if ctx.and_merge && ctx.sources.len() > 1 {
            for source_key in ctx.sources.keys() {
                if source_key == "Source 1" {
                    continue;
                }
                if !delays.source_delays_ms.contains_key(source_key) {
                    return Err(PipelineValidationError(format!(
                        "Analysis incomplete: No delay calculated for {source_key}. \
                         Audio correlation may have failed."
                    )));
                }
            }
        }

        for (source_key, delay) in &delays.source_delays_ms {
            if delay.abs() > 3_600_000 {
                return Err(PipelineValidationError(format!(
                    "Unreasonable delay for {source_key}: {delay}ms. \
                     This likely indicates an analysis error."
                )));
            }
        }

        Ok(())
    }

    /// Validates extraction step results — `validate_extraction`
    pub fn validate_extraction(ctx: &Context) -> Result<(), PipelineValidationError> {
        let items = ctx.extracted_items.as_ref().ok_or_else(|| {
            PipelineValidationError(
                "Extraction failed: No tracks extracted. \
                 Check that mkvextract completed successfully."
                    .to_string(),
            )
        })?;

        let expected = ctx.manual_layout.iter().filter(|item| item.source.is_some()).count();
        let actual = items.iter().filter(|item| !item.is_preserved).count();

        if actual < expected {
            return Err(PipelineValidationError(format!(
                "Extraction incomplete: Expected {expected} tracks, got {actual}. \
                 Some tracks may have failed to extract."
            )));
        }

        for item in items {
            if let Some(ref path) = item.extracted_path {
                if !path.exists() {
                    return Err(PipelineValidationError(format!(
                        "Extraction failed: Track file missing at {}",
                        path.display()
                    )));
                }
            }
        }

        Ok(())
    }

    /// Validates audio correction results — `validate_correction`
    pub fn validate_correction(ctx: &Context) -> Result<(), PipelineValidationError> {
        let items = match &ctx.extracted_items {
            Some(items) => items,
            None => return Ok(()),
        };
        let mut errors: Vec<String> = Vec::new();

        // Check PAL drift corrections
        for analysis_key in ctx.pal_drift_flags.keys() {
            let source_key = analysis_key.split('_').next().unwrap_or("");
            Self::check_corrected_tracks(items, source_key, "PAL drift", &mut errors);
        }

        // Check linear drift corrections
        for analysis_key in ctx.linear_drift_flags.keys() {
            let source_key = analysis_key.split('_').next().unwrap_or("");
            Self::check_corrected_tracks(items, source_key, "Linear drift", &mut errors);
        }

        // Check stepping corrections
        for (analysis_key, flag_info) in &ctx.segment_flags {
            let source_key = analysis_key.split('_').next().unwrap_or("");
            let subs_only = flag_info.subs_only.unwrap_or(false);

            if subs_only {
                if !ctx.stepping_edls.contains_key(source_key) {
                    errors.push(format!(
                        "Stepping correction (subs-only) failed for {source_key}: \
                         No EDL stored for subtitle adjustment"
                    ));
                }
                continue;
            }

            Self::check_corrected_tracks(items, source_key, "Stepping", &mut errors);
        }

        if !errors.is_empty() {
            return Err(PipelineValidationError(format!(
                "Audio correction validation failed:\n{}",
                errors.iter().map(|e| format!("  - {e}")).collect::<Vec<_>>().join("\n")
            )));
        }

        Ok(())
    }

    /// Helper: Check that corrected tracks exist for a source.
    fn check_corrected_tracks(
        items: &[crate::models::jobs::PlanItem],
        source_key: &str,
        correction_type: &str,
        errors: &mut Vec<String>,
    ) {
        let corrected: Vec<_> = items
            .iter()
            .filter(|item| {
                item.track.source == source_key
                    && item.track.track_type == TrackType::Audio
                    && item.is_corrected
                    && !item.is_preserved
            })
            .collect();

        if corrected.is_empty() {
            // Not necessarily an error — corrector may determine no correction needed
            return;
        }

        for item in corrected {
            if item.track.props.codec_id != "FLAC" {
                errors.push(format!(
                    "{correction_type} corrected track for {source_key} is not FLAC: {}",
                    item.track.props.codec_id
                ));
            }
            if let Some(ref path) = item.extracted_path {
                if !path.exists() {
                    errors.push(format!(
                        "{correction_type} corrected file missing for {source_key}"
                    ));
                }
            }
        }
    }

    /// Validates subtitle processing results — `validate_subtitles`
    pub fn validate_subtitles(ctx: &Context) -> Result<(), PipelineValidationError> {
        let items = match &ctx.extracted_items {
            Some(items) => items,
            None => return Ok(()),
        };
        let mut errors: Vec<String> = Vec::new();

        for item in items {
            if item.track.track_type != TrackType::Subtitles {
                continue;
            }
            if item.is_preserved {
                continue;
            }

            if item.perform_ocr {
                if let Some(ref path) = item.extracted_path {
                    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                    if !matches!(ext.as_str(), "srt" | "ass" | "ssa") {
                        errors.push(format!(
                            "OCR track '{}' has wrong extension: .{ext}",
                            item.track.props.name
                        ));
                    }
                }
            }

            if item.convert_to_ass {
                if let Some(ref path) = item.extracted_path {
                    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                    if !matches!(ext.as_str(), "ass" | "ssa") {
                        errors.push(format!(
                            "ASS conversion failed for '{}': file is .{ext}",
                            item.track.props.name
                        ));
                    }
                }
            }

            if let Some(ref path) = item.extracted_path {
                if !path.exists() {
                    errors.push(format!("Subtitle file missing: {}", path.display()));
                }
            }
        }

        if !errors.is_empty() {
            return Err(PipelineValidationError(format!(
                "Subtitle processing validation failed:\n{}",
                errors.iter().map(|e| format!("  - {e}")).collect::<Vec<_>>().join("\n")
            )));
        }

        Ok(())
    }

    /// Validates merge planning results — `validate_mux`
    pub fn validate_mux(ctx: &Context) -> Result<(), PipelineValidationError> {
        let tokens = ctx.tokens.as_ref().ok_or_else(|| {
            PipelineValidationError(
                "Merge planning failed: No mkvmerge command tokens generated".to_string(),
            )
        })?;

        let mut errors: Vec<String> = Vec::new();
        let path_flags = ["--chapters", "--attach-file"];
        let mut in_parens = false;

        for (i, token) in tokens.iter().enumerate() {
            if token == "(" {
                in_parens = true;
                continue;
            }
            if token == ")" {
                in_parens = false;
                continue;
            }

            let prev_token = if i > 0 { &tokens[i - 1] } else { "" };
            let prev_str: &str = prev_token;
            let is_path_argument = path_flags.contains(&prev_str);
            let is_input_file = in_parens;

            if (is_path_argument || is_input_file) && !Path::new(token).exists() {
                errors.push(format!("Input file missing from mux command: {token}"));
            }
        }

        if !errors.is_empty() {
            return Err(PipelineValidationError(format!(
                "Merge planning validation failed:\n{}",
                errors.iter().map(|e| format!("  - {e}")).collect::<Vec<_>>().join("\n")
            )));
        }

        Ok(())
    }
}
