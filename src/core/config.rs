// src/core/config.rs
//
// Rust port of AppConfig:
// - settings.json lives next to the executable (like Python's script_dir)
// - default values carried over verbatim
// - load() fills missing keys with defaults, ignores unknown keys
// - save() persists pretty-printed JSON
// - ensure_dirs_exist() creates output_folder and temp_root

use serde_json::{json, Map as JsonMap, Value};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub struct AppConfig {
    pub script_dir: PathBuf,
    pub settings_path: PathBuf,
    pub defaults: JsonMap<String, Value>,
    pub settings: JsonMap<String, Value>,
}

impl AppConfig {
    pub fn new(settings_filename: &str) -> Self {
        let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let settings_path = exe_dir.join(settings_filename);

        let defaults = AppConfig::default_map();
        let mut this = Self {
            script_dir: exe_dir,
            settings_path,
            defaults: defaults.clone(),
                settings: defaults,
        };
        this.load();
        this.ensure_dirs_exist();
        this
    }

    fn default_map() -> JsonMap<String, Value> {
        let mut m = JsonMap::new();
        m.insert("last_ref_path".into(), json!(""));
        m.insert("last_sec_path".into(), json!(""));
        m.insert("last_ter_path".into(), json!(""));
        m.insert(
            "output_folder".into(),
                 json!(Path::new(".").join("sync_output").to_string_lossy().to_string()),
        );
        m.insert(
            "temp_root".into(),
                 json!(Path::new(".").join("temp_work").to_string_lossy().to_string()),
        );
        m.insert("videodiff_path".into(), json!(""));
        m.insert("analysis_mode".into(), json!("Audio Correlation"));
        m.insert("analysis_lang_ref".into(), json!(""));
        m.insert("analysis_lang_sec".into(), json!(""));
        m.insert("analysis_lang_ter".into(), json!(""));
        m.insert("scan_chunk_count".into(), json!(10));
        m.insert("scan_chunk_duration".into(), json!(15));
        m.insert("min_match_pct".into(), json!(5.0));
        m.insert("videodiff_error_min".into(), json!(0.0));
        m.insert("videodiff_error_max".into(), json!(100.0));
        m.insert("rename_chapters".into(), json!(false));
        m.insert("apply_dialog_norm_gain".into(), json!(false));
        m.insert("snap_chapters".into(), json!(false));
        m.insert("snap_mode".into(), json!("previous"));
        m.insert("snap_threshold_ms".into(), json!(250));
        m.insert("snap_starts_only".into(), json!(true));
        m.insert("log_compact".into(), json!(true));
        m.insert("log_autoscroll".into(), json!(true)); // GUI concern; kept for parity
        m.insert("log_error_tail".into(), json!(20));
        m.insert("log_tail_lines".into(), json!(0));
        m.insert("log_progress_step".into(), json!(20));
        m.insert("log_show_options_pretty".into(), json!(false));
        m.insert("log_show_options_json".into(), json!(false));
        m.insert("disable_track_statistics_tags".into(), json!(false));
        m.insert("archive_logs".into(), json!(true));
        m.insert("auto_apply_strict".into(), json!(false));
        m
    }

    pub fn load(&mut self) {
        let mut changed = false;
        if self.settings_path.exists() {
            let mut buf = String::new();
            match fs::File::open(&self.settings_path).and_then(|mut f| f.read_to_string(&mut buf)) {
                Ok(_) => {
                    if let Ok(mut loaded) = serde_json::from_str::<JsonMap<String, Value>>(&buf) {
                        // add missing defaults, ignore unknowns
                        for (k, vdef) in self.defaults.iter() {
                            if !loaded.contains_key(k) {
                                loaded.insert(k.clone(), vdef.clone());
                                changed = true;
                            }
                        }
                        self.settings = loaded;
                    } else {
                        self.settings = self.defaults.clone();
                        changed = true;
                    }
                }
                Err(_) => {
                    self.settings = self.defaults.clone();
                    changed = true;
                }
            }
        } else {
            self.settings = self.defaults.clone();
            changed = true;
        }

        if changed {
            let _ = self.save();
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let mut f = fs::File::create(&self.settings_path)?;
        let s = serde_json::to_string_pretty(&self.settings).unwrap_or_else(|_| "{}".into());
        f.write_all(s.as_bytes())
    }

    pub fn get<'a>(&'a self, key: &str) -> Option<&'a Value> {
        self.settings.get(key)
    }

    pub fn set<V: Into<Value>>(&mut self, key: &str, value: V) {
        self.settings.insert(key.into(), value.into());
    }

    pub fn ensure_dirs_exist(&self) {
        if let Some(Value::String(out)) = self.settings.get("output_folder") {
            let _ = fs::create_dir_all(out);
        }
        if let Some(Value::String(tmp)) = self.settings.get("temp_root") {
            let _ = fs::create_dir_all(tmp);
        }
    }
}
