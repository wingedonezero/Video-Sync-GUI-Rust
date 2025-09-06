// src/core/mkv_utils/streams.rs

use crate::core::command_runner::CommandRunner;
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackDialogInfo {
    pub source: String,         // "REF" | "SEC" | "TER"
    pub original_path: String,  // file path
    pub id: i64,                // mkvmerge track id
    #[serde(rename = "type")]
    pub ttype: String,          // "video"|"audio"|"subtitles"
    pub codec_id: String,
    pub lang: String,           // default "und"
    pub name: String,           // track_name
}

/// Raw mkvmerge -J response structures (partial).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MkvmergeInfo {
    tracks: Option<Vec<MkvTrack>>,
    attachments: Option<Vec<MkvAttachment>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MkvTrack {
    id: i64,
    #[serde(rename = "type")]
    ttype: String,
    properties: Option<MkvTrackProps>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MkvTrackProps {
    codec_id: Option<String>,
    language: Option<String>,
    track_name: Option<String>,
    audio_bits_per_sample: Option<i64>,
    bit_depth: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MkvAttachment {
    pub id: i64,
    pub file_name: String,
}

pub fn get_stream_info(mkv_path: &str, runner: &CommandRunner) -> Option<Value> {
    let out = runner.run(&["mkvmerge", "-J", mkv_path])?;
    serde_json::from_str::<Value>(&out).ok()
}

pub fn get_track_info_for_dialog(
    ref_file: &str,
    sec_file: Option<&str>,
    ter_file: Option<&str>,
    runner: &CommandRunner,
) -> HashMap<String, Vec<TrackDialogInfo>> {
    let mut all: HashMap<String, Vec<TrackDialogInfo>> =
    HashMap::from([("REF".into(), vec![]), ("SEC".into(), vec![]), ("TER".into(), vec![])]);

    for (source, path_opt) in [("REF", Some(ref_file)), ("SEC", sec_file), ("TER", ter_file)] {
        let Some(path) = path_opt else { continue };
        if !Path::new(path).exists() { continue; }
        let info_val = match get_stream_info(path, runner) { Some(v) => v, None => continue };
        let info: MkvmergeInfo = serde_json::from_value(info_val).unwrap_or(MkvmergeInfo{tracks:None,attachments:None});
        if let Some(tracks) = info.tracks {
            for t in tracks {
                let props = t.properties.unwrap_or(MkvTrackProps{
                    codec_id: None, language: None, track_name: None, audio_bits_per_sample: None, bit_depth: None
                });
                all.get_mut(source).unwrap().push(TrackDialogInfo{
                    source: source.to_string(),
                                                  original_path: path.to_string(),
                                                  id: t.id,
                                                  ttype: t.ttype,
                                                  codec_id: props.codec_id.unwrap_or_else(|| "N/A".into()),
                                                  lang: props.language.unwrap_or_else(|| "und".into()),
                                                  name: props.track_name.unwrap_or_default(),
                });
            }
        }
    }
    all
}

/// Convenience accessors for attachments (used by attachments.rs)
pub fn list_attachments(mkv_path: &str, runner: &CommandRunner) -> Vec<MkvAttachment> {
    let out = match runner.run(&["mkvmerge", "-J", mkv_path]) {
        Some(s) => s,
        None => return vec![],
    };
    let info: MkvmergeInfo = serde_json::from_str(&out).unwrap_or(MkvmergeInfo {
        tracks: None,
        attachments: None,
    });
    info.attachments.unwrap_or_default()
}
