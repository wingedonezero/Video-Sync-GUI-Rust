//! Model converters — 1:1 port of `vsg_core/models/converters.py`.

use std::collections::HashMap;

use super::context_types::ManualLayoutItem;
use super::enums::TrackType;
use super::jobs::PlanItem;
use super::media::{StreamProps, Track};

/// Convert a string to a TrackType — `_type_from_str`
fn type_from_str(s: &str) -> TrackType {
    match s.to_lowercase().as_str() {
        "video" => TrackType::Video,
        "audio" => TrackType::Audio,
        "subtitles" => TrackType::Subtitles,
        _ => TrackType::Video, // fallback
    }
}

/// Convert raw track info dicts into typed Track objects — `tracks_from_dialog_info`
pub fn tracks_from_dialog_info(
    track_info: &HashMap<String, Vec<serde_json::Value>>,
) -> HashMap<String, Vec<Track>> {
    let mut out: HashMap<String, Vec<Track>> = HashMap::new();

    for (source_key, items) in track_info {
        let mut tracks = Vec::new();
        for t in items {
            let id = t.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let track_type_str = t
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("video");
            let codec_id = t
                .get("codec_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let lang = t
                .get("lang")
                .and_then(|v| v.as_str())
                .unwrap_or("und")
                .to_string();
            let name = t
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            tracks.push(Track {
                source: source_key.clone(),
                id,
                track_type: type_from_str(track_type_str),
                props: StreamProps {
                    codec_id,
                    lang,
                    name,
                },
            });
        }
        out.insert(source_key.clone(), tracks);
    }

    out
}

/// Bind manual layout selections to typed Track models — `realize_plan_from_manual_layout`
pub fn realize_plan_from_manual_layout(
    manual_layout: &[ManualLayoutItem],
    track_info_by_source: &HashMap<String, Vec<Track>>,
) -> Vec<PlanItem> {
    // Build lookup map: (source_key, track_id) -> &Track
    let mut idx: HashMap<(String, i32), &Track> = HashMap::new();
    for tracks in track_info_by_source.values() {
        for tr in tracks {
            idx.insert((tr.source.clone(), tr.id), tr);
        }
    }

    let mut realized = Vec::new();
    for sel in manual_layout {
        let source_key = match &sel.source {
            Some(s) => s.clone(),
            None => continue,
        };
        let tid = match sel.id {
            Some(id) => id,
            None => continue,
        };

        let track_model = match idx.get(&(source_key, tid)) {
            Some(t) => (*t).clone(),
            None => continue,
        };

        realized.push(PlanItem {
            track: track_model,
            extracted_path: None,
            is_default: sel.is_default.unwrap_or(false),
            is_forced_display: sel.is_forced_display.unwrap_or(false),
            apply_track_name: sel.apply_track_name.unwrap_or(false),
            convert_to_ass: sel.convert_to_ass.unwrap_or(false),
            rescale: sel.rescale.unwrap_or(false),
            size_multiplier: sel.size_multiplier.unwrap_or(1.0),
            style_patch: None,
            font_replacements: None,
            user_modified_path: None,
            sync_to: None,
            is_preserved: false,
            is_corrected: false,
            correction_source: None,
            perform_ocr: sel.perform_ocr.unwrap_or(false),
            container_delay_ms: 0,
            custom_lang: sel.custom_lang.clone().unwrap_or_default(),
            custom_name: sel.custom_name.clone().unwrap_or_default(),
            aspect_ratio: None,
            stepping_adjusted: false,
            frame_adjusted: false,
            is_generated: sel.is_generated.unwrap_or(false),
            source_track_id: sel.source_track_id,
            filter_config: sel.filter_config.clone(),
            original_style_list: sel.original_style_list.clone().unwrap_or_default(),
            sync_exclusion_styles: Vec::new(),
            sync_exclusion_mode: "exclude".to_string(),
            sync_exclusion_original_style_list: Vec::new(),
            framelocked_stats: None,
            clamping_info: None,
            video_verified_bitmap: false,
            video_verified_details: None,
        });
    }

    realized
}

/// Generate a track signature for auto-applying layouts — `signature_for_auto_apply`
pub fn signature_for_auto_apply(
    track_info: &HashMap<String, Vec<Track>>,
    strict: bool,
) -> HashMap<String, usize> {
    let mut counter: HashMap<String, usize> = HashMap::new();

    for tracks in track_info.values() {
        for tr in tracks {
            let key = if strict {
                format!(
                    "{}_{}_{}_{}",
                    tr.source,
                    tr.track_type,
                    tr.props.lang.to_lowercase(),
                    tr.props.codec_id.to_lowercase(),
                )
            } else {
                format!("{}_{}", tr.source, tr.track_type)
            };
            *counter.entry(key).or_insert(0) += 1;
        }
    }

    counter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_from_str_works() {
        assert_eq!(type_from_str("audio"), TrackType::Audio);
        assert_eq!(type_from_str("SUBTITLES"), TrackType::Subtitles);
        assert_eq!(type_from_str("Video"), TrackType::Video);
    }

    #[test]
    fn signature_non_strict() {
        let mut info = HashMap::new();
        info.insert(
            "Source 1".to_string(),
            vec![
                Track {
                    source: "Source 1".to_string(),
                    id: 0,
                    track_type: TrackType::Video,
                    props: StreamProps::new("V_MPEG4"),
                },
                Track {
                    source: "Source 1".to_string(),
                    id: 1,
                    track_type: TrackType::Audio,
                    props: StreamProps::new("A_AAC"),
                },
            ],
        );
        let sig = signature_for_auto_apply(&info, false);
        assert_eq!(sig["Source 1_video"], 1);
        assert_eq!(sig["Source 1_audio"], 1);
    }
}
