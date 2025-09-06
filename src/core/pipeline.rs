// src/core/pipeline.rs

use serde_json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::sync::mpsc;

use crate::core::config::AppConfig;
use crate::core::process::CommandRunner;
use crate::core::{analysis, mkv_utils, subtitle_utils};

#[derive(Clone, Debug)]
pub struct Job {
    pub ref_file: String,
    pub sec_file: Option<String>,
    pub ter_file: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TrackSelection {
    pub source: String, // "REF", "SEC", or "TER"
    pub original_track: mkv_utils::Track,
    pub extracted_path: Option<PathBuf>, // Path is set after extraction
    pub is_default: bool,
    pub is_forced: bool,
    pub apply_track_name: bool,
    pub convert_to_ass: bool,
    pub rescale: bool,
    pub size_multiplier: f64,
}

pub struct JobPipeline {
    config: AppConfig,
    log_sender: mpsc::Sender<String>,
}

impl JobPipeline {
    pub fn new(config: AppConfig, log_sender: mpsc::Sender<String>) -> Self {
        Self { config, log_sender }
    }

    pub async fn run_job(
        &self,
        job: &Job,
        and_merge: bool,
        layout: &[TrackSelection],
    ) -> Result<String, String> {
        let runner = CommandRunner::new(self.config.clone(), self.log_sender.clone());
        let temp_dir_name = format!(
            "job_{}_{}",
            Path::new(&job.ref_file)
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap(),
                                    chrono::Utc::now().timestamp()
        );
        let temp_dir = PathBuf::from(&self.config.temp_root).join(temp_dir_name);
        fs::create_dir_all(&temp_dir)
        .await
        .map_err(|e| e.to_string())?;

        // --- Analysis Phase ---
        runner.send_log("--- Analysis Phase ---").await;
        let analysis_mode = self.config.analysis_mode.as_str();

        let analyze_target = |target_file: &str, ref_lang_conf: &str, target_lang_conf: &str| async {
            match analysis_mode {
                "VideoDiff" => analysis::run_videodiff(&runner, &job.ref_file, target_file, &self.config)
                .await
                .map(|(d, _)| d),
                _ => {
                    // Audio Correlation
                    let ref_lang = if ref_lang_conf.is_empty() { None } else { Some(ref_lang_conf) };
                    let target_lang = if target_lang_conf.is_empty() { None } else { Some(target_lang_conf) };
                    let results = analysis::run_audio_correlation(
                        &runner,
                        &self.config,
                        &job.ref_file,
                        target_file,
                        &temp_dir,
                        ref_lang,
                        target_lang,
                    )
                    .await?;
                    Ok(analysis::best_from_results(&results, self.config.min_match_pct)
                    .map(|b| b.delay_ms)
                    .unwrap_or(0))
                }
            }
        };

        let delay_sec = if let Some(sec_file) = &job.sec_file {
            runner
            .send_log(&format!("Analyzing Secondary file ({})", analysis_mode))
            .await;
            let delay =
            analyze_target(sec_file, &self.config.analysis_lang_ref, &self.config.analysis_lang_sec)
            .await?;
            runner
            .send_log(&format!("Secondary delay determined: {} ms", delay))
            .await;
            Some(delay)
        } else {
            None
        };

        let delay_ter = if let Some(ter_file) = &job.ter_file {
            runner
            .send_log(&format!("Analyzing Tertiary file ({})", analysis_mode))
            .await;
            let delay =
            analyze_target(ter_file, &self.config.analysis_lang_ref, &self.config.analysis_lang_ter)
            .await?;
            runner
            .send_log(&format!("Tertiary delay determined: {} ms", delay))
            .await;
            Some(delay)
        } else {
            None
        };

        let delay_sec_val = delay_sec.unwrap_or(0);
        let delay_ter_val = delay_ter.unwrap_or(0);

        if !and_merge {
            fs::remove_dir_all(&temp_dir).await.ok();
            return Ok(format!(
                "Analysis Complete.\n  - Secondary Delay: {} ms\n  - Tertiary Delay: {} ms",
                delay_sec_val, delay_ter_val
            ));
        }

        // --- Merge Planning & Extraction ---
        runner.send_log("--- Merge Planning Phase ---").await;
        let min_delay = 0i64.min(delay_sec_val).min(delay_ter_val);
        let global_shift = if min_delay < 0 { -min_delay } else { 0 };
        runner
        .send_log(&format!(
            "[Delay] Applying lossless global shift: +{} ms",
            global_shift
        ))
        .await;

        runner.send_log("--- Extraction Phase ---").await;
        let mut final_layout: Vec<TrackSelection> = Vec::new();

        // Group tracks to extract by source file role.
        let mut tracks_to_extract: HashMap<&str, (Vec<mkv_utils::Track>, &str)> = HashMap::new();
        for selection in layout {
            let entry = tracks_to_extract
            .entry(&selection.source)
            .or_insert((vec![], ""));
            entry.0.push(selection.original_track.clone());
            entry.1 = match selection.source.as_str() {
                "REF" => &job.ref_file,
                "SEC" => job.sec_file.as_ref().unwrap(),
                "TER" => job.ter_file.as_ref().unwrap(),
                _ => "",
            };
        }

        // Extract to one-file-per-track (keeps mkvmerge track id = 0 for each file).
        for (role, (tracks, file_path)) in tracks_to_extract {
            let extracted =
            mkv_utils::extract_tracks(&runner, Path::new(file_path), &tracks, &temp_dir, role)
            .await?;
            for ext_track in extracted {
                if let Some(original_selection) = layout.iter().find(|s| {
                    s.original_track.id == ext_track.original_track.id && s.source == role
                }) {
                    let mut new_selection = original_selection.clone();
                    new_selection.extracted_path = Some(ext_track.path);
                    final_layout.push(new_selection);
                }
            }
        }
        // Keep the original selection order for user intent.
        final_layout.sort_by_key(|s| {
            layout
            .iter()
            .position(|orig| orig.original_track.id == s.original_track.id && orig.source == s.source)
            .unwrap_or(usize::MAX)
        });

        // --- Subtitle Post-Processing ---
        runner.send_log("--- Subtitle Processing Phase ---").await;
        for selection in final_layout.iter_mut() {
            if selection.original_track.r#type == "subtitles" {
                if let Some(extracted_path) = &selection.extracted_path {
                    let mut current_path = extracted_path.clone();
                    if selection.convert_to_ass {
                        current_path = subtitle_utils::convert_srt_to_ass(&runner, &current_path).await?;
                    }
                    if selection.rescale {
                        subtitle_utils::rescale_subtitle(&runner, &current_path, &job.ref_file).await?;
                    }
                    if (selection.size_multiplier - 1.0).abs() > 1e-9 {
                        let content = subtitle_utils::read_subtitle_file(&current_path)?;
                        let (new_content, count) =
                        subtitle_utils::multiply_font_size(&content, selection.size_multiplier);
                        if count > 0 {
                            subtitle_utils::write_subtitle_file(&current_path, &new_content)?;
                            runner
                            .send_log(&format!(
                                "[Font Size] Modified {} style definition(s).",
                                               count
                            ))
                            .await;
                        }
                    }
                    selection.extracted_path = Some(current_path); // Path may change (e.g., SRT→ASS)
                }
            }
        }

        let chapters_path =
        mkv_utils::process_chapters(&runner, &job.ref_file, &temp_dir, global_shift, &self.config)
        .await?;
        let ter_attachments = if let Some(ter_file) = &job.ter_file {
            mkv_utils::extract_attachments(&runner, ter_file, &temp_dir).await?
        } else {
            vec![]
        };

        // --- Build MKVToolNix-style opts.json tokens and mux ---
        runner.send_log("--- Merge Execution ---").await;
        let output_filename = Path::new(&job.ref_file).file_name().unwrap();
        let output_path = PathBuf::from(&self.config.output_folder).join(output_filename);

        let tokens = self.build_mkvmerge_tokens(
            &output_path,
            global_shift,
            delay_sec,
            delay_ter,
            &final_layout,
            chapters_path.as_deref(),
                                                &ter_attachments,
        );

        let opts_path = temp_dir.join("opts.json");
        fs::write(&opts_path, serde_json::to_string(&tokens).unwrap())
        .await
        .map_err(|e| e.to_string())?;

        let result = runner
        .run("mkvmerge", &[&format!("@{}", opts_path.to_string_lossy())])
        .await?;

        if result.exit_code == 0 {
            fs::remove_dir_all(&temp_dir).await.ok();
            Ok(format!(
                "Merge successful! Output file: {}",
                output_path.to_string_lossy()
            ))
        } else {
            runner
            .send_log(&format!(
                "mkvmerge failed. Temp files kept for inspection at: {}",
                temp_dir.display()
            ))
            .await;
            Err("mkvmerge failed during final mux.".to_string())
        }
    }

    /// Build mkvmerge tokens following MKVToolNix export style:
    /// - Per-track options come *before* each `( file )`
    /// - One extracted file per track (track id = 0 in each)
    /// - AC3/E-AC3: `--remove-dialog-normalization-gain 0`
    /// - Attachments: `--attachment-name`, `--attachment-mime-type`, then `--attach-file`
    fn build_mkvmerge_tokens(
        &self,
        output_path: &Path,
        global_shift: i64,
        delay_sec: Option<i64>,
        delay_ter: Option<i64>,
        layout: &[TrackSelection],
        chapters_path: Option<&Path>,
        attachments: &[PathBuf],
    ) -> Vec<String> {
        let mut tokens: Vec<String> =
        vec!["--output".into(), output_path.to_string_lossy().to_string()];

        if self.config.disable_track_statistics_tags {
            tokens.push("--disable-track-statistics-tags".into());
        }

        // Track order will be "file_index:0" for each `( single-track file )` we add.
        let mut track_order: Vec<String> = Vec::new();
        let mut file_id_counter: usize = 0;

        for selection in layout {
            let tid = 0; // each extracted file has exactly one track with id 0
            let track = &selection.original_track;

            // Compute per-file sync (apply global shift + per-source delay)
            let per_file_sync = match selection.source.as_str() {
                "SEC" => delay_sec.unwrap_or(0) + global_shift,
                "TER" => delay_ter.unwrap_or(0) + global_shift,
                _ => global_shift, // REF selections also get global shift
            };

            // --- Per-track options block (before "(" file ")") ---
            tokens.push("--sync".into());
            tokens.push(format!("{}:{}", tid, per_file_sync));

            // AC3/E-AC3 : remove dialog normalization gain (use MKVToolNix form "0")
            if self.config.apply_dialog_norm_gain {
                if let Some(codec) = &track.properties.codec_id {
                    if codec.contains("AC3") {
                        tokens.push("--remove-dialog-normalization-gain".into());
                        tokens.push(tid.to_string());
                    }
                }
            }

            // Language
            let lang = track.properties.language.as_deref().unwrap_or("und");
            tokens.push("--language".into());
            tokens.push(format!("{}:{}", tid, lang));

            // Track name (if requested and present)
            if selection.apply_track_name {
                if let Some(name) = &track.properties.track_name {
                    if !name.is_empty() {
                        tokens.push("--track-name".into());
                        tokens.push(format!("{}:{}", tid, name));
                    }
                }
            }

            // Default flag
            tokens.push("--default-track-flag".into());
            tokens.push(format!("{}:{}", tid, if selection.is_default { "yes" } else { "no" }));

            // Forced (only for subtitles)
            if track.r#type == "subtitles" && selection.is_forced {
                tokens.push("--forced-display-flag".into());
                tokens.push(format!("{}:yes", tid));
            }

            // Compression
            tokens.push("--compression".into());
            tokens.push(format!("{}:none", tid));

            // The file for this single track
            if let Some(path) = &selection.extracted_path {
                tokens.push("(".into());
                tokens.push(path.to_string_lossy().to_string());
                tokens.push(")".into());

                track_order.push(format!("{}:{}", file_id_counter, tid));
                file_id_counter += 1;
            }
        }

        // Chapters (if any)
        if let Some(path) = chapters_path {
            tokens.push("--chapters".into());
            tokens.push(path.to_string_lossy().to_string());
        }

        // Attachments, with name + mime like MKVToolNix exports
        for att in attachments {
            let name = att
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "attachment.bin".to_string());
            let mime = guess_mime_for_attachment(att);

            tokens.push("--attachment-name".into());
            tokens.push(name);
            tokens.push("--attachment-mime-type".into());
            tokens.push(mime.to_string());
            tokens.push("--attach-file".into());
            tokens.push(att.to_string_lossy().to_string());
        }

        // Track order
        if !track_order.is_empty() {
            tokens.push("--track-order".into());
            tokens.push(track_order.join(","));
        }

        tokens
    }
}

// --- Helpers ---

/// Best-effort MIME guessing for attachments (to match MKVToolNix export style).
fn guess_mime_for_attachment(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) {
        Some(ext) if ext == "ttf" => "font/ttf",
        Some(ext) if ext == "otf" => "font/otf",
        Some(ext) if ext == "otc" => "font/collection",
        Some(ext) if ext == "ttc" => "font/collection",
        Some(ext) if ext == "woff" => "font/woff",
        Some(ext) if ext == "woff2" => "font/woff2",
        Some(ext) if ext == "png" => "image/png",
        Some(ext) if ext == "jpg" || ext == "jpeg" => "image/jpeg",
        Some(ext) if ext == "svg" => "image/svg+xml",
        Some(ext) if ext == "xml" => "application/xml",
        Some(ext) if ext == "json" => "application/json",
        _ => "application/octet-stream",
    }
}
