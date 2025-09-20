use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    // File paths
    pub last_ref_path: String,
    pub last_sec_path: String,
    pub last_ter_path: String,
    pub output_folder: String,
    pub temp_root: String,

    // Analysis settings
    pub min_match_pct: f32,
    pub scan_chunk_count: i32,
    pub scan_chunk_duration: i32,
    pub segmented_enabled: bool,

    // Chapter settings
    pub rename_chapters: bool,
    pub snap_chapters: bool,
    pub snap_threshold_ms: i32,

    // UI settings
    pub archive_logs: bool,
    pub log_autoscroll: bool,
    pub log_compact: bool,

    // Merge settings
    pub disable_track_statistics_tags: bool,
    pub disable_header_compression: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let output_folder = home.join("sync_output").display().to_string();
        let temp_root = home.join("temp_work").display().to_string();

        Self {
            last_ref_path: String::new(),
            last_sec_path: String::new(),
            last_ter_path: String::new(),
            output_folder,
            temp_root,
            min_match_pct: 5.0,
            scan_chunk_count: 10,
            scan_chunk_duration: 15,
            segmented_enabled: false,
            rename_chapters: false,
            snap_chapters: false,
            snap_threshold_ms: 250,
            archive_logs: true,
            log_autoscroll: true,
            log_compact: true,
            disable_track_statistics_tags: false,
            disable_header_compression: true,
        }
    }
}

impl AppConfig {
    pub fn config_path() -> PathBuf {
        let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("video_sync_merger");

        fs::create_dir_all(&config_dir).ok();
        config_dir.join("settings.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(contents) = fs::read_to_string(&path) {
                if let Ok(config) = serde_json::from_str(&contents) {
                    return config;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            fs::write(path, json).ok();
        }
    }
}
