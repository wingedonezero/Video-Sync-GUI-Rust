//! Media types — 1:1 port of `vsg_core/models/media.py`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::enums::TrackType;

/// Stream properties from mkvmerge probe — `StreamProps`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamProps {
    pub codec_id: String,
    #[serde(default = "default_und")]
    pub lang: String,
    #[serde(default)]
    pub name: String,
}

fn default_und() -> String {
    "und".to_string()
}

impl StreamProps {
    pub fn new(codec_id: impl Into<String>) -> Self {
        Self {
            codec_id: codec_id.into(),
            lang: "und".to_string(),
            name: String::new(),
        }
    }
}

/// A media track within a source file — `Track`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    /// Source role string, e.g. "Source 1", "Source 2"
    pub source: String,
    /// mkvmerge track ID (per container)
    pub id: i32,
    /// Track type (video/audio/subtitles)
    #[serde(rename = "type")]
    pub track_type: TrackType,
    /// Stream properties (codec, language, name)
    pub props: StreamProps,
}

/// An attachment (font, image, etc.) within a container — `Attachment`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub id: i32,
    pub file_name: String,
    #[serde(default)]
    pub out_path: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_serializes() {
        let track = Track {
            source: "Source 1".to_string(),
            id: 0,
            track_type: TrackType::Video,
            props: StreamProps::new("V_MPEG4/ISO/AVC"),
        };
        let json = serde_json::to_string(&track).unwrap();
        assert!(json.contains("\"Source 1\""));
        assert!(json.contains("\"video\""));
    }

    #[test]
    fn stream_props_defaults() {
        let props = StreamProps::new("A_AAC");
        assert_eq!(props.lang, "und");
        assert_eq!(props.name, "");
    }
}
