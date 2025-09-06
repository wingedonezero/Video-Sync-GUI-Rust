// src/vsg_core/config.rs

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::{env, fs};

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub last_ref_path: String,
    pub last_sec_path: String,
    pub last_ter_path: String,
    pub output_folder: PathBuf,
    pub temp_root: PathBuf,
    pub videodiff_path: String,
    pub analysis_mode: String,
    pub analysis_lang_ref: String,
    pub analysis_lang_sec: String,
    pub analysis_lang_ter: String,
    pub scan_chunk_count: u32,
    pub scan_chunk_duration: u32,
    pub min_match_pct: f64,
    pub videodiff_error_min: f64,
    pub videodiff_error_max: f64,
    pub rename_chapters: bool,
    pub apply_dialog_norm_gain: bool,
    pub snap_chapters: bool,
    pub snap_mode: String,
    pub snap_threshold_ms: u32,
    pub snap_starts_only: bool,
    pub log_compact: bool,
    pub log_autoscroll: bool,
    pub log_error_tail: u32,
    pub log_tail_lines: u32,
    pub log_progress_step: u32,
    pub log_show_options_pretty: bool,
    pub log_show_options_json: bool,
    pub disable_track_statistics_tags: bool,
    pub archive_logs: bool,
    pub auto_apply_strict: bool,
}

impl Config {
    /// Loads configuration from settings.json or returns default values.
    pub fn load() -> Self {
        let config_path = match env::current_exe() {
            Ok(exe_path) => exe_path.parent().unwrap().join("settings.json"),
            Err(_) => PathBuf::from("settings.json"),
        };

        if config_path.exists() {
            if let Ok(content) = fs::read_to_string(&config_path) {
                // Attempt to deserialize, but fall back to default if it fails
                // This also handles cases where new fields are added to the struct
                serde_json::from_str(&content).unwrap_or_else(|_| Self::default())
            } else {
                Self::default()
            }
        } else {
            let config = Self::default();
            config.save(); // Save the default config on first run
            config
        }
    }

    /// Saves the current configuration to settings.json.
    pub fn save(&self) {
        let config_path = match env::current_exe() {
            Ok(exe_path) => exe_path.parent().unwrap().join("settings.json"),
            Err(_) => PathBuf::from("settings.json"),
        };

        let content = serde_json::to_string_pretty(self).unwrap();
        fs::write(config_path, content).expect("Failed to write settings.json");
    }
}

impl Default for Config {
    fn default() -> Self {
        let script_dir = match env::current_exe() {
            Ok(exe_path) => exe_path.parent().unwrap().to_path_buf(),
            Err(_) => PathBuf::from("."),
        };

        let config = Self {
            last_ref_path: "".to_string(),
            last_sec_path: "".to_string(),
            last_ter_path: "".to_string(),
            output_folder: script_dir.join("sync_output"),
            temp_root: script_dir.join("temp_work"),
            videodiff_path: "".to_string(),
            analysis_mode: "Audio Correlation".to_string(),
            analysis_lang_ref: "".to_string(),
            analysis_lang_sec: "".to_string(),
            analysis_lang_ter: "".to_string(),
            scan_chunk_count: 10,
            scan_chunk_duration: 15,
            min_match_pct: 5.0,
            videodiff_error_min: 0.0,
            videodiff_error_max: 100.0,
            rename_chapters: false,
            apply_dialog_norm_gain: false,
            snap_chapters: false,
            snap_mode: "previous".to_string(),
            snap_threshold_ms: 250,
            snap_starts_only: true,
            log_compact: true,
            log_autoscroll: true,
            log_error_tail: 20,
            log_tail_lines: 0,
            log_progress_step: 20,
            log_show_options_pretty: false,
            log_show_options_json: false,
            disable_track_statistics_tags: false,
            archive_logs: true,
            auto_apply_strict: false,
        };

        // Ensure the default directories exist
        fs::create_dir_all(&config.output_folder).ok();
        fs::create_dir_all(&config.temp_root).ok();

        config
    }
}
