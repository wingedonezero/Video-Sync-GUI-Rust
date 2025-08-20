use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExtractionStrategy {
    /// Reuse extracted audio if present; else extract; then analyze.
    Auto,
    /// Always extract fresh; then analyze.
    ForceExtract,
    /// Fail if extracted audio isn't present; do not extract.
    ReuseOnly,
    /// Skip extraction, decode directly from MKV inputs.
    DecodeDirect,
}

impl Default for ExtractionStrategy {
    fn default() -> Self { ExtractionStrategy::Auto }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub ref_path: String,
    pub sec_path: String,
    pub ter_path: String,
    pub work_dir: String,
    pub out_dir: String,
    pub chunks: usize,
    pub chunk_dur_s: f64,
    pub sample_rate: String, // s12000|s24000|s48000
    pub stereo_mode: String, // mono|left|right|mid|best
    pub method: String,      // fft|compat
    pub band: String,        // none|voice
    pub strategy: ExtractionStrategy,
    pub keep_temp: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ref_path: String::new(),
            sec_path: String::new(),
            ter_path: String::new(),
            work_dir: String::new(),
            out_dir: String::new(),
            chunks: 10,
            chunk_dur_s: 8.0,
            sample_rate: "s24000".into(),
            stereo_mode: "best".into(),
            method: "fft".into(),
            band: "none".into(),
            strategy: ExtractionStrategy::Auto,
            keep_temp: false,
        }
    }
}
