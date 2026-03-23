//! Signature generation — 1:1 port of `vsg_core/job_layouts/signature.py`.
//!
//! Generates robust signatures for track matching and layout compatibility.
//! Handles duplicate tracks (e.g., PGS) by including their position.

use std::collections::HashMap;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// Generates robust signatures for track matching and layout compatibility — `EnhancedSignatureGenerator`.
pub struct EnhancedSignatureGenerator;

impl EnhancedSignatureGenerator {
    /// Generates a basic signature for a set of tracks.
    ///
    /// `track_info` maps source names to their track lists.
    /// If `strict`, includes codec and language for a stricter match.
    pub fn generate_track_signature(
        track_info: &HashMap<String, Vec<Value>>,
        strict: bool,
    ) -> Value {
        let mut signature: HashMap<String, i64> = HashMap::new();

        if !strict {
            // Non-strict: counts tracks by source and type
            for tracks in track_info.values() {
                for track in tracks {
                    let source = track
                        .get("source")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let track_type = track
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let key = format!("{source}_{track_type}");
                    *signature.entry(key).or_insert(0) += 1;
                }
            }
        } else {
            // Strict: includes codec, language, and position
            let mut type_counters: HashMap<String, i64> = HashMap::new();
            for tracks in track_info.values() {
                for track in tracks {
                    let track_type = track
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let position = type_counters.entry(track_type.to_string()).or_insert(0);

                    let source = track
                        .get("source")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let codec = track
                        .get("codec_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_lowercase();
                    let lang = track
                        .get("lang")
                        .and_then(|v| v.as_str())
                        .unwrap_or("und")
                        .to_lowercase();

                    let sig_item =
                        format!("{source}_{track_type}_{codec}_{lang}_{position}");
                    *signature.entry(sig_item).or_insert(0) += 1;
                    *position += 1;
                }
            }
        }

        let total: i64 = signature.values().sum();
        json!({
            "signature": signature,
            "strict": strict,
            "total_tracks": total,
        })
    }

    /// Generates a detailed, order-sensitive signature of the file structure.
    ///
    /// Used for exact compatibility checking. Includes track IDs to prevent
    /// layouts from being applied to files where tracks are in different orders.
    pub fn generate_structure_signature(
        track_info: &HashMap<String, Vec<Value>>,
    ) -> Value {
        let mut structure: serde_json::Map<String, Value> = serde_json::Map::new();

        let mut sorted_keys: Vec<&String> = track_info.keys().collect();
        sorted_keys.sort();

        for source_key in sorted_keys {
            let tracks = &track_info[source_key];
            let mut source_structure: HashMap<&str, Vec<Value>> = HashMap::new();
            source_structure.insert("video", Vec::new());
            source_structure.insert("audio", Vec::new());
            source_structure.insert("subtitles", Vec::new());

            for track in tracks {
                let track_type = track
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if let Some(list) = source_structure.get_mut(track_type) {
                    list.push(json!({
                        "id": track.get("id"),
                        "codec_id": track.get("codec_id").and_then(|v| v.as_str()).unwrap_or(""),
                        "lang": track.get("lang").and_then(|v| v.as_str()).unwrap_or("und"),
                    }));
                }
            }

            structure.insert(
                source_key.clone(),
                serde_json::to_value(&source_structure).unwrap_or_default(),
            );
        }

        let structure_json =
            serde_json::to_string(&structure).unwrap_or_default();
        let hash = format!("{:x}", Sha256::digest(structure_json.as_bytes()));

        json!({
            "structure": structure,
            "hash": hash,
        })
    }

    /// Compares two structure signatures for exact compatibility.
    pub fn structures_are_compatible(struct1: &Value, struct2: &Value) -> bool {
        let hash1 = struct1.get("hash").and_then(|v| v.as_str());
        let hash2 = struct2.get("hash").and_then(|v| v.as_str());
        match (hash1, hash2) {
            (Some(h1), Some(h2)) if !h1.is_empty() => h1 == h2,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compatible_structures() {
        let mut info = HashMap::new();
        info.insert(
            "Source 1".to_string(),
            vec![json!({"type": "video", "id": 0, "codec_id": "V_MPEG4", "lang": "und"})],
        );

        let sig1 = EnhancedSignatureGenerator::generate_structure_signature(&info);
        let sig2 = EnhancedSignatureGenerator::generate_structure_signature(&info);
        assert!(EnhancedSignatureGenerator::structures_are_compatible(
            &sig1, &sig2
        ));
    }

    #[test]
    fn test_incompatible_structures() {
        let mut info1 = HashMap::new();
        info1.insert(
            "Source 1".to_string(),
            vec![json!({"type": "video", "id": 0, "codec_id": "V_MPEG4", "lang": "und"})],
        );

        let mut info2 = HashMap::new();
        info2.insert(
            "Source 1".to_string(),
            vec![json!({"type": "audio", "id": 0, "codec_id": "A_AAC", "lang": "eng"})],
        );

        let sig1 = EnhancedSignatureGenerator::generate_structure_signature(&info1);
        let sig2 = EnhancedSignatureGenerator::generate_structure_signature(&info2);
        assert!(!EnhancedSignatureGenerator::structures_are_compatible(
            &sig1, &sig2
        ));
    }
}
