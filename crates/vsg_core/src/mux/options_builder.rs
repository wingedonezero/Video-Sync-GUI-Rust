//! MKV merge options builder — 1:1 port of `vsg_core/mux/options_builder.py`.
//!
//! Builds the list of command-line arguments for mkvmerge.

use crate::models::enums::TrackType;
use crate::models::jobs::{MergePlan, PlanItem};
use crate::models::settings::AppSettings;

/// Builds mkvmerge command-line options — `MkvmergeOptionsBuilder`
pub struct MkvmergeOptionsBuilder;

impl MkvmergeOptionsBuilder {
    /// Build mkvmerge command tokens from a merge plan — `build()`
    pub fn build(plan: &MergePlan, settings: &AppSettings) -> Result<Vec<String>, String> {
        let mut tokens: Vec<String> = Vec::new();

        if let Some(ref chapters_xml) = plan.chapters_xml {
            tokens.push("--chapters".to_string());
            tokens.push(chapters_xml.to_string_lossy().to_string());
        }
        if settings.disable_track_statistics_tags {
            tokens.push("--disable-track-statistics-tags".to_string());
        }

        // Separate final tracks from preserved original tracks
        let mut final_items: Vec<&PlanItem> = plan
            .items
            .iter()
            .filter(|item| !item.is_preserved)
            .collect();
        let preserved_audio: Vec<&PlanItem> = plan
            .items
            .iter()
            .filter(|item| item.is_preserved && item.track.track_type == TrackType::Audio)
            .collect();
        let preserved_subs: Vec<&PlanItem> = plan
            .items
            .iter()
            .filter(|item| {
                item.is_preserved && item.track.track_type == TrackType::Subtitles
            })
            .collect();

        // Insert preserved audio tracks after the last main audio track
        if !preserved_audio.is_empty() {
            let last_audio_idx = final_items
                .iter()
                .rposition(|item| item.track.track_type == TrackType::Audio);
            match last_audio_idx {
                Some(idx) => {
                    let insert_at = idx + 1;
                    for (j, item) in preserved_audio.iter().enumerate() {
                        final_items.insert(insert_at + j, item);
                    }
                }
                None => final_items.extend(&preserved_audio),
            }
        }

        // Insert preserved subtitle tracks after the last main subtitle track
        if !preserved_subs.is_empty() {
            let last_sub_idx = final_items
                .iter()
                .rposition(|item| item.track.track_type == TrackType::Subtitles);
            match last_sub_idx {
                Some(idx) => {
                    let insert_at = idx + 1;
                    for (j, item) in preserved_subs.iter().enumerate() {
                        final_items.insert(insert_at + j, item);
                    }
                }
                None => final_items.extend(&preserved_subs),
            }
        }

        let default_audio_idx =
            first_index(&final_items, TrackType::Audio, |it| it.is_default);
        let default_sub_idx =
            first_index(&final_items, TrackType::Subtitles, |it| it.is_default);
        let first_video_idx =
            first_index(&final_items, TrackType::Video, |_| true);
        let forced_sub_idx =
            first_index(&final_items, TrackType::Subtitles, |it| {
                it.is_forced_display
            });

        let mut order_entries: Vec<String> = Vec::new();

        for (i, item) in final_items.iter().enumerate() {
            let tr = &item.track;
            let delay_ms = effective_delay_ms(plan, item);

            let is_default = i as i32 == first_video_idx
                || i as i32 == default_audio_idx
                || i as i32 == default_sub_idx;

            // Use custom language if set, otherwise use original from track
            let lang_code = if !item.custom_lang.is_empty() {
                &item.custom_lang
            } else if !tr.props.lang.is_empty() {
                &tr.props.lang
            } else {
                "und"
            };

            tokens.push("--language".to_string());
            tokens.push(format!("0:{lang_code}"));

            // Use custom name if set, otherwise fall back to apply_track_name behavior
            if !item.custom_name.is_empty() {
                tokens.push("--track-name".to_string());
                tokens.push(format!("0:{}", item.custom_name));
            } else if item.apply_track_name && !tr.props.name.trim().is_empty() {
                tokens.push("--track-name".to_string());
                tokens.push(format!("0:{}", tr.props.name));
            }

            tokens.push("--sync".to_string());
            tokens.push(format!("0:{delay_ms:+}"));
            tokens.push("--default-track-flag".to_string());
            tokens.push(format!("0:{}", if is_default { "yes" } else { "no" }));

            if i as i32 == forced_sub_idx && tr.track_type == TrackType::Subtitles {
                tokens.push("--forced-display-flag".to_string());
                tokens.push("0:yes".to_string());
            }

            if settings.disable_header_compression {
                tokens.push("--compression".to_string());
                tokens.push("0:none".to_string());
            }

            if settings.apply_dialog_norm_gain && tr.track_type == TrackType::Audio {
                let cid = tr.props.codec_id.to_uppercase();
                if cid.contains("AC3") || cid.contains("EAC3") {
                    tokens.push("--remove-dialog-normalization-gain".to_string());
                    tokens.push("0".to_string());
                }
            }

            // Preserve original aspect ratio for video tracks
            if tr.track_type == TrackType::Video {
                if let Some(ref aspect_ratio) = item.aspect_ratio {
                    tokens.push("--aspect-ratio".to_string());
                    tokens.push(format!("0:{aspect_ratio}"));
                }
            }

            let extracted_path = item
                .extracted_path
                .as_ref()
                .ok_or_else(|| {
                    format!(
                        "Plan item at index {i} ('{}') missing extracted_path",
                        tr.props.name
                    )
                })?;

            tokens.push("(".to_string());
            tokens.push(extracted_path.to_string_lossy().to_string());
            tokens.push(")".to_string());
            order_entries.push(format!("{i}:0"));
        }

        for att in &plan.attachments {
            tokens.push("--attach-file".to_string());
            tokens.push(att.to_string_lossy().to_string());
        }

        if !order_entries.is_empty() {
            tokens.push("--track-order".to_string());
            tokens.push(order_entries.join(","));
        }

        Ok(tokens)
    }
}

/// Find index of first matching item — `_first_index`
fn first_index(items: &[&PlanItem], kind: TrackType, predicate: impl Fn(&PlanItem) -> bool) -> i32 {
    for (i, it) in items.iter().enumerate() {
        if it.track.track_type == kind && predicate(it) {
            return i as i32;
        }
    }
    -1
}

/// Calculate the final sync delay for a track — `_effective_delay_ms`
///
/// CRITICAL: Video container delays from the source MKV should be IGNORED.
/// Video defines the timeline and should only get the global shift.
///
/// Source 1 VIDEO: Only global shift (video defines timeline)
/// Source 1 AUDIO: container_delay + global_shift (preserves internal sync)
/// Source 1 SUBTITLES: correlation delay (0 + global shift)
/// Other Sources: pre-calculated correlation delay (includes global shift)
/// External Subtitles: delay from sync_to source
fn effective_delay_ms(plan: &MergePlan, item: &PlanItem) -> i32 {
    let tr = &item.track;

    // Source 1 AUDIO: Preserve individual container delays + add global shift
    if tr.source == "Source 1" && tr.track_type == TrackType::Audio {
        // Use round() for proper rounding of negative values
        // int() truncates toward zero: int(-1001.825) = -1001 (wrong)
        // round() rounds to nearest: round(-1001.825) = -1002 (correct)
        let container_delay = (item.container_delay_ms as f64).round() as i32;
        let global_shift = plan.delays.global_shift_ms;
        return container_delay + global_shift;
    }

    // Source 1 VIDEO: ONLY apply global shift (IGNORE container delays)
    // Video defines the timeline - we don't preserve its container delays
    if tr.source == "Source 1" && tr.track_type == TrackType::Video {
        return plan.delays.global_shift_ms;
    }

    // SPECIAL CASE: Subtitles with stepping-adjusted timestamps
    // The delay is baked into the subtitle file. Don't double-apply.
    if tr.track_type == TrackType::Subtitles && item.stepping_adjusted {
        return 0;
    }

    // SPECIAL CASE: Subtitles with frame-perfect sync applied
    // The delay is baked into the subtitle file. Don't double-apply.
    if tr.track_type == TrackType::Subtitles && item.frame_adjusted {
        return 0;
    }

    let sync_key = if tr.source == "External" {
        item.sync_to.as_deref().unwrap_or("Source 1")
    } else {
        &tr.source
    };

    // SUBTITLE-SPECIFIC DELAYS: Check if this subtitle has a sync-mode-specific delay
    if tr.track_type == TrackType::Subtitles {
        if let Some(&delay) = plan.subtitle_delays_ms.get(sync_key) {
            return delay.round() as i32;
        }
    }

    // DEFAULT: Use correlation delay from analysis
    let delay = plan.delays.source_delays_ms.get(sync_key).copied().unwrap_or(0);
    delay
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::jobs::Delays;
    use crate::models::media::{StreamProps, Track};
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn make_track(source: &str, track_type: TrackType, codec: &str) -> Track {
        Track {
            source: source.to_string(),
            id: 0,
            track_type,
            props: StreamProps {
                codec_id: codec.to_string(),
                lang: "eng".to_string(),
                name: String::new(),
            },
        }
    }

    fn make_plan_item(track: Track, path: &str) -> PlanItem {
        PlanItem {
            track,
            extracted_path: Some(PathBuf::from(path)),
            is_default: false,
            is_forced_display: false,
            apply_track_name: false,
            convert_to_ass: false,
            rescale: false,
            size_multiplier: 1.0,
            style_patch: None,
            font_replacements: None,
            user_modified_path: None,
            sync_to: None,
            is_preserved: false,
            is_corrected: false,
            correction_source: None,
            perform_ocr: false,
            container_delay_ms: 0,
            custom_lang: String::new(),
            custom_name: String::new(),
            aspect_ratio: None,
            stepping_adjusted: false,
            frame_adjusted: false,
            is_generated: false,
            source_track_id: None,
            filter_config: None,
            original_style_list: Vec::new(),
            sync_exclusion_styles: Vec::new(),
            sync_exclusion_mode: "exclude".to_string(),
            sync_exclusion_original_style_list: Vec::new(),
            framelocked_stats: None,
            clamping_info: None,
            video_verified_bitmap: false,
            video_verified_details: None,
        }
    }

    #[test]
    fn effective_delay_source1_video_gets_global_shift_only() {
        let plan = MergePlan {
            items: Vec::new(),
            delays: Delays {
                source_delays_ms: HashMap::new(),
                raw_source_delays_ms: HashMap::new(),
                global_shift_ms: 100,
                raw_global_shift_ms: 100.0,
            },
            chapters_xml: None,
            attachments: Vec::new(),
            subtitle_delays_ms: HashMap::new(),
        };
        let item = make_plan_item(
            make_track("Source 1", TrackType::Video, "V_MPEG4/ISO/AVC"),
            "/tmp/video.h264",
        );
        assert_eq!(effective_delay_ms(&plan, &item), 100);
    }

    #[test]
    fn effective_delay_source1_audio_gets_container_plus_global() {
        let plan = MergePlan {
            items: Vec::new(),
            delays: Delays {
                source_delays_ms: HashMap::new(),
                raw_source_delays_ms: HashMap::new(),
                global_shift_ms: 50,
                raw_global_shift_ms: 50.0,
            },
            chapters_xml: None,
            attachments: Vec::new(),
            subtitle_delays_ms: HashMap::new(),
        };
        let mut item = make_plan_item(
            make_track("Source 1", TrackType::Audio, "A_AAC"),
            "/tmp/audio.aac",
        );
        item.container_delay_ms = -10;
        assert_eq!(effective_delay_ms(&plan, &item), -10 + 50);
    }

    #[test]
    fn effective_delay_stepping_adjusted_is_zero() {
        let plan = MergePlan {
            items: Vec::new(),
            delays: Delays {
                source_delays_ms: {
                    let mut m = HashMap::new();
                    m.insert("Source 2".to_string(), -500);
                    m
                },
                raw_source_delays_ms: HashMap::new(),
                global_shift_ms: 500,
                raw_global_shift_ms: 500.0,
            },
            chapters_xml: None,
            attachments: Vec::new(),
            subtitle_delays_ms: HashMap::new(),
        };
        let mut item = make_plan_item(
            make_track("Source 2", TrackType::Subtitles, "S_TEXT/ASS"),
            "/tmp/subs.ass",
        );
        item.stepping_adjusted = true;
        assert_eq!(effective_delay_ms(&plan, &item), 0);
    }

    #[test]
    fn effective_delay_other_source_uses_correlation() {
        let mut source_delays = HashMap::new();
        source_delays.insert("Source 2".to_string(), -300);
        let plan = MergePlan {
            items: Vec::new(),
            delays: Delays {
                source_delays_ms: source_delays,
                raw_source_delays_ms: HashMap::new(),
                global_shift_ms: 300,
                raw_global_shift_ms: 300.0,
            },
            chapters_xml: None,
            attachments: Vec::new(),
            subtitle_delays_ms: HashMap::new(),
        };
        let item = make_plan_item(
            make_track("Source 2", TrackType::Audio, "A_FLAC"),
            "/tmp/audio.flac",
        );
        assert_eq!(effective_delay_ms(&plan, &item), -300);
    }
}
