
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiConfig {
    pub ref_path: String,
    pub sec_path: String,
    pub ter_path: String,
    pub work_dir: String,
    pub out_dir: String,
    pub chunks: usize,
    pub chunk_dur: f64,
    pub sample_rate: String, // "s12000" | "s24000" | "s48000"
    pub stereo_mode: String, // "mono" | "left" | "right" | "mid" | "best"
    pub method: String,      // "fft" | "compat"
    pub band: String,        // "none" | "voice"
}

impl Default for GuiConfig {
    fn default() -> Self {
        GuiConfig {
            ref_path: String::new(),
            sec_path: String::new(),
            ter_path: String::new(),
            work_dir: String::new(),
            out_dir: String::new(),
            chunks: 10,
            chunk_dur: 8.0,
            sample_rate: "s24000".into(),
            stereo_mode: "best".into(),
            method: "fft".into(),
            band: "none".into(),
        }
    }
}

pub fn settings_path() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    exe.parent().unwrap_or(&PathBuf::from(".")).join("vsg_settings.json")
}

pub fn load() -> GuiConfig {
    let p = settings_path();
    if let Ok(txt) = fs::read_to_string(&p) {
        if let Ok(cfg) = serde_json::from_str::<GuiConfig>(&txt) {
            return cfg;
        }
    }
    GuiConfig::default()
}

pub fn save(cfg: &GuiConfig) -> std::io::Result<()> {
    let p = settings_path();
    fs::write(p, serde_json::to_string_pretty(cfg).unwrap())
}
