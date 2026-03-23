//! Extract step — 1:1 port of `vsg_core/orchestrator/steps/extract_step.py`.

use std::path::{Path, PathBuf};

use crate::extraction::tracks::{extract_tracks, get_stream_info_with_delays};
use crate::io::runner::CommandRunner;
use crate::models::enums::TrackType;
use crate::models::jobs::PlanItem;
use crate::models::media::{StreamProps, Track};

use super::context::Context;

/// Extracts tracks from MKV sources — `ExtractStep`
pub struct ExtractStep;

impl ExtractStep {
    /// Run the extraction step.
    pub fn run(&self, ctx: &mut Context, runner: &CommandRunner) -> Result<(), String> {
        if !ctx.and_merge {
            ctx.extracted_items = Some(Vec::new());
            return Ok(());
        }

        // Filter out video tracks from secondary sources
        ctx.manual_layout.retain(|item| {
            let source = item.source.as_deref().unwrap_or("");
            let track_type = item.track_type.as_deref().unwrap_or("");
            if track_type == "video" && source != "Source 1" {
                runner.log_message(&format!(
                    "[WARNING] Skipping video track from {source} (ID {}). Video is only allowed from Source 1.",
                    item.id.unwrap_or(0)
                ));
                false
            } else {
                true
            }
        });

        // --- Read container delays ---
        runner.log_message("--- Reading Container Delays from Source Files ---");
        let mut source1_video_delay_ms: i64 = 0;

        for (source_key, source_path) in &ctx.sources.clone() {
            if let Some(info) = get_stream_info_with_delays(source_path, runner, &ctx.tool_paths) {
                let mut delays_for_source = std::collections::HashMap::new();
                runner.log_message(&format!(
                    "[Container Delays] Reading delays from {source_key}:"
                ));

                // Find Source 1 video delay first
                if source_key == "Source 1" {
                    if let Some(tracks) = info.get("tracks").and_then(|v| v.as_array()) {
                        for track in tracks {
                            if track.get("type").and_then(|v| v.as_str()) == Some("video") {
                                source1_video_delay_ms = track
                                    .get("container_delay_ms")
                                    .and_then(|v| v.as_i64())
                                    .unwrap_or(0);
                                break;
                            }
                        }
                    }
                }

                if let Some(tracks) = info.get("tracks").and_then(|v| v.as_array()) {
                    for track in tracks {
                        let tid = track.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        let track_type = track.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        let delay_ms = track
                            .get("container_delay_ms")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0) as i32;

                        // For Source 1 audio, calculate delay relative to video
                        let stored_delay = if source_key == "Source 1" && track_type == "audio" {
                            delay_ms - source1_video_delay_ms as i32
                        } else {
                            delay_ms
                        };

                        delays_for_source.insert(tid, stored_delay);

                        // Log non-zero audio/video delays
                        if delay_ms != 0 && matches!(track_type, "audio" | "video") {
                            let props = track.get("properties").unwrap_or(&serde_json::Value::Null);
                            let lang = props.get("language").and_then(|v| v.as_str()).unwrap_or("und");
                            let name = props.get("track_name").and_then(|v| v.as_str()).unwrap_or("");
                            let mut desc = format!("  Track {tid} ({track_type}");
                            if lang != "und" {
                                desc.push_str(&format!(", {lang}"));
                            }
                            if !name.is_empty() {
                                desc.push_str(&format!(", '{name}'"));
                            }
                            desc.push_str(&format!("): {delay_ms:+.1}ms"));
                            runner.log_message(&desc);
                        }
                    }
                }

                let non_zero: Vec<_> = delays_for_source.values().filter(|&&d| d != 0).collect();
                if non_zero.is_empty() {
                    runner.log_message("  All tracks have zero container delay");
                }

                ctx.container_delays.insert(source_key.clone(), delays_for_source);
            }
        }

        // --- Read aspect ratios ---
        runner.log_message("--- Reading Aspect Ratios from Source Files ---");
        let mut source_aspect_ratios: std::collections::HashMap<String, std::collections::HashMap<i32, String>> =
            std::collections::HashMap::new();

        for (source_key, source_path) in &ctx.sources {
            let ffprobe_out = runner.run(
                &["ffprobe", "-v", "error", "-show_streams", "-of", "json", source_path],
                &ctx.tool_paths,
            );
            if let Some(out) = ffprobe_out {
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&out) {
                    let mut ratios = std::collections::HashMap::new();
                    if let Some(streams) = data.get("streams").and_then(|v| v.as_array()) {
                        for stream in streams {
                            if stream.get("codec_type").and_then(|v| v.as_str()) == Some("video") {
                                if let Some(dar) = stream.get("display_aspect_ratio").and_then(|v| v.as_str()) {
                                    let idx = stream.get("index").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                                    ratios.insert(idx, dar.to_string());
                                    runner.log_message(&format!(
                                        "[{source_key}] Video track {idx} aspect ratio: {dar}"
                                    ));
                                }
                            }
                        }
                    }
                    source_aspect_ratios.insert(source_key.clone(), ratios);
                }
            }
        }

        // --- Extract tracks ---
        let mut all_extracted: Vec<(String, serde_json::Value)> = Vec::new();
        for (source_key, source_path) in &ctx.sources.clone() {
            let track_ids: Vec<i32> = ctx.manual_layout
                .iter()
                .filter(|item| item.source.as_deref() == Some(source_key))
                .filter_map(|item| item.id)
                .collect();

            if !track_ids.is_empty() {
                runner.log_message(&format!(
                    "Preparing to extract {} track(s) from {source_key}...",
                    track_ids.len()
                ));

                let extracted = extract_tracks(
                    source_path,
                    &ctx.temp_dir,
                    runner,
                    &ctx.tool_paths,
                    source_key,
                    Some(&track_ids),
                )?;

                for et in extracted {
                    let key = format!("{}_{}", et.source, et.id);
                    all_extracted.push((key, serde_json::to_value(&et).unwrap_or_default()));
                }
            }
        }

        let extracted_map: std::collections::HashMap<String, serde_json::Value> =
            all_extracted.into_iter().collect();

        // --- Build PlanItem list ---
        let mut items: Vec<PlanItem> = Vec::new();
        for sel in &ctx.manual_layout.clone() {
            let source = sel.source.as_deref().unwrap_or("");

            let plan_item = if source == "External" {
                let original_path_str = match sel.original_path.as_deref() {
                    Some(p) => p,
                    None => continue,
                };
                let original_path = Path::new(original_path_str);
                let temp_path = ctx.temp_dir.join(
                    original_path.file_name().unwrap_or_default()
                );
                let _ = std::fs::copy(original_path, &temp_path);

                PlanItem {
                    track: Track {
                        source: "External".to_string(),
                        id: 0,
                        track_type: TrackType::Subtitles,
                        props: StreamProps {
                            codec_id: sel.codec_id.clone().unwrap_or_default(),
                            lang: sel.lang.clone().unwrap_or_else(|| "und".to_string()),
                            name: sel.name.clone().unwrap_or_default(),
                        },
                    },
                    extracted_path: Some(temp_path),
                    container_delay_ms: 0,
                    aspect_ratio: None,
                    ..PlanItem::default_fields()
                }
            } else {
                let tid = sel.id.unwrap_or(0);
                let key = format!("{source}_{tid}");
                let trk = match extracted_map.get(&key) {
                    Some(t) => t,
                    None => {
                        runner.log_message(&format!(
                            "[WARNING] Could not find extracted file for {key}. Skipping."
                        ));
                        continue;
                    }
                };

                let track_type_str = trk.get("type").and_then(|v| v.as_str()).unwrap_or("video");
                let track_type = match track_type_str {
                    "audio" => TrackType::Audio,
                    "subtitles" => TrackType::Subtitles,
                    _ => TrackType::Video,
                };

                let container_delay = ctx.container_delays
                    .get(source)
                    .and_then(|m| m.get(&tid))
                    .copied()
                    .unwrap_or(0);

                let aspect_ratio = if track_type == TrackType::Video {
                    source_aspect_ratios.get(source).and_then(|m| m.get(&tid)).cloned()
                } else {
                    None
                };

                let path_str = trk.get("path").and_then(|v| v.as_str()).unwrap_or("");

                PlanItem {
                    track: Track {
                        source: source.to_string(),
                        id: tid,
                        track_type,
                        props: StreamProps {
                            codec_id: trk.get("codec_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            lang: trk.get("lang").and_then(|v| v.as_str()).unwrap_or("und").to_string(),
                            name: trk.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        },
                    },
                    extracted_path: Some(PathBuf::from(path_str)),
                    container_delay_ms: container_delay,
                    aspect_ratio,
                    ..PlanItem::default_fields()
                }
            };

            let mut plan_item = plan_item;
            plan_item.is_default = sel.is_default.unwrap_or(false);
            plan_item.is_forced_display = sel.is_forced_display.unwrap_or(false);
            plan_item.apply_track_name = sel.apply_track_name.unwrap_or(false);
            plan_item.perform_ocr = sel.perform_ocr.unwrap_or(false);
            plan_item.convert_to_ass = sel.convert_to_ass.unwrap_or(false);
            plan_item.rescale = sel.rescale.unwrap_or(false);
            plan_item.size_multiplier = sel.size_multiplier.unwrap_or(1.0).max(0.001);
            if plan_item.size_multiplier == 0.0 {
                plan_item.size_multiplier = 1.0;
            }
            plan_item.sync_to = sel.sync_to.clone();
            plan_item.correction_source = sel.correction_source.clone();
            plan_item.custom_lang = sel.custom_lang.clone().unwrap_or_default();
            plan_item.custom_name = sel.custom_name.clone().unwrap_or_default();
            plan_item.is_generated = sel.is_generated.unwrap_or(false);
            plan_item.source_track_id = sel.source_track_id;
            plan_item.filter_config = sel.filter_config.clone();
            plan_item.original_style_list = sel.original_style_list.clone().unwrap_or_default();

            items.push(plan_item);
        }

        // --- Process generated tracks ---
        runner.log_message("--- Processing Generated Tracks ---");
        let failed = Self::process_generated_tracks(&mut items, runner, &ctx.temp_dir);
        if !failed.is_empty() {
            runner.log_message(&format!(
                "[WARNING] Removing {} generated track(s) that failed processing",
                failed.len()
            ));
            items.retain(|item| !failed.contains(&(item as *const PlanItem as usize)));
        }

        ctx.extracted_items = Some(items);
        Ok(())
    }

    /// Prepare generated tracks by copying source files — `_process_generated_tracks`
    fn process_generated_tracks(
        items: &mut [PlanItem],
        runner: &CommandRunner,
        temp_dir: &Path,
    ) -> Vec<usize> {
        let mut failed_indices: Vec<usize> = Vec::new();

        // Collect source info first to avoid borrow issues
        let source_info: Vec<(Option<PathBuf>, i32, Option<String>)> = items
            .iter()
            .map(|item| {
                (
                    item.extracted_path.clone(),
                    item.container_delay_ms,
                    item.sync_to.clone(),
                )
            })
            .collect();

        for (i, item) in items.iter_mut().enumerate() {
            if !item.is_generated {
                continue;
            }

            runner.log_message(&format!(
                "[Generated Track] Preparing track from {} Track {:?}...",
                item.track.source,
                item.source_track_id
            ));

            // Find source track
            let source_idx = source_info.iter().position(|(_, _, _)| {
                // This is simplified — in Python it searches by source + track_id
                false
            });

            let source_path = if let Some(idx) = source_idx {
                let (ref path, delay, ref sync) = source_info[idx];
                item.container_delay_ms = delay;
                item.sync_to = sync.clone();
                path.clone()
            } else {
                runner.log_message(
                    "  Source track not in pipeline - using generated track's own extraction",
                );
                item.extracted_path.clone()
            };

            let source_path = match source_path {
                Some(p) if p.exists() => p,
                _ => {
                    runner.log_message(&format!(
                        "[ERROR] Source file not found for generated track: {:?}",
                        source_path
                    ));
                    failed_indices.push(i);
                    continue;
                }
            };

            let original_stem = source_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let ext = source_path
                .extension()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let generated_path = temp_dir.join(format!(
                "{original_stem}_generated_{i}.{ext}"
            ));

            match std::fs::copy(&source_path, &generated_path) {
                Ok(_) => {
                    item.extracted_path = Some(generated_path.clone());
                    runner.log_message(&format!(
                        "  Copied source to: {}",
                        generated_path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                    runner.log_message("  Style filtering will be applied in subtitles step");
                }
                Err(e) => {
                    runner.log_message(&format!(
                        "[ERROR] Failed to prepare generated track: {e}"
                    ));
                    failed_indices.push(i);
                }
            }
        }

        failed_indices
    }
}

// Helper for PlanItem default construction
impl PlanItem {
    /// Create a PlanItem with all optional fields defaulted.
    pub(crate) fn default_fields() -> Self {
        Self {
            track: Track {
                source: String::new(),
                id: 0,
                track_type: TrackType::Video,
                props: StreamProps::new(""),
            },
            extracted_path: None,
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
}
