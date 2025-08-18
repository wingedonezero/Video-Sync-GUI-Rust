use serde::{Deserialize, Serialize};
use camino::Utf8PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub analysis_mode: String,         // "Audio Correlation" | "VideoDiff"
    pub scan_chunk_count: u32,
    pub scan_chunk_duration: u32,      // seconds
    pub min_match_pct: f32,
    pub videodiff_error_min: f32,
    pub videodiff_error_max: f32,
    pub temp_root: Utf8PathBuf,
    pub ffmpeg_path: Option<Utf8PathBuf>,
    pub ffprobe_path: Option<Utf8PathBuf>,
    pub mkvmerge_path: Option<Utf8PathBuf>,
    pub mkvextract_path: Option<Utf8PathBuf>,
    pub videodiff_path: Option<Utf8PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            analysis_mode: "Audio Correlation".into(),
            scan_chunk_count: 10,
            scan_chunk_duration: 15,
            min_match_pct: 5.0,
            videodiff_error_min: 0.0,
            videodiff_error_max: 100.0,
            temp_root: Utf8PathBuf::from("./.tmp"),
            ffmpeg_path: None,
            ffprobe_path: None,
            mkvmerge_path: None,
            mkvextract_path: None,
            videodiff_path: None,
        }
    }
}
