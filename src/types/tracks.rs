// src/types/tracks.rs
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackDialogInfo {
    pub source: String,        // "REF" | "SEC" | "TER"
    pub original_path: String, // source file path
    pub id: i64,               // mkvmerge JSON track id
    pub r#type: String,        // "video" | "audio" | "subtitles"
    pub codec_id: String,
    pub lang: String,          // "und" default
    pub name: String,          // may be ""
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtractedTrack {
    pub id: i64,
    pub r#type: String,   // "video" | "audio" | "subtitles"
    pub lang: String,     // "und"
    pub name: String,     // ""
    pub path: String,     // output file we extracted to
    pub codec_id: String, // source codec id
    pub source: String,   // "REF" | "SEC" | "TER"
}
