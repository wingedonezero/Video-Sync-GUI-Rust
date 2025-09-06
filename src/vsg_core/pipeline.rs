// src/vsg_core/pipeline.rs

use crate::{
    analysis, config::Config, mkv_utils::{self, ExtractedTrack}, process, subtitle_utils
};
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Represents a track chosen by the user, with all associated muxing options.
#[derive(Debug, Clone)]
pub struct ManualTrack {
    pub source: String, // "REF", "SEC", or "TER"
    pub id: u64,
    pub track_type: String,
    pub is_default: bool,
    pub is_forced_display: bool,
    pub apply_track_name: bool,
    pub convert_to_ass: bool,
    pub rescale: bool,
    pub size_multiplier: f64,
}

pub struct JobPipeline {
    pub config: Config,
}

impl JobPipeline {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn run_job<F>(
        &self,
        ref_file: &Path,
        sec_file: Option<&Path>,
        ter_file: Option<&Path>,
        and_merge: bool,
        manual_layout: &[ManualTrack],
        output_dir: &Path,
        log_callback: Arc<Mutex<F>>,
    ) -> Result<()>
    where
    F: FnMut(String) + Send + 'static,
    {
        // --- 1. Tool Discovery ---
        for tool in ["ffmpeg", "ffprobe", "mkvmerge", "mkvextract"] {
            if which::which(tool).is_err() {
                return Err(anyhow!("Required tool '{}' not found in PATH.", tool));
            }
        }

        let start = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let job_temp_dir = self.config.temp_root.join(format!(
            "job_{}_{}",
            ref_file.file_stem().unwrap().to_str().unwrap(),
                                                              start
        ));
        std::fs::create_dir_all(&job_temp_dir)?;

        // --- 2. Analysis Phase ---
        log_callback.lock().unwrap()("--- Analysis Phase ---".to_string());
        let ref_lang = if self.config.analysis_lang_ref.is_empty() { None } else { Some(self.config.analysis_lang_ref.as_str()) };
        let sec_lang = if self.config.analysis_lang_sec.is_empty() { None } else { Some(self.config.analysis_lang_sec.as_str()) };
        let ter_lang = if self.config.analysis_lang_ter.is_empty() { None } else { Some(self.config.analysis_lang_ter.as_str()) };

        let mut delay_sec = None;
        if let Some(file) = sec_file {
            log_callback.lock().unwrap()(format!("Analyzing Secondary file ({})...", self.config.analysis_mode));
            let delay = if self.config.analysis_mode == "VideoDiff" {
                analysis::run_videodiff(&self.config, ref_file, file, Arc::clone(&log_callback))?.0
            } else {
                let results = analysis::run_audio_correlation(&self.config, ref_file, file, &job_temp_dir, ref_lang, sec_lang, Arc::clone(&log_callback))?;
                analysis::best_from_results(&self.config, &results).map_or(0, |r| r.delay_ms)
            };
            delay_sec = Some(delay);
            log_callback.lock().unwrap()(format!("Secondary delay determined: {} ms", delay));
        }

        let mut delay_ter = None;
        if let Some(file) = ter_file {
            log_callback.lock().unwrap()(format!("Analyzing Tertiary file ({})...", self.config.analysis_mode));
            let delay = if self.config.analysis_mode == "VideoDiff" {
                analysis::run_videodiff(&self.config, ref_file, file, Arc::clone(&log_callback))?.0
            } else {
                let results = analysis::run_audio_correlation(&self.config, ref_file, file, &job_temp_dir, ref_lang, ter_lang, Arc::clone(&log_callback))?;
                analysis::best_from_results(&self.config, &results).map_or(0, |r| r.delay_ms)
            };
            delay_ter = Some(delay);
            log_callback.lock().unwrap()(format!("Tertiary delay determined: {} ms", delay));
        }

        if !and_merge {
            log_callback.lock().unwrap()("--- Analysis Complete (No Merge) ---".to_string());
            std::fs::remove_dir_all(job_temp_dir)?;
            return Ok(());
        }

        if manual_layout.is_empty() {
            return Err(anyhow!("Cannot merge without a track layout."));
        }

        // --- 3. Merge Planning, Extraction, Processing ---
        log_callback.lock().unwrap()("--- Merge Planning Phase ---".to_string());
        let delays = [0, delay_sec.unwrap_or(0), delay_ter.unwrap_or(0)];
        let min_delay = *delays.iter().min().unwrap_or(&0);
        let global_shift = if min_delay < 0 { -min_delay } else { 0 };
        log_callback.lock().unwrap()(format!("[Delay] Applying lossless global shift: +{} ms", global_shift));

        log_callback.lock().unwrap()("--- Extraction Phase ---".to_string());
        let ref_ids: Vec<u64> = manual_layout.iter().filter(|t| t.source == "REF").map(|t| t.id).collect();
        let sec_ids: Vec<u64> = manual_layout.iter().filter(|t| t.source == "SEC").map(|t| t.id).collect();
        let ter_ids: Vec<u64> = manual_layout.iter().filter(|t| t.source == "TER").map(|t| t.id).collect();

        let mut all_extracted = Vec::new();
        all_extracted.extend(mkv_utils::extract_tracks(&self.config, ref_file, &job_temp_dir, "ref", &ref_ids, Arc::clone(&log_callback))?);
        if let Some(path) = sec_file { all_extracted.extend(mkv_utils::extract_tracks(&self.config, path, &job_temp_dir, "sec", &sec_ids, Arc::clone(&log_callback))?); }
        if let Some(path) = ter_file { all_extracted.extend(mkv_utils::extract_tracks(&self.config, path, &job_temp_dir, "ter", &ter_ids, Arc::clone(&log_callback))?); }

        log_callback.lock().unwrap()("--- Post-Extraction Processing ---".to_string());
        for track_layout in manual_layout {
            if track_layout.track_type == "subtitles" {
                if let Some(extracted_file) = all_extracted.iter_mut().find(|ef| ef.source == track_layout.source && ef.id == track_layout.id) {
                    let mut current_path = extracted_file.path.clone();
                    if track_layout.convert_to_ass {
                        current_path = subtitle_utils::convert_srt_to_ass(&self.config, &current_path, Arc::clone(&log_callback))?;
                    }
                    if track_layout.rescale {
                        subtitle_utils::rescale_subtitle(&self.config, &current_path, ref_file, Arc::clone(&log_callback))?;
                    }
                    if (track_layout.size_multiplier - 1.0).abs() > 1e-6 {
                        subtitle_utils::multiply_font_size(&current_path, track_layout.size_multiplier, Arc::clone(&log_callback))?;
                    }
                    extracted_file.path = current_path;
                }
            }
        }

        let chapters_xml = mkv_utils::process_chapters(&self.config, ref_file, &job_temp_dir, global_shift, Arc::clone(&log_callback))?;
        let attachments = if let Some(ter_file) = ter_file {
            mkv_utils::extract_attachments(&self.config, ter_file, &job_temp_dir, "ter", Arc::clone(&log_callback))?
        } else { Vec::new() };

        // --- 5. Mkvmerge Execution Phase ---
        log_callback.lock().unwrap()("--- Merge Execution Phase ---".to_string());
        let tokens = self.build_mkvmerge_tokens(manual_layout, &all_extracted, output_dir, ref_file, chapters_xml.as_deref(), &attachments, global_shift, delay_sec, delay_ter, Arc::clone(&log_callback))?;

        let opts_path = job_temp_dir.join("opts.json");
        let opts_content = serde_json::to_string_pretty(&tokens)?;
        std::fs::write(&opts_path, opts_content)?;

        process::run_command(&self.config, "mkvmerge", &[&format!("@{}", opts_path.display())], Arc::clone(&log_callback))?;

        let out_file = output_dir.join(ref_file.file_name().unwrap());
        log_callback.lock().unwrap()(format!("[SUCCESS] Output file created: {}", out_file.display()));
        std::fs::remove_dir_all(job_temp_dir)?;

        Ok(())
    }

    fn build_mkvmerge_tokens<F>(
        &self,
        layout: &[ManualTrack],
        extracted: &[ExtractedTrack],
        output_dir: &Path,
        ref_file: &Path,
        chapters_xml: Option<&Path>,
        attachments: &[PathBuf],
        global_shift: i64,
        delay_sec: Option<i64>,
        delay_ter: Option<i64>,
        log_callback: Arc<Mutex<F>>,
    ) -> Result<Vec<String>>
    where
    F: FnMut(String) + Send + 'static,
    {
        let mut tokens: Vec<String> = Vec::new();
        let out_file = output_dir.join(ref_file.file_name().unwrap());

        tokens.extend(["--output".to_string(), out_file.to_str().unwrap().to_string()]);
        if let Some(chapters) = chapters_xml {
            tokens.extend(["--chapters".to_string(), chapters.to_str().unwrap().to_string()]);
        }
        if self.config.disable_track_statistics_tags {
            tokens.push("--disable-track-statistics-tags".to_string());
        }

        // --- Log only guardrails from python version ---
        if !layout.iter().any(|t| t.source == "REF" && t.track_type == "video") {
            log_callback.lock().unwrap()("[WARN] No REF video present in final plan.".to_string());
        }
        if layout.iter().any(|t| t.source != "REF" && t.track_type == "video") {
            log_callback.lock().unwrap()("[WARN] Non-REF video detected in final plan.".to_string());
        }

        let mut layout_mut = layout.to_vec();
        if let Some(first_video) = layout_mut.iter_mut().find(|t| t.track_type == "video") {
            first_video.is_default = true;
        }

        let mut track_order = Vec::new();
        for (i, track_layout) in layout_mut.iter().enumerate() {
            if let Some(extracted_track) = extracted.iter().find(|e| e.source == track_layout.source && e.id == track_layout.id) {
                let delay = match track_layout.source.as_str() {
                    "SEC" => global_shift + delay_sec.unwrap_or(0),
                    "TER" => global_shift + delay_ter.unwrap_or(0),
                    _ => global_shift,
                };

                tokens.extend(["--language".to_string(), format!("0:{}", extracted_track.lang)]);
                if track_layout.apply_track_name && !extracted_track.name.is_empty() {
                    tokens.extend(["--track-name".to_string(), format!("0:{}", extracted_track.name)]);
                }
                tokens.extend(["--sync".to_string(), format!("0:{}", delay)]);
                tokens.extend(["--default-track-flag".to_string(), format!("0:{}", if track_layout.is_default { "yes" } else { "no" })]);
                if track_layout.is_forced_display {
                    tokens.extend(["--forced-display-flag".to_string(), "0:yes".to_string()]);
                }
                tokens.extend(["--compression".to_string(), "0:none".to_string()]);

                if self.config.apply_dialog_norm_gain && extracted_track.track_type == "audio" {
                    if extracted_track.codec_id.contains("AC3") || extracted_track.codec_id.contains("EAC3") {
                        tokens.extend(["--remove-dialog-normalization-gain".to_string(), "0".to_string()]);
                    }
                }

                tokens.extend(["(".to_string(), extracted_track.path.to_str().unwrap().to_string(), ")".to_string()]);
                track_order.push(format!("{}:0", i));
            }
        }

        for att_path in attachments {
            tokens.extend(["--attach-file".to_string(), att_path.to_str().unwrap().to_string()]);
        }

        if !track_order.is_empty() {
            tokens.extend(["--track-order".to_string(), track_order.join(",")]);
        }

        Ok(tokens)
    }
}
