//! Manual selection logic — 1:1 port of `vsg_qt/manual_selection_dialog/logic.py`.
//!
//! Manages track layout creation, pool-matching prepopulate, default/forced
//! normalization, and conversion between dialog format and ManualLayoutItem.

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// ManualSelectionLogic QObject.
        #[qobject]
        #[qml_element]
        #[qproperty(i32, layout_track_count)]
        #[qproperty(bool, has_external_subtitles)]
        #[qproperty(QString, info_text)]
        type ManualSelectionLogic = super::ManualSelectionLogicRust;

        /// Initialize with track info and optional previous layout (JSON).
        #[qinvokable]
        fn initialize(
            self: Pin<&mut ManualSelectionLogic>,
            track_info_json: QString,
            previous_layout_json: QString,
            previous_attachments_json: QString,
            previous_source_settings_json: QString,
        );

        /// Add a track to the layout by source key and track index.
        #[qinvokable]
        fn add_track_to_layout(
            self: Pin<&mut ManualSelectionLogic>,
            source_key: QString,
            track_index: i32,
        );

        /// Remove a track from the layout at the given index.
        #[qinvokable]
        fn remove_track_from_layout(self: Pin<&mut ManualSelectionLogic>, index: i32);

        /// Move a layout track from one index to another.
        #[qinvokable]
        fn move_track(self: Pin<&mut ManualSelectionLogic>, from_index: i32, to_index: i32);

        /// Get the final layout (with normalization applied), attachment sources,
        /// and source settings as JSON.
        #[qinvokable]
        fn get_result(self: Pin<&mut ManualSelectionLogic>) -> QString;

        /// Get layout track data at index as JSON (for display).
        #[qinvokable]
        fn get_layout_track(self: Pin<&mut ManualSelectionLogic>, index: i32) -> QString;

        /// Get the available tracks for a source as JSON array.
        #[qinvokable]
        fn get_source_tracks(
            self: Pin<&mut ManualSelectionLogic>,
            source_key: QString,
        ) -> QString;

        /// Get all source keys as JSON array (sorted numerically).
        #[qinvokable]
        fn get_source_keys(self: Pin<&mut ManualSelectionLogic>) -> QString;

        /// Toggle attachment source selection.
        #[qinvokable]
        fn toggle_attachment_source(
            self: Pin<&mut ManualSelectionLogic>,
            source_key: QString,
        );

        /// Set attachment source as checked (for restoring previous state).
        #[qinvokable]
        fn set_attachment_source_checked(
            self: Pin<&mut ManualSelectionLogic>,
            source_key: QString,
            checked: bool,
        );

        /// Get current attachment sources as JSON array.
        #[qinvokable]
        fn get_attachment_sources(self: Pin<&mut ManualSelectionLogic>) -> QString;

        /// Update source settings for a source (JSON).
        #[qinvokable]
        fn set_source_settings(
            self: Pin<&mut ManualSelectionLogic>,
            source_key: QString,
            settings_json: QString,
        );

        /// Clear source settings for a source.
        #[qinvokable]
        fn clear_source_settings(
            self: Pin<&mut ManualSelectionLogic>,
            source_key: QString,
        );

        /// Get source settings for a source as JSON (empty object if none).
        #[qinvokable]
        fn get_source_settings_for(
            self: Pin<&mut ManualSelectionLogic>,
            source_key: QString,
        ) -> QString;

        /// Check if a source has non-default settings.
        #[qinvokable]
        fn has_source_settings(
            self: Pin<&mut ManualSelectionLogic>,
            source_key: QString,
        ) -> bool;

        /// Update a track's settings at the given layout index (from TrackSettingsDialog result).
        #[qinvokable]
        fn update_track_settings(
            self: Pin<&mut ManualSelectionLogic>,
            index: i32,
            settings_json: QString,
        );

        /// Get track badge text for a layout track at index.
        #[qinvokable]
        fn get_track_badges(self: Pin<&mut ManualSelectionLogic>, index: i32) -> QString;

        /// Get track summary text for a layout track at index.
        #[qinvokable]
        fn get_track_summary(self: Pin<&mut ManualSelectionLogic>, index: i32) -> QString;

        /// Set is_default flag for a track, enforcing single-default-per-type.
        #[qinvokable]
        fn set_track_default(
            self: Pin<&mut ManualSelectionLogic>,
            index: i32,
            is_default: bool,
        );

        /// Set is_forced flag for a track, enforcing single-forced for subtitles.
        #[qinvokable]
        fn set_track_forced(
            self: Pin<&mut ManualSelectionLogic>,
            index: i32,
            is_forced: bool,
        );

        /// Add external subtitle files (JSON array of file paths).
        /// Returns JSON array of added track data objects.
        #[qinvokable]
        fn add_external_subtitles(
            self: Pin<&mut ManualSelectionLogic>,
            paths_json: QString,
        ) -> QString;

        /// Check if a track is blocked video (non-Source-1 video).
        #[qinvokable]
        fn is_blocked_video_at(self: Pin<&mut ManualSelectionLogic>, source_key: QString, track_index: i32) -> bool;

        /// Signal: layout changed, UI needs refresh.
        #[qsignal]
        fn layout_changed(self: Pin<&mut ManualSelectionLogic>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

use core::pin::Pin;
use std::collections::HashMap;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

/// Track type constants for matching.
const TYPE_VIDEO: &str = "video";
const TYPE_AUDIO: &str = "audio";
const TYPE_SUBTITLES: &str = "subtitles";

#[derive(Default)]
pub struct ManualSelectionLogicRust {
    layout_track_count: i32,
    has_external_subtitles: bool,
    info_text: QString,
    track_info: HashMap<String, Vec<serde_json::Value>>,
    layout_tracks: Vec<serde_json::Value>,
    attachment_sources: Vec<String>,
    source_settings: HashMap<String, serde_json::Value>,
}

// ── Helper functions (not methods, to avoid Pin borrowing issues) ──

fn is_blocked_video(track: &serde_json::Value) -> bool {
    let ttype = track.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let source = track.get("source").and_then(|v| v.as_str()).unwrap_or("");
    ttype == TYPE_VIDEO && source != "Source 1"
}

fn track_type_str(track: &serde_json::Value) -> &str {
    track.get("type").and_then(|v| v.as_str()).unwrap_or("")
}

fn track_source_str(track: &serde_json::Value) -> &str {
    track.get("source").and_then(|v| v.as_str()).unwrap_or("")
}

fn track_bool(track: &serde_json::Value, key: &str) -> bool {
    track.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}

fn track_str<'a>(track: &'a serde_json::Value, key: &str) -> &'a str {
    track.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

fn track_i64(track: &serde_json::Value, key: &str) -> Option<i64> {
    track.get(key).and_then(|v| v.as_i64())
}

/// Build badge text for a track — 1:1 port of track_widget/logic.py refresh_badges().
fn build_badges(track: &serde_json::Value) -> String {
    let mut badges = Vec::new();
    let ttype = track_type_str(track);

    if track_bool(track, "is_generated") {
        badges.push("GENERATED".to_string());
    }

    if track_bool(track, "is_default") {
        badges.push("DEFAULT".to_string());
    }

    if ttype == TYPE_SUBTITLES && track_bool(track, "is_forced_display") {
        badges.push("FORCED".to_string());
    }

    let custom_lang = track_str(track, "custom_lang");
    if !custom_lang.is_empty() {
        badges.push(format!("LANG:{custom_lang}"));
    }

    let custom_name = track_str(track, "custom_name");
    if !custom_name.is_empty() {
        let display = if custom_name.len() > 20 {
            format!("{}…", &custom_name[..20])
        } else {
            custom_name.to_string()
        };
        badges.push(format!("NAME:{display}"));
    }

    if ttype == TYPE_SUBTITLES {
        if track_bool(track, "perform_ocr") {
            badges.push("OCR".to_string());
        }
        if track_bool(track, "convert_to_ass") {
            badges.push("->ASS".to_string());
        }
        if track_bool(track, "rescale") {
            badges.push("RESCALE".to_string());
        }

        // Sync exclusion
        let has_excl = track
            .get("sync_exclusion_styles")
            .and_then(|v| v.as_array())
            .is_some_and(|a| !a.is_empty());
        if has_excl {
            badges.push("SYNC-EX".to_string());
        }

        // Style patch
        let has_patch = track
            .get("style_patch")
            .and_then(|v| v.as_object())
            .is_some_and(|o| !o.is_empty());
        if has_patch {
            badges.push("STYLED".to_string());
        }

        // Font replacements
        let font_count = track
            .get("font_replacements")
            .and_then(|v| v.as_object())
            .map(|o| o.len())
            .unwrap_or(0);
        if font_count > 0 {
            badges.push(format!("FONTS:{font_count}"));
        }

        // User modified path
        if !track_str(track, "user_modified_path").is_empty() {
            badges.push("EDITED".to_string());
        }
    }

    badges.join(" | ")
}

/// Build summary text for a track — 1:1 port of track_widget/logic.py refresh_summary().
fn build_summary(track: &serde_json::Value) -> String {
    let source = track_source_str(track);
    let ttype = track_type_str(track);
    let type_letter = match ttype {
        TYPE_VIDEO => "V",
        TYPE_AUDIO => "A",
        TYPE_SUBTITLES => "S",
        _ => "?",
    };
    let track_id = track_i64(track, "id").unwrap_or(0);
    let codec = track_str(track, "codec_id");
    let lang = track_str(track, "lang");
    let lang_display = if lang.is_empty() { "und" } else { lang };
    let description = track_str(track, "description");

    let desc = if description.is_empty() {
        // Strip MKV codec prefixes for cleaner display
        let clean_codec = codec
            .replace("A_", "")
            .replace("V_", "")
            .replace("S_TEXT/", "")
            .replace("S_HDMV/", "")
            .replace("S_VOBSUB", "VobSub");
        format!("{clean_codec} [{lang_display}]")
    } else {
        description.to_string()
    };

    let mut summary = format!("[{source}] [{type_letter}-{track_id}] {desc}");

    let custom_lang = track_str(track, "custom_lang");
    if !custom_lang.is_empty() {
        summary.push_str(&format!(" -> {custom_lang}"));
    }

    if track_bool(track, "is_generated") {
        if let Some(src_id) = track_i64(track, "source_track_id") {
            summary.push_str(&format!(" [Generated from {source} Track {src_id}]"));
        }
    }

    summary
}

/// Prepopulate layout using pool-matching algorithm.
/// 1:1 port of ManualLogic.prepopulate_from_layout().
fn prepopulate_with_pool_matching(
    track_info: &HashMap<String, Vec<serde_json::Value>>,
    previous_layout: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    if previous_layout.is_empty() {
        return Vec::new();
    }

    // Build pools keyed by (source, type, counter_index)
    let mut pools: HashMap<(String, String, usize), serde_json::Value> = HashMap::new();
    let mut build_counters: HashMap<(String, String), usize> = HashMap::new();

    for (src_key, track_list) in track_info {
        for t in track_list {
            let ttype = track_type_str(t).to_string();
            let counter_key = (src_key.clone(), ttype.clone());
            let idx = *build_counters.get(&counter_key).unwrap_or(&0);
            pools.insert((src_key.clone(), ttype.clone(), idx), t.clone());
            build_counters.insert(counter_key, idx + 1);
        }
    }

    let mut realized: Vec<serde_json::Value> = Vec::new();
    let mut match_counters: HashMap<(String, String), usize> = HashMap::new();

    for prev_item in previous_layout {
        let src = track_source_str(prev_item).to_string();
        let ttype = track_type_str(prev_item).to_string();

        // Generated tracks: validate source track exists, preserve from layout
        if track_bool(prev_item, "is_generated") {
            if let Some(source_track_id) = track_i64(prev_item, "source_track_id") {
                let source_exists = track_info
                    .get(&src)
                    .map(|tracks| {
                        tracks
                            .iter()
                            .any(|t| track_i64(t, "id") == Some(source_track_id))
                    })
                    .unwrap_or(false);
                if source_exists {
                    realized.push(prev_item.clone());
                }
            }
            continue;
        }

        // Normal tracks: pool-match by (source, type, counter)
        let counter_key = (src.clone(), ttype.clone());
        let idx = *match_counters.get(&counter_key).unwrap_or(&0);
        match_counters.insert(counter_key, idx + 1);

        let pool_key = (src, ttype, idx);
        if let Some(fresh_track) = pools.get(&pool_key) {
            // Merge: fresh track data + previous config overlay
            let mut new_item = fresh_track.clone();
            if let (Some(new_obj), Some(prev_obj)) =
                (new_item.as_object_mut(), prev_item.as_object())
            {
                for (k, v) in prev_obj {
                    new_obj.insert(k.clone(), v.clone());
                }
            }
            realized.push(new_item);
        }
    }

    // Filter out blocked video
    realized.retain(|t| !is_blocked_video(t));
    realized
}

/// Normalize single default per track type.
/// 1:1 port of ManualLogic.normalize_single_default_for_type().
fn normalize_defaults(layout: &mut [serde_json::Value], ttype: &str, force_if_none: bool) {
    // Find first track with is_default=true for this type
    let mut first_default_idx: Option<usize> = None;
    for (i, track) in layout.iter().enumerate() {
        if track_type_str(track) == ttype && track_bool(track, "is_default") {
            first_default_idx = Some(i);
            break;
        }
    }

    // If none found and force_if_none, set the first track of this type as default
    if first_default_idx.is_none() && force_if_none {
        for (i, track) in layout.iter().enumerate() {
            if track_type_str(track) == ttype {
                first_default_idx = Some(i);
                break;
            }
        }
    }

    // Clear all other defaults for this type
    for (i, track) in layout.iter_mut().enumerate() {
        if track_type_str(track) == ttype {
            let should_be_default = first_default_idx == Some(i);
            if let Some(obj) = track.as_object_mut() {
                obj.insert(
                    "is_default".to_string(),
                    serde_json::Value::Bool(should_be_default),
                );
            }
        }
    }
}

/// Normalize forced subtitles — at most one forced.
/// 1:1 port of ManualLogic.normalize_forced_subtitles().
fn normalize_forced(layout: &mut [serde_json::Value]) {
    let mut found_first = false;
    for track in layout.iter_mut() {
        if track_type_str(track) == TYPE_SUBTITLES {
            let is_forced = track_bool(track, "is_forced_display");
            if is_forced {
                if found_first {
                    // Clear subsequent forced flags
                    if let Some(obj) = track.as_object_mut() {
                        obj.insert(
                            "is_forced_display".to_string(),
                            serde_json::Value::Bool(false),
                        );
                    }
                } else {
                    found_first = true;
                }
            }
        }
    }
}

impl ffi::ManualSelectionLogic {
    fn initialize(
        mut self: Pin<&mut Self>,
        track_info_json: QString,
        previous_layout_json: QString,
        previous_attachments_json: QString,
        previous_source_settings_json: QString,
    ) {
        // Parse track info
        let info: HashMap<String, Vec<serde_json::Value>> =
            serde_json::from_str(&track_info_json.to_string()).unwrap_or_default();

        // Parse previous layout
        let prev_layout: Vec<serde_json::Value> =
            serde_json::from_str(&previous_layout_json.to_string()).unwrap_or_default();

        // Pool-matching prepopulate (not raw assignment!)
        let layout = prepopulate_with_pool_matching(&info, &prev_layout);

        self.as_mut().rust_mut().track_info = info;
        self.as_mut().rust_mut().layout_tracks = layout;

        // Restore attachments
        let prev_attachments: Vec<String> =
            serde_json::from_str(&previous_attachments_json.to_string()).unwrap_or_default();
        self.as_mut().rust_mut().attachment_sources = prev_attachments;

        // Restore source settings
        let prev_settings: HashMap<String, serde_json::Value> =
            serde_json::from_str(&previous_source_settings_json.to_string())
                .unwrap_or_default();
        self.as_mut().rust_mut().source_settings = prev_settings;

        // Set info text if we had a previous layout
        if !prev_layout.is_empty() {
            self.as_mut().set_info_text(QString::from(
                "Pre-populated with the layout from the previous file.",
            ));
        }

        let count = self.rust().layout_tracks.len() as i32;
        self.as_mut().set_layout_track_count(count);
        self.as_mut().layout_changed();
    }

    fn add_track_to_layout(
        mut self: Pin<&mut Self>,
        source_key: QString,
        track_index: i32,
    ) {
        let key = source_key.to_string();
        let idx = track_index as usize;

        let track = self
            .rust()
            .track_info
            .get(&key)
            .and_then(|tracks| tracks.get(idx))
            .cloned();

        if let Some(track) = track {
            if is_blocked_video(&track) {
                return;
            }

            self.as_mut().rust_mut().layout_tracks.push(track);
            let count = self.rust().layout_tracks.len() as i32;
            self.as_mut().set_layout_track_count(count);
            self.as_mut().layout_changed();
        }
    }

    fn remove_track_from_layout(mut self: Pin<&mut Self>, index: i32) {
        let idx = index as usize;
        if idx < self.rust().layout_tracks.len() {
            self.as_mut().rust_mut().layout_tracks.remove(idx);
            let count = self.rust().layout_tracks.len() as i32;
            self.as_mut().set_layout_track_count(count);
            self.as_mut().layout_changed();
        }
    }

    fn move_track(mut self: Pin<&mut Self>, from_index: i32, to_index: i32) {
        let from = from_index as usize;
        let to = to_index as usize;
        let len = self.rust().layout_tracks.len();
        if from < len && to < len && from != to {
            let track = self.as_mut().rust_mut().layout_tracks.remove(from);
            self.as_mut().rust_mut().layout_tracks.insert(to, track);
            self.as_mut().layout_changed();
        }
    }

    fn get_result(mut self: Pin<&mut Self>) -> QString {
        // Apply normalization before returning — 1:1 with Python's accept()
        let layout = &mut self.as_mut().rust_mut().layout_tracks;

        // Normalize audio defaults (force first audio as default if none set)
        normalize_defaults(layout, TYPE_AUDIO, true);
        // Normalize subtitle defaults (don't force if none set)
        normalize_defaults(layout, TYPE_SUBTITLES, false);
        // Normalize forced subtitles (at most one)
        normalize_forced(layout);

        let result = serde_json::json!({
            "layout": self.rust().layout_tracks,
            "attachment_sources": self.rust().attachment_sources,
            "source_settings": self.rust().source_settings,
        });
        let json = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
        QString::from(json.as_str())
    }

    fn get_layout_track(self: Pin<&mut Self>, index: i32) -> QString {
        let idx = index as usize;
        match self.rust().layout_tracks.get(idx) {
            Some(t) => {
                let json =
                    serde_json::to_string(t).unwrap_or_else(|_| "{}".to_string());
                QString::from(json.as_str())
            }
            None => QString::from("{}"),
        }
    }

    fn get_source_tracks(
        self: Pin<&mut Self>,
        source_key: QString,
    ) -> QString {
        let key = source_key.to_string();
        match self.rust().track_info.get(&key) {
            Some(t) => {
                let json =
                    serde_json::to_string(t).unwrap_or_else(|_| "[]".to_string());
                QString::from(json.as_str())
            }
            None => QString::from("[]"),
        }
    }

    fn get_source_keys(self: Pin<&mut Self>) -> QString {
        let mut keys: Vec<&String> = self.rust().track_info.keys().collect();
        // Sort numerically: "Source 1" < "Source 2" < "Source 3"
        keys.sort_by(|a, b| {
            let num_a = a.chars().filter(|c| c.is_ascii_digit()).collect::<String>();
            let num_b = b.chars().filter(|c| c.is_ascii_digit()).collect::<String>();
            num_a
                .parse::<i32>()
                .unwrap_or(999)
                .cmp(&num_b.parse::<i32>().unwrap_or(999))
        });
        let json =
            serde_json::to_string(&keys).unwrap_or_else(|_| "[]".to_string());
        QString::from(json.as_str())
    }

    fn toggle_attachment_source(
        mut self: Pin<&mut Self>,
        source_key: QString,
    ) {
        let key = source_key.to_string();
        let sources = &mut self.as_mut().rust_mut().attachment_sources;
        if let Some(pos) = sources.iter().position(|s| s == &key) {
            sources.remove(pos);
        } else {
            sources.push(key);
        }
    }

    fn set_attachment_source_checked(
        mut self: Pin<&mut Self>,
        source_key: QString,
        checked: bool,
    ) {
        let key = source_key.to_string();
        let sources = &mut self.as_mut().rust_mut().attachment_sources;
        let pos = sources.iter().position(|s| s == &key);
        if checked {
            if pos.is_none() {
                sources.push(key);
            }
        } else if let Some(pos) = pos {
            sources.remove(pos);
        }
    }

    fn get_attachment_sources(self: Pin<&mut Self>) -> QString {
        let json = serde_json::to_string(&self.rust().attachment_sources)
            .unwrap_or_else(|_| "[]".to_string());
        QString::from(json.as_str())
    }

    fn set_source_settings(
        mut self: Pin<&mut Self>,
        source_key: QString,
        settings_json: QString,
    ) {
        let key = source_key.to_string();
        let settings: serde_json::Value =
            serde_json::from_str(&settings_json.to_string()).unwrap_or_default();
        self.as_mut()
            .rust_mut()
            .source_settings
            .insert(key, settings);
    }

    fn clear_source_settings(
        mut self: Pin<&mut Self>,
        source_key: QString,
    ) {
        let key = source_key.to_string();
        self.as_mut().rust_mut().source_settings.remove(&key);
        self.as_mut().set_info_text(QString::from(
            &format!("Correlation settings cleared for {key}.") as &str,
        ));
    }

    fn get_source_settings_for(
        self: Pin<&mut Self>,
        source_key: QString,
    ) -> QString {
        let key = source_key.to_string();
        match self.rust().source_settings.get(&key) {
            Some(s) => {
                let json =
                    serde_json::to_string(s).unwrap_or_else(|_| "{}".to_string());
                QString::from(json.as_str())
            }
            None => QString::from("{}"),
        }
    }

    fn has_source_settings(
        self: Pin<&mut Self>,
        source_key: QString,
    ) -> bool {
        let key = source_key.to_string();
        self.rust().source_settings.contains_key(&key)
    }

    fn update_track_settings(
        mut self: Pin<&mut Self>,
        index: i32,
        settings_json: QString,
    ) {
        let idx = index as usize;
        let settings: serde_json::Value =
            serde_json::from_str(&settings_json.to_string()).unwrap_or_default();

        if idx < self.rust().layout_tracks.len() {
            if let Some(obj) = self.as_mut().rust_mut().layout_tracks[idx].as_object_mut()
            {
                if let Some(settings_obj) = settings.as_object() {
                    for (k, v) in settings_obj {
                        // Delete-if-empty logic: remove empty strings and empty arrays
                        let is_empty = (v.is_string()
                            && v.as_str().unwrap_or("").is_empty())
                            || (v.is_array()
                                && v.as_array().is_none_or(|a| a.is_empty()));
                        if is_empty {
                            obj.remove(k);
                        } else {
                            obj.insert(k.clone(), v.clone());
                        }
                    }
                }
            }
            self.as_mut().layout_changed();
        }
    }

    fn get_track_badges(self: Pin<&mut Self>, index: i32) -> QString {
        let idx = index as usize;
        match self.rust().layout_tracks.get(idx) {
            Some(track) => QString::from(&build_badges(track) as &str),
            None => QString::from(""),
        }
    }

    fn get_track_summary(self: Pin<&mut Self>, index: i32) -> QString {
        let idx = index as usize;
        match self.rust().layout_tracks.get(idx) {
            Some(track) => QString::from(&build_summary(track) as &str),
            None => QString::from(""),
        }
    }

    fn set_track_default(mut self: Pin<&mut Self>, index: i32, is_default: bool) {
        let idx = index as usize;
        let len = self.rust().layout_tracks.len();
        if idx >= len {
            return;
        }

        let ttype = track_type_str(&self.rust().layout_tracks[idx]).to_string();

        // Set/clear the requested track
        if let Some(obj) = self.as_mut().rust_mut().layout_tracks[idx].as_object_mut() {
            obj.insert(
                "is_default".to_string(),
                serde_json::Value::Bool(is_default),
            );
        }

        // If setting as default, clear all other defaults of same type
        if is_default {
            let layout = &mut self.as_mut().rust_mut().layout_tracks;
            for (i, track) in layout.iter_mut().enumerate() {
                if i != idx && track_type_str(track) == ttype {
                    if let Some(obj) = track.as_object_mut() {
                        obj.insert(
                            "is_default".to_string(),
                            serde_json::Value::Bool(false),
                        );
                    }
                }
            }
        }

        self.as_mut().layout_changed();
    }

    fn set_track_forced(mut self: Pin<&mut Self>, index: i32, is_forced: bool) {
        let idx = index as usize;
        let len = self.rust().layout_tracks.len();
        if idx >= len {
            return;
        }

        // Set the requested track
        if let Some(obj) = self.as_mut().rust_mut().layout_tracks[idx].as_object_mut() {
            obj.insert(
                "is_forced_display".to_string(),
                serde_json::Value::Bool(is_forced),
            );
        }

        // If setting as forced, clear all other forced subtitles
        if is_forced {
            let layout = &mut self.as_mut().rust_mut().layout_tracks;
            for (i, track) in layout.iter_mut().enumerate() {
                if i != idx && track_type_str(track) == TYPE_SUBTITLES {
                    if let Some(obj) = track.as_object_mut() {
                        obj.insert(
                            "is_forced_display".to_string(),
                            serde_json::Value::Bool(false),
                        );
                    }
                }
            }
        }

        self.as_mut().layout_changed();
    }

    fn add_external_subtitles(
        mut self: Pin<&mut Self>,
        paths_json: QString,
    ) -> QString {
        let paths: Vec<String> =
            serde_json::from_str(&paths_json.to_string()).unwrap_or_default();

        let mut added = Vec::new();

        for file_path in &paths {
            let path = std::path::Path::new(file_path);
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            let codec_id = match ext.as_str() {
                "srt" => "S_TEXT/UTF8",
                "ass" => "S_TEXT/ASS",
                "ssa" => "S_TEXT/SSA",
                "sup" => "S_HDMV/PGS",
                _ => "S_TEXT/UTF8",
            };

            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("External");

            let track_data = serde_json::json!({
                "source": "External",
                "original_path": file_path,
                "id": 0,
                "type": "subtitles",
                "codec_id": codec_id,
                "lang": "und",
                "name": name,
            });

            self.as_mut().rust_mut().layout_tracks.push(track_data.clone());
            added.push(track_data);
        }

        if !added.is_empty() {
            self.as_mut().set_has_external_subtitles(true);
            let count = self.rust().layout_tracks.len() as i32;
            self.as_mut().set_layout_track_count(count);
            self.as_mut().layout_changed();
        }

        let json = serde_json::to_string(&added).unwrap_or_else(|_| "[]".to_string());
        QString::from(json.as_str())
    }

    fn is_blocked_video_at(
        self: Pin<&mut Self>,
        source_key: QString,
        track_index: i32,
    ) -> bool {
        let key = source_key.to_string();
        let idx = track_index as usize;
        self.rust()
            .track_info
            .get(&key)
            .and_then(|tracks| tracks.get(idx))
            .map(is_blocked_video)
            .unwrap_or(false)
    }
}
