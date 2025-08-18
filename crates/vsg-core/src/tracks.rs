use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use std::process::Command;
use crate::process::{which_tool, run_capture};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackMeta {
    pub id: u32,                 // mkvmerge "id"
    pub codec: String,           // codec string (e.g. "AVC/H.264")
    pub lang: Option<String>,
    pub kind: String,            // "video" | "audio" | "subtitles" | other
    pub name: Option<String>,
    pub default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub tracks: Vec<TrackMeta>,
}

/// Subset of mkvmerge -J we consume
#[derive(Debug, Deserialize)]
struct MkvmergeJson {
    tracks: Vec<MkvTrack>,
}

#[derive(Debug, Deserialize)]
struct MkvTrack {
    id: u32,
    r#type: String,
    properties: Option<MkvProps>,
    codec: Option<String>,
    codec_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MkvProps {
    language: Option<String>,
    default_track: Option<bool>,
    track_name: Option<String>,
}

/// Call `mkvmerge -J <file>` and map to our ProbeResult.
pub fn probe_streams(mkvmerge_path: &str, input_path: &str) -> Result<ProbeResult> {
    let mkvmerge = if mkvmerge_path.is_empty() {
        which_tool("mkvmerge")?
    } else {
        mkvmerge_path.to_string()
    };

    let out = run_capture(Command::new(&mkvmerge).arg("-J").arg(input_path))
        .with_context(|| format!("Failed to run '{} -J {}'", mkvmerge, input_path))?;

    let parsed: MkvmergeJson = serde_json::from_str(&out)
        .with_context(|| "Failed to parse mkvmerge -J JSON")?;

    let mut tracks = Vec::new();
    for t in parsed.tracks {
        let props = t.properties.unwrap_or(MkvProps {
            language: None,
            default_track: None,
            track_name: None,
        });
        let codec = t.codec.or(t.codec_id).unwrap_or_default();
        let kind = t.r#type;
        tracks.push(TrackMeta {
            id: t.id,
            codec,
            lang: props.language,
            kind,
            name: props.track_name,
            default: props.default_track.unwrap_or(false),
        });
    }
    Ok(ProbeResult { tracks })
}
