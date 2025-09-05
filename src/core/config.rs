// src/core/config.rs
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

// Helper functions to provide default values for the config struct.
fn default_output_folder() -> String { "sync_output".to_string() }
fn default_temp_root() -> String { "temp_work".to_string() }
fn default_analysis_mode() -> String { "Audio Correlation".to_string() }
fn default_scan_chunk_count() -> u32 { 10 }
fn default_scan_chunk_duration() -> u32 { 15 }
fn default_min_match_pct() -> f64 { 5.0 }
fn default_videodiff_error_max() -> f64 { 100.0 }
fn default_snap_mode() -> String { "previous".to_string() }
fn default_snap_threshold_ms() -> u32 { 250 }
fn default_snap_starts_only() -> bool { true }
fn default_log_compact() -> bool { true }
fn default_log_autoscroll() -> bool { true }
fn default_log_error_tail() -> u32 { 20 }
fn default_log_progress_step() -> u32 { 20 }
fn default_archive_logs() -> bool { true }

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppConfig {
    #[serde(default)]
    pub last_ref_path: String,
    #[serde(default)]
    pub last_sec_path: String,
    #[serde(default)]
    pub last_ter_path: String,
    #[serde(default = "default_output_folder")]
    pub output_folder: String,
    #[serde(default = "default_temp_root")]
    pub temp_root: String,
    #[serde(default)]
    pub videodiff_path: String,
    #[serde(default = "default_analysis_mode")]
    pub analysis_mode: String,
    #[serde(default)]
    pub analysis_lang_ref: String,
    #[serde(default)]
    pub analysis_lang_sec: String,
    #[serde(default)]
    pub analysis_lang_ter: String,
    #[serde(default = "default_scan_chunk_count")]
    pub scan_chunk_count: u32,
    #[serde(default = "default_scan_chunk_duration")]
    pub scan_chunk_duration: u32,
    #[serde(default = "default_min_match_pct")]
    pub min_match_pct: f64,
    #[serde(default)]
    pub videodiff_error_min: f64,
    #[serde(default = "default_videodiff_error_max")]
    pub videodiff_error_max: f64,
    #[serde(default)]
    pub rename_chapters: bool,
    #[serde(default)]
    pub apply_dialog_norm_gain: bool,
    #[serde(default)]
    pub snap_chapters: bool,
    #[serde(default = "default_snap_mode")]
    pub snap_mode: String,
    #[serde(default = "default_snap_threshold_ms")]
    pub snap_threshold_ms: u32,
    #[serde(default = "default_snap_starts_only")]
    pub snap_starts_only: bool,
    #[serde(default = "default_log_compact")]
    pub log_compact: bool,
    #[serde(default = "default_log_autoscroll")]
    pub log_autoscroll: bool,
    #[serde(default = "default_log_error_tail")]
    pub log_error_tail: u32,
    #[serde(default)]
    pub log_tail_lines: u32,
    #[serde(default = "default_log_progress_step")]
    pub log_progress_step: u32,
    #[serde(default)]
    pub log_show_options_pretty: bool,
    #[serde(default)]
    pub log_show_options_json: bool,
    #[serde(default)]
    pub disable_track_statistics_tags: bool,
    #[serde(default = "default_archive_logs")]
    pub archive_logs: bool,
    #[serde(default)]
    pub auto_apply_strict: bool,
}

impl AppConfig {
    fn get_settings_path() -> PathBuf { PathBuf::from("settings.json") }

    pub fn load() -> Self {
        let path = Self::get_settings_path();
        if !path.exists() { return AppConfig::default(); }
        let content = fs::read_to_string(path).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or_default()
    }

    pub fn save(&self) {
        let path = Self::get_settings_path();
        let content = serde_json::to_string_pretty(&self).expect("Failed to serialize config");
        fs::write(path, content).expect("Failed to write settings.json");
    }

    pub fn ensure_dirs_exist(&self) {
        fs::create_dir_all(Path::new(&self.output_folder)).expect("Failed to create output folder");
        fs::create_dir_all(Path::new(&self.temp_root)).expect("Failed to create temp folder");
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            last_ref_path: String::new(),
            last_sec_path: String::new(),
            last_ter_path: String::new(),
            output_folder: default_output_folder(),
            temp_root: default_temp_root(),
            videodiff_path: String::new(),
            analysis_mode: default_analysis_mode(),
            analysis_lang_ref: String::new(),
            analysis_lang_sec: String::new(),
            analysis_lang_ter: String::new(),
            scan_chunk_count: default_scan_chunk_count(),
            scan_chunk_duration: default_scan_chunk_duration(),
            min_match_pct: default_min_match_pct(),
            videodiff_error_min: 0.0,
            videodiff_error_max: default_videodiff_error_max(),
            rename_chapters: false,
            apply_dialog_norm_gain: false,
            snap_chapters: false,
            snap_mode: default_snap_mode(),
            snap_threshold_ms: default_snap_threshold_ms(),
            snap_starts_only: default_snap_starts_only(),
            log_compact: default_log_compact(),
            log_autoscroll: default_log_autoscroll(),
            log_error_tail: default_log_error_tail(),
            log_tail_lines: 0,
            log_progress_step: default_log_progress_step(),
            log_show_options_pretty: false,
            log_show_options_json: false,
            disable_track_statistics_tags: false,
            archive_logs: default_archive_logs(),
            auto_apply_strict: false,
        }
    }
}
