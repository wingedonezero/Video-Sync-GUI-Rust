//! Application configuration — 1:1 port of `vsg_core/config.py`.
//!
//! Manages persistent user settings stored in settings.toml.
//! All defaults come from `AppSettings::default()`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::settings::{AppSettings, PATH_SENTINEL};

/// Application configuration manager — `AppConfig`
///
/// Handles loading/saving settings to TOML, runtime path resolution,
/// legacy key migration, and directory creation.
pub struct AppConfig {
    /// Application root directory
    pub script_dir: PathBuf,
    /// Path to the settings file
    pub settings_path: PathBuf,
    /// Current settings
    pub settings: AppSettings,
    /// Track accessed keys for typo detection — 1:1 with Python `_accessed_keys`
    accessed_keys: std::collections::HashSet<String>,
}

impl AppConfig {
    /// Create a new AppConfig, loading settings from disk.
    pub fn new(script_dir: impl Into<PathBuf>) -> Result<Self, ConfigError> {
        Self::with_filename(script_dir, "settings.toml")
    }

    /// Create a new AppConfig with a custom settings filename.
    pub fn with_filename(
        script_dir: impl Into<PathBuf>,
        filename: &str,
    ) -> Result<Self, ConfigError> {
        let script_dir = script_dir.into();
        let settings_path = script_dir.join(filename);

        let mut config = Self {
            script_dir,
            settings_path,
            settings: AppSettings::default(),
            accessed_keys: std::collections::HashSet::new(),
        };

        config.load()?;
        config.resolve_path_sentinels();
        config.ensure_dirs_exist();

        Ok(config)
    }

    /// Load settings from TOML file, applying migrations and defaults.
    fn load(&mut self) -> Result<(), ConfigError> {
        if !self.settings_path.exists() {
            // No file — use defaults, save them
            self.settings = AppSettings::default();
            self.save()?;
            return Ok(());
        }

        let contents = fs::read_to_string(&self.settings_path)
            .map_err(|e| ConfigError::Io(e.to_string()))?;

        // Try parsing as TOML first (fast path for valid files)
        match toml::from_str::<AppSettings>(&contents) {
            Ok(settings) => {
                self.settings = settings;
            }
            Err(_toml_err) => {
                // Try JSON (for migration from Python version)
                match serde_json::from_str::<AppSettings>(&contents) {
                    Ok(settings) => {
                        self.settings = settings;
                        // Re-save as TOML
                        self.save()?;
                    }
                    Err(_json_err) => {
                        // Both direct parsing failed — try field-by-field recovery.
                        // 1:1 port of Python's field-by-field recovery path.
                        tracing::warn!(
                            "Direct parse failed for {}, attempting field-by-field recovery",
                            self.settings_path.display()
                        );

                        // Parse as raw key-value map
                        let raw_map: Option<HashMap<String, serde_json::Value>> =
                            toml::from_str::<HashMap<String, serde_json::Value>>(&contents)
                                .ok()
                                .or_else(|| serde_json::from_str(&contents).ok());

                        if let Some(raw) = raw_map {
                            // Start from defaults, then try each field individually
                            self.settings = AppSettings::default();
                            let mut rejected = Vec::new();

                            for (key, value) in &raw {
                                // Try setting this single field via JSON round-trip
                                let mut test_json = match serde_json::to_value(&self.settings) {
                                    Ok(serde_json::Value::Object(map)) => map,
                                    _ => continue,
                                };
                                test_json.insert(key.clone(), value.clone());

                                match serde_json::from_value::<AppSettings>(
                                    serde_json::Value::Object(test_json),
                                ) {
                                    Ok(new_settings) => {
                                        self.settings = new_settings;
                                    }
                                    Err(_) => {
                                        rejected.push(key.clone());
                                    }
                                }
                            }

                            if !rejected.is_empty() {
                                tracing::warn!(
                                    "Settings recovery: {} field(s) reset to defaults: {}",
                                    rejected.len(),
                                    rejected.join(", ")
                                );
                            }
                        } else {
                            // Even raw parsing failed — use full defaults
                            tracing::warn!(
                                "Failed to parse settings file at all, using defaults: {}",
                                self.settings_path.display()
                            );
                            self.settings = AppSettings::default();
                        }

                        self.save()?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Resolve PATH_SENTINEL values to actual paths based on script_dir.
    fn resolve_path_sentinels(&mut self) {
        if self.settings.output_folder == PATH_SENTINEL {
            self.settings.output_folder = self.script_dir.join("sync_output").to_string_lossy().to_string();
        }
        if self.settings.temp_root == PATH_SENTINEL {
            self.settings.temp_root = self.script_dir.join("temp_work").to_string_lossy().to_string();
        }
        if self.settings.logs_folder == PATH_SENTINEL {
            self.settings.logs_folder = self
                .script_dir
                .join(".config")
                .join("logs")
                .to_string_lossy()
                .to_string();
        }
        if self.settings.source_separation_model_dir == PATH_SENTINEL {
            self.settings.source_separation_model_dir = self
                .script_dir
                .join(".config")
                .join("audio_separator_models")
                .to_string_lossy()
                .to_string();
        }
    }

    /// Save current settings to TOML file.
    pub fn save(&self) -> Result<(), ConfigError> {
        let toml_str =
            toml::to_string_pretty(&self.settings).map_err(|e| ConfigError::Serialize(e.to_string()))?;

        // Ensure parent directory exists
        if let Some(parent) = self.settings_path.parent() {
            fs::create_dir_all(parent).map_err(|e| ConfigError::Io(e.to_string()))?;
        }

        fs::write(&self.settings_path, toml_str).map_err(|e| ConfigError::Io(e.to_string()))?;

        Ok(())
    }

    /// Get a setting value by field name — `get()`
    ///
    /// Tracks accessed keys for typo detection.
    pub fn get_str(&mut self, key: &str) -> Option<String> {
        self.accessed_keys.insert(key.to_string());
        let json = serde_json::to_value(&self.settings).ok()?;
        json.get(key).map(|v| match v {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
    }

    /// Get a setting as f64.
    pub fn get_f64(&mut self, key: &str) -> Option<f64> {
        self.accessed_keys.insert(key.to_string());
        let json = serde_json::to_value(&self.settings).ok()?;
        json.get(key).and_then(|v| v.as_f64())
    }

    /// Get a setting as i64.
    pub fn get_i64(&mut self, key: &str) -> Option<i64> {
        self.accessed_keys.insert(key.to_string());
        let json = serde_json::to_value(&self.settings).ok()?;
        json.get(key).and_then(|v| v.as_i64())
    }

    /// Get a setting as bool.
    pub fn get_bool(&mut self, key: &str) -> Option<bool> {
        self.accessed_keys.insert(key.to_string());
        let json = serde_json::to_value(&self.settings).ok()?;
        json.get(key).and_then(|v| v.as_bool())
    }

    /// Returns set of accessed keys that are not in field_names.
    /// 1:1 port of `get_unrecognized_keys()`.
    pub fn get_unrecognized_keys(&self) -> Vec<String> {
        let known_fields: std::collections::HashSet<&str> =
            AppSettings::field_names().iter().copied().collect();
        self.accessed_keys
            .iter()
            .filter(|k| !known_fields.contains(k.as_str()))
            .cloned()
            .collect()
    }

    /// Set a setting value by field name — `set()`
    ///
    /// Uses JSON round-trip for dynamic field access.
    /// Returns true if the value was set, false on error.
    pub fn set(&mut self, key: &str, value: serde_json::Value) -> bool {
        let mut json = match serde_json::to_value(&self.settings) {
            Ok(serde_json::Value::Object(map)) => map,
            _ => return false,
        };

        json.insert(key.to_string(), value);

        match serde_json::from_value::<AppSettings>(serde_json::Value::Object(json)) {
            Ok(new_settings) => {
                self.settings = new_settings;
                true
            }
            Err(_) => false,
        }
    }

    /// Ensure required directories exist — `ensure_dirs_exist()`
    pub fn ensure_dirs_exist(&self) {
        let dirs = [
            self.settings.output_folder.as_str(),
            self.settings.temp_root.as_str(),
            self.settings.logs_folder.as_str(),
        ];

        for dir in &dirs {
            if !dir.is_empty() && *dir != PATH_SENTINEL {
                let _ = fs::create_dir_all(dir);
            }
        }

        // .config directories
        let _ = fs::create_dir_all(self.get_config_dir());
        let _ = fs::create_dir_all(self.get_fonts_dir());
        let _ = fs::create_dir_all(self.get_ocr_config_dir());
    }

    /// Returns the .config directory path — `get_config_dir()`
    pub fn get_config_dir(&self) -> PathBuf {
        self.script_dir.join(".config")
    }

    /// Returns the fonts directory — `get_fonts_dir()`
    pub fn get_fonts_dir(&self) -> PathBuf {
        let custom = &self.settings.fonts_directory;
        if !custom.is_empty() {
            let p = PathBuf::from(custom);
            if p.exists() {
                return p;
            }
        }
        self.script_dir.join(".config").join("fonts")
    }

    /// Returns the OCR config directory — `get_ocr_config_dir()`
    pub fn get_ocr_config_dir(&self) -> PathBuf {
        self.get_config_dir().join("ocr")
    }

    /// Returns the default custom wordlist path — `get_default_wordlist_path()`
    pub fn get_default_wordlist_path(&self) -> PathBuf {
        self.get_ocr_config_dir().join("custom_wordlist.txt")
    }

    /// Returns the style editor temp directory — `get_style_editor_temp_dir()`
    pub fn get_style_editor_temp_dir(&self) -> PathBuf {
        let dir = PathBuf::from(&self.settings.temp_root).join("style_editor");
        let _ = fs::create_dir_all(&dir);
        dir
    }

    /// Returns the VapourSynth index directory — `get_vs_index_dir()`
    pub fn get_vs_index_dir(&self) -> PathBuf {
        let dir = PathBuf::from(&self.settings.temp_root).join("vs_indexes");
        let _ = fs::create_dir_all(&dir);
        dir
    }

    /// Clean up style editor temp files — `cleanup_style_editor_temp()`
    pub fn cleanup_style_editor_temp(&self) -> u32 {
        cleanup_dir_contents(&self.get_style_editor_temp_dir())
    }

    /// Clean up VapourSynth index directories — `cleanup_vs_indexes()`
    pub fn cleanup_vs_indexes(&self) -> u32 {
        cleanup_dir_contents(&self.get_vs_index_dir())
    }

    /// Clean up old files (> max_age_hours) in the style editor temp dir.
    /// 1:1 port of `cleanup_old_style_editor_temp()`.
    pub fn cleanup_old_style_editor_temp(&self, max_age_hours: f64) -> u32 {
        cleanup_old_dir_contents(&self.get_style_editor_temp_dir(), max_age_hours)
    }

    /// Get a unique index directory for a specific video file.
    /// Uses MD5 hash of the video path (matches Python's hashlib.md5).
    /// 1:1 port of `get_vs_index_for_video()`.
    pub fn get_vs_index_for_video(&self, video_path: &str) -> PathBuf {
        use md5::{Digest, Md5};

        let mut hasher = Md5::new();
        hasher.update(video_path.as_bytes());
        let hash = hasher.finalize();
        let hash_hex = format!("{:x}", hash);
        let short_hash = &hash_hex[..16]; // First 16 chars, same as Python [:16]

        let index_dir = self.get_vs_index_dir().join(short_hash);
        let _ = fs::create_dir_all(&index_dir);
        index_dir
    }

    /// Get orphaned keys from the current settings file on disk.
    /// 1:1 port of `get_orphaned_keys()`.
    pub fn get_orphaned_keys(&self) -> Vec<String> {
        if !self.settings_path.exists() {
            return Vec::new();
        }

        let contents = match fs::read_to_string(&self.settings_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        // Try TOML first, then JSON
        let on_disk_keys: Vec<String> =
            if let Ok(table) = contents.parse::<toml::Table>() {
                table.keys().cloned().collect()
            } else if let Ok(map) =
                serde_json::from_str::<HashMap<String, serde_json::Value>>(&contents)
            {
                map.keys().cloned().collect()
            } else {
                return Vec::new();
            };

        let known_fields: std::collections::HashSet<&str> =
            AppSettings::field_names().iter().copied().collect();

        on_disk_keys
            .into_iter()
            .filter(|k| !known_fields.contains(k.as_str()))
            .collect()
    }

    /// Remove orphaned keys from the settings file and re-save.
    /// 1:1 port of `remove_orphaned_keys()`.
    pub fn remove_orphaned_keys(&self) -> Vec<String> {
        let orphaned = self.get_orphaned_keys();
        if !orphaned.is_empty() {
            // Re-save from model (excludes unknown keys automatically)
            let _ = self.save();
        }
        orphaned
    }

    /// Validate that AppSettings fields match what we expect.
    /// 1:1 port of `validate_schema()`.
    pub fn validate_schema(&self) -> Vec<String> {
        let mut warnings = Vec::new();

        let settings_fields: std::collections::HashSet<&str> =
            AppSettings::field_names().iter().copied().collect();

        // Check if serialized settings have unexpected keys
        if let Ok(json) = serde_json::to_value(&self.settings) {
            if let Some(obj) = json.as_object() {
                for key in obj.keys() {
                    if !settings_fields.contains(key.as_str()) {
                        warnings.push(format!(
                            "Serialized settings has key not in field_names: {key}"
                        ));
                    }
                }
            }
        }

        warnings
    }

    /// Get orphaned keys from a JSON settings file (for migration detection).
    pub fn get_orphaned_keys_from_json(&self, json_path: &Path) -> Vec<String> {
        let contents = match fs::read_to_string(json_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let on_disk: HashMap<String, serde_json::Value> = match serde_json::from_str(&contents) {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };

        let known_fields: std::collections::HashSet<&str> =
            AppSettings::field_names().iter().copied().collect();

        on_disk
            .keys()
            .filter(|k| !known_fields.contains(k.as_str()))
            .cloned()
            .collect()
    }
}

/// Remove all contents of a directory, returning count of items removed.
fn cleanup_dir_contents(dir: &Path) -> u32 {
    if !dir.exists() {
        return 0;
    }
    let mut count = 0u32;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let result = if path.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };
            if result.is_ok() {
                count += 1;
            }
        }
    }
    count
}

/// Remove items older than `max_age_hours` from a directory.
/// 1:1 port of `cleanup_old_style_editor_temp_files()`.
fn cleanup_old_dir_contents(dir: &Path, max_age_hours: f64) -> u32 {
    if !dir.exists() {
        return 0;
    }

    let max_age_secs = max_age_hours * 3600.0;
    let now = std::time::SystemTime::now();
    let mut count = 0u32;

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_old = path
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|mtime| now.duration_since(mtime).ok())
                .is_some_and(|age| age.as_secs_f64() > max_age_secs);

            if is_old {
                let result = if path.is_dir() {
                    fs::remove_dir_all(&path)
                } else {
                    fs::remove_file(&path)
                };
                if result.is_ok() {
                    count += 1;
                }
            }
        }
    }
    count
}

/// Standalone path helpers (mirrors Python module-level functions)
pub fn get_config_dir_path(script_dir: &Path) -> PathBuf {
    script_dir.join(".config")
}

pub fn get_fonts_dir_path(script_dir: &Path, fonts_directory_setting: &str) -> PathBuf {
    if !fonts_directory_setting.is_empty() {
        let p = PathBuf::from(fonts_directory_setting);
        if p.exists() {
            return p;
        }
    }
    script_dir.join(".config").join("fonts")
}

/// Configuration errors.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("I/O error: {0}")]
    Io(String),
    #[error("Serialization error: {0}")]
    Serialize(String),
    #[error("Parse error: {0}")]
    Parse(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_creates_with_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config = AppConfig::new(dir.path()).unwrap();

        // Path sentinels should be resolved
        assert!(config.settings.output_folder.contains("sync_output"));
        assert!(config.settings.temp_root.contains("temp_work"));

        // Settings file should exist
        assert!(config.settings_path.exists());
    }

    #[test]
    fn config_saves_and_loads_toml() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = AppConfig::new(dir.path()).unwrap();

        // Change a setting
        config.settings.min_match_pct = 42.0;
        config.save().unwrap();

        // Reload
        let config2 = AppConfig::new(dir.path()).unwrap();
        assert_eq!(config2.settings.min_match_pct, 42.0);
    }

    #[test]
    fn config_reads_json_and_converts() {
        let dir = tempfile::tempdir().unwrap();
        let json_content = r#"{"min_match_pct": 55.0, "log_compact": false}"#;
        fs::write(dir.path().join("settings.toml"), json_content).unwrap();

        let config = AppConfig::new(dir.path()).unwrap();
        assert_eq!(config.settings.min_match_pct, 55.0);
        assert!(!config.settings.log_compact);

        // File should now be TOML
        let contents = fs::read_to_string(&config.settings_path).unwrap();
        assert!(contents.contains("min_match_pct"));
    }

    #[test]
    fn config_dynamic_get_set() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = AppConfig::new(dir.path()).unwrap();

        assert_eq!(config.get_f64("min_match_pct"), Some(10.0));

        config.set("min_match_pct", serde_json::json!(99.0));
        assert_eq!(config.get_f64("min_match_pct"), Some(99.0));

        assert_eq!(config.get_bool("log_compact"), Some(true));
    }

    #[test]
    fn config_ensures_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let config = AppConfig::new(dir.path()).unwrap();

        assert!(PathBuf::from(&config.settings.output_folder).exists());
        assert!(PathBuf::from(&config.settings.temp_root).exists());
        assert!(config.get_config_dir().exists());
    }
}
