// src/core/pipeline.rs

use serde_json;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::sync::mpsc;

use crate::core::config::AppConfig;
use crate::core::process::CommandRunner;
use crate::core::{analysis, mkv_utils, subtitle_utils};

#[derive(Clone)]
pub struct Job {
    pub ref_file: String,
    pub sec_file: Option<String>,
    pub ter_file: Option<String>,
}

// A complete representation of a user's choice for a single track in the output.
#[derive(Debug, Clone)]
pub struct TrackSelection {
    pub source: String, // "REF", "SEC", or "TER"
    pub original_track: mkv_utils::Track,
    pub extracted_path: PathBuf,
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

    pub async fn run_job(&self, job: &Job, and_merge: bool, layout: &mut [TrackSelection]) -> Result<String, String> {
        let runner = CommandRunner::new(self.config.clone(), self.log_sender.clone());
        let temp_dir_name = format!("job_{}_{}", Path::new(&job.ref_file).file_stem().unwrap().to_str().unwrap(), chrono::Utc::now().timestamp());
        let temp_dir = PathBuf::from(&self.config.temp_root).join(temp_dir_name);
        fs::create_dir_all(&temp_dir).await.map_err(|e| e.to_string())?;

        runner.send_log("--- Analysis Phase ---").await;
        let delay_sec = if let Some(sec_file) = &job.sec_file {
            let results = analysis::run_audio_correlation(&runner, &job.ref_file, sec_file, &temp_dir).await?;
            analysis::best_from_results(&results).map(|b| b.delay_ms)
        } else { None };
        let delay_ter = None; // Placeholder for now

        let delay_sec_val = delay_sec.unwrap_or(0);
        let delay_ter_val = delay_ter.unwrap_or(0);
        runner.send_log(&format!("Secondary delay determined: {} ms", delay_sec_val)).await;

        if !and_merge {
            fs::remove_dir_all(&temp_dir).await.ok();
            return Ok(format!("Analysis Complete.\n  - Secondary Delay: {} ms\n  - Tertiary Delay: {} ms", delay_sec_val, delay_ter_val));
        }

        runner.send_log("--- Merge Planning Phase ---").await;
        let min_delay = 0i64.min(delay_sec_val).min(delay_ter_val);
        let global_shift = if min_delay < 0 { -min_delay } else { 0 };
        runner.send_log(&format!("[Delay] Applying lossless global shift: +{} ms", global_shift)).await;

        runner.send_log("--- Subtitle Processing Phase ---").await;
        for selection in layout.iter_mut() {
            if selection.original_track.r#type == "subtitles" {
                if selection.convert_to_ass {
                    let new_path = subtitle_utils::convert_srt_to_ass(&runner, &selection.extracted_path).await?;
                    selection.extracted_path = new_path;
                }
                if selection.rescale {
                    subtitle_utils::rescale_subtitle(&runner, &selection.extracted_path, &job.ref_file).await?;
                }
                if (selection.size_multiplier - 1.0).abs() > 1e-9 {
                    let content = subtitle_utils::read_subtitle_file(&selection.extracted_path)?;
                    let (new_content, count) = subtitle_utils::multiply_font_size(&content, selection.size_multiplier);
                    if count > 0 {
                        subtitle_utils::write_subtitle_file(&selection.extracted_path, &new_content)?;
                        runner.send_log(&format!("[Font Size] Modified {} style definition(s).", count)).await;
                    }
                }
            }
        }

        let chapters_path = mkv_utils::process_chapters(&runner, &job.ref_file, &temp_dir, global_shift, &self.config).await?;
        let ter_attachments = if let Some(ter_file) = &job.ter_file {
            mkv_utils::extract_attachments(&runner, ter_file, &temp_dir).await?
        } else { vec![] };

        runner.send_log("--- Merge Execution ---").await;
        let output_filename = Path::new(&job.ref_file).file_name().unwrap();
        let output_path = PathBuf::from(&self.config.output_folder).join(output_filename);

        let tokens = self.build_mkvmerge_tokens(&output_path, global_shift, delay_sec, delay_ter, layout, chapters_path.as_deref(), &ter_attachments);

        let opts_path = temp_dir.join("opts.json");
        fs::write(&opts_path, serde_json::to_string(&tokens).unwrap()).await.map_err(|e| e.to_string())?;

        let result = runner.run("mkvmerge", &[&format!("@{}", opts_path.to_string_lossy())]).await?;

        fs::remove_dir_all(&temp_dir).await.ok();

        if result.exit_code == 0 {
            Ok(format!("Merge successful! Output file: {}", output_path.to_string_lossy()))
        } else {
            Err("mkvmerge failed during final mux.".to_string())
        }
    }

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
        let mut tokens = vec!["--output".to_string(), output_path.to_string_lossy().to_string()];

        if self.config.disable_track_statistics_tags {
            tokens.push("--disable-track-statistics-tags".to_string());
        }

        let mut track_order = Vec::new();
        let mut file_id_counter = 0;

        for selection in layout {
            let track_id_in_file = 0; // Each extracted file has only one track
            let track = &selection.original_track;

            let sync = match selection.source.as_str() {
                "SEC" => delay_sec.unwrap_or(0) + global_shift,
                "TER" => delay_ter.unwrap_or(0) + global_shift,
                _ => global_shift,
            };
            tokens.extend_from_slice(&["--sync".to_string(), format!("{}:{}", track_id_in_file, sync)]);
            tokens.extend_from_slice(&["--language".to_string(), format!("{}:{}", track_id_in_file, track.properties.language.as_deref().unwrap_or("und"))]);

            if selection.apply_track_name {
                if let Some(name) = &track.properties.track_name {
                    if !name.is_empty() {
                        tokens.extend_from_slice(&["--track-name".to_string(), format!("{}:{}", track_id_in_file, name)]);
                    }
                }
            }

            tokens.extend_from_slice(&["--default-track-flag".to_string(), format!("{}:{}", track_id_in_file, if selection.is_default {"yes"} else {"no"})]);

            if track.r#type == "subtitles" && selection.is_forced {
                tokens.extend_from_slice(&["--forced-display-flag".to_string(), format!("{}:yes", track_id_in_file)]);
            }

            if self.config.apply_dialog_norm_gain {
                if let Some(codec) = &track.properties.codec_id {
                    if codec.contains("AC3") { // Simplified check for E-AC3 as well
                        tokens.extend_from_slice(&["--remove-dialog-normalization-gain".to_string(), format!("{}:1", track_id_in_file)]);
                    }
                }
            }
            tokens.push("--compression".to_string());
            tokens.push(format!("{}:none", track_id_in_file));

            tokens.extend_from_slice(&["(".to_string(), selection.extracted_path.to_string_lossy().to_string(), ")".to_string()]);
            track_order.push(format!("{}:{}", file_id_counter, track_id_in_file));
            file_id_counter += 1;
        }

        if let Some(path) = chapters_path {
            tokens.extend_from_slice(&["--chapters".to_string(), path.to_string_lossy().to_string()]);
        }

        for att_path in attachments {
            tokens.extend_from_slice(&["--attach-file".to_string(), att_path.to_string_lossy().to_string()]);
        }

        if !track_order.is_empty() {
            tokens.extend_from_slice(&["--track-order".to_string(), track_order.join(",")]);
        }

        tokens
    }
}
