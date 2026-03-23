//! Track widget logic — 1:1 port of `vsg_qt/track_widget/logic.py`.
//!
//! Manages enable/disable state, track settings, and interaction
//! for a single track in the layout.

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// TrackWidgetLogic QObject — logic for a single track display.
        #[qobject]
        #[qml_element]
        #[qproperty(bool, enabled)]
        #[qproperty(QString, track_type)]
        #[qproperty(QString, description)]
        #[qproperty(QString, language)]
        #[qproperty(QString, codec)]
        #[qproperty(QString, codec_id)]
        #[qproperty(i32, track_id)]
        #[qproperty(QString, source_key)]
        #[qproperty(bool, is_default)]
        #[qproperty(bool, is_forced)]
        #[qproperty(bool, apply_track_name)]
        #[qproperty(bool, perform_ocr)]
        #[qproperty(bool, convert_to_ass)]
        #[qproperty(bool, rescale)]
        #[qproperty(f64, size_multiplier)]
        #[qproperty(QString, custom_name)]
        #[qproperty(QString, custom_lang)]
        #[qproperty(QString, sync_to)]
        #[qproperty(QString, summary_text)]
        #[qproperty(QString, badge_text)]
        #[qproperty(QString, source_label_text)]
        #[qproperty(bool, is_generated)]
        // Visibility flags for QML
        #[qproperty(bool, show_forced)]
        #[qproperty(bool, show_style_editor)]
        #[qproperty(bool, show_sync_to)]
        #[qproperty(bool, ocr_enabled)]
        #[qproperty(bool, convert_enabled)]
        type TrackWidgetLogic = super::TrackWidgetLogicRust;

        /// Initialize from track data JSON + available sources JSON array.
        #[qinvokable]
        fn initialize(
            self: Pin<&mut TrackWidgetLogic>,
            track_json: QString,
            available_sources_json: QString,
            source_settings_json: QString,
        );

        /// Get the current track configuration as JSON — 1:1 port of `get_config()`.
        #[qinvokable]
        fn get_config(self: Pin<&mut TrackWidgetLogic>) -> QString;

        /// Refresh the summary and badge text from current state.
        #[qinvokable]
        fn refresh_display(self: Pin<&mut TrackWidgetLogic>);

        /// Refresh badges only.
        #[qinvokable]
        fn refresh_badges(self: Pin<&mut TrackWidgetLogic>);

        /// Refresh summary only.
        #[qinvokable]
        fn refresh_summary(self: Pin<&mut TrackWidgetLogic>);

        /// Apply settings from TrackSettingsDialog result (JSON).
        #[qinvokable]
        fn apply_settings(self: Pin<&mut TrackWidgetLogic>, settings_json: QString);

        /// Get the available sync-to sources as JSON array of {display, value}.
        #[qinvokable]
        fn get_sync_sources(self: Pin<&mut TrackWidgetLogic>) -> QString;

        /// Signal: track data was modified.
        #[qsignal]
        fn track_modified(self: Pin<&mut TrackWidgetLogic>);

        /// Signal: request to open settings dialog for this track.
        #[qsignal]
        fn open_settings_requested(self: Pin<&mut TrackWidgetLogic>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

pub struct TrackWidgetLogicRust {
    enabled: bool,
    track_type: QString,
    description: QString,
    language: QString,
    codec: QString,
    codec_id: QString,
    track_id: i32,
    source_key: QString,
    is_default: bool,
    is_forced: bool,
    apply_track_name: bool,
    perform_ocr: bool,
    convert_to_ass: bool,
    rescale: bool,
    size_multiplier: f64,
    custom_name: QString,
    custom_lang: QString,
    sync_to: QString,
    summary_text: QString,
    badge_text: QString,
    source_label_text: QString,
    is_generated: bool,
    // Visibility flags
    show_forced: bool,
    show_style_editor: bool,
    show_sync_to: bool,
    ocr_enabled: bool,
    convert_enabled: bool,
    // Internal state not exposed as properties
    track_data: serde_json::Value,
    available_sources: Vec<String>,
    source_settings: serde_json::Value,
}

impl Default for TrackWidgetLogicRust {
    fn default() -> Self {
        Self {
            enabled: true,
            track_type: QString::from(""),
            description: QString::from(""),
            language: QString::from(""),
            codec: QString::from(""),
            codec_id: QString::from(""),
            track_id: 0,
            source_key: QString::from(""),
            is_default: false,
            is_forced: false,
            apply_track_name: false,
            perform_ocr: false,
            convert_to_ass: false,
            rescale: false,
            size_multiplier: 1.0,
            custom_name: QString::from(""),
            custom_lang: QString::from(""),
            sync_to: QString::from(""),
            summary_text: QString::from(""),
            badge_text: QString::from(""),
            source_label_text: QString::from(""),
            is_generated: false,
            show_forced: false,
            show_style_editor: false,
            show_sync_to: false,
            ocr_enabled: false,
            convert_enabled: false,
            track_data: serde_json::Value::Null,
            available_sources: Vec::new(),
            source_settings: serde_json::json!({}),
        }
    }
}

// ── Helper functions ──

fn get_str<'a>(v: &'a serde_json::Value, key: &str) -> &'a str {
    v.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

fn get_bool(v: &serde_json::Value, key: &str) -> bool {
    v.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}

fn get_f64(v: &serde_json::Value, key: &str) -> f64 {
    v.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0)
}

impl ffi::TrackWidgetLogic {
    /// Initialize from track data JSON — sets all properties from the track dict.
    /// 1:1 port of `TrackWidget.__init__` + `TrackWidgetLogic.init_ui_state`.
    fn initialize(
        mut self: Pin<&mut Self>,
        track_json: QString,
        available_sources_json: QString,
        source_settings_json: QString,
    ) {
        let data: serde_json::Value =
            serde_json::from_str(&track_json.to_string()).unwrap_or_default();
        let sources: Vec<String> =
            serde_json::from_str(&available_sources_json.to_string()).unwrap_or_default();
        let source_settings: serde_json::Value =
            serde_json::from_str(&source_settings_json.to_string()).unwrap_or_default();

        // Extract and set properties
        let track_type = get_str(&data, "type");
        let codec_id_raw = get_str(&data, "codec_id");
        let lang = get_str(&data, "lang");
        let name = get_str(&data, "name");
        let source = get_str(&data, "source");
        let id = data.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let is_subs = track_type == "subtitles";
        let is_external = source == "External";

        self.as_mut().set_track_type(QString::from(track_type));
        self.as_mut().set_codec_id(QString::from(codec_id_raw));
        self.as_mut().set_language(QString::from(if lang.is_empty() { "und" } else { lang }));
        self.as_mut().set_source_key(QString::from(source));
        self.as_mut().set_track_id(id);

        // Set quick-access controls from track_data
        self.as_mut().set_is_default(get_bool(&data, "is_default"));
        self.as_mut().set_is_forced(get_bool(&data, "is_forced_display"));
        self.as_mut().set_apply_track_name(get_bool(&data, "apply_track_name"));
        self.as_mut().set_is_generated(get_bool(&data, "is_generated"));

        // Subtitle-specific init — 1:1 with logic.py init_ui_state
        if is_subs {
            let size_mult = data
                .get("size_multiplier")
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0);
            self.as_mut().set_size_multiplier(size_mult);
            self.as_mut().set_perform_ocr(get_bool(&data, "perform_ocr"));
            self.as_mut().set_convert_to_ass(get_bool(&data, "convert_to_ass"));
            self.as_mut().set_rescale(get_bool(&data, "rescale"));
        } else {
            self.as_mut().set_size_multiplier(1.0);
        }

        // Visibility flags — 1:1 with init_ui_state
        self.as_mut().set_show_forced(is_subs);
        self.as_mut().set_show_style_editor(is_subs);
        self.as_mut().set_show_sync_to(is_subs && is_external);

        // OCR/convert enabled based on codec — 1:1 with track_settings_dialog/logic.py
        let codec_upper = codec_id_raw.to_uppercase();
        self.as_mut().set_ocr_enabled(
            codec_upper.contains("VOBSUB") || codec_upper.contains("PGS"),
        );
        self.as_mut()
            .set_convert_enabled(codec_upper.contains("S_TEXT/UTF8"));

        // Custom lang/name from track_data
        let custom_lang = get_str(&data, "custom_lang");
        let custom_name = get_str(&data, "custom_name");
        self.as_mut().set_custom_lang(QString::from(custom_lang));
        self.as_mut().set_custom_name(QString::from(custom_name));

        // Sync to (external subs only)
        let sync_to = get_str(&data, "sync_to");
        self.as_mut().set_sync_to(QString::from(sync_to));

        // Build description
        let codec_display = crate::track_widget::helpers::format_codec_display(codec_id_raw);
        let type_display = crate::track_widget::helpers::format_track_type(track_type);
        let desc = if name.is_empty() {
            format!("{type_display}: {codec_display} [{lang}]")
        } else {
            format!("{type_display}: {codec_display} [{lang}] - {name}")
        };
        self.as_mut().set_description(QString::from(desc.as_str()));

        // Store internals
        self.as_mut().rust_mut().track_data = data;
        self.as_mut().rust_mut().available_sources = sources;
        self.as_mut().rust_mut().source_settings = source_settings;

        // Initial refresh
        self.as_mut().refresh_summary_impl();
        self.as_mut().refresh_badges_impl();
    }

    /// Get the current configuration — 1:1 port of `logic.py::get_config()`.
    fn get_config(self: Pin<&mut Self>) -> QString {
        let is_subs = self.as_ref().track_type().to_string() == "subtitles";

        let size_mult = if is_subs {
            let v = *self.as_ref().size_multiplier();
            if v <= 0.0 || v > 10.0 { 1.0 } else { v }
        } else {
            1.0
        };

        // sync_to: only for visible external subs
        let sync_to = if *self.as_ref().show_sync_to() {
            let st = self.as_ref().sync_to().to_string();
            if st.is_empty() { serde_json::Value::Null } else { serde_json::json!(st) }
        } else {
            serde_json::Value::Null
        };

        let config = serde_json::json!({
            "is_default": *self.as_ref().is_default(),
            "apply_track_name": *self.as_ref().apply_track_name(),
            "is_forced_display": if is_subs { *self.as_ref().is_forced() } else { false },
            "perform_ocr": if is_subs { *self.as_ref().perform_ocr() } else { false },
            "convert_to_ass": if is_subs { *self.as_ref().convert_to_ass() } else { false },
            "rescale": if is_subs { *self.as_ref().rescale() } else { false },
            "size_multiplier": size_mult,
            "custom_lang": self.as_ref().custom_lang().to_string(),
            "custom_name": self.as_ref().custom_name().to_string(),
            "style_patch": self.rust().track_data.get("style_patch"),
            "font_replacements": self.rust().track_data.get("font_replacements"),
            "sync_to": sync_to,
            // Generated track fields
            "is_generated": *self.as_ref().is_generated(),
            "source_track_id": self.rust().track_data.get("source_track_id"),
            "filter_config": self.rust().track_data.get("filter_config"),
            "original_style_list": self.rust().track_data.get("original_style_list").unwrap_or(&serde_json::json!([])),
            // Sync exclusion fields
            "sync_exclusion_styles": self.rust().track_data.get("sync_exclusion_styles").unwrap_or(&serde_json::json!([])),
            "sync_exclusion_mode": self.rust().track_data.get("sync_exclusion_mode").unwrap_or(&serde_json::json!("exclude")),
            "sync_exclusion_original_style_list": self.rust().track_data.get("sync_exclusion_original_style_list").unwrap_or(&serde_json::json!([])),
        });
        let json = serde_json::to_string(&config).unwrap_or_else(|_| "{}".to_string());
        QString::from(json.as_str())
    }

    fn refresh_display(mut self: Pin<&mut Self>) {
        self.as_mut().refresh_summary_impl();
        self.as_mut().refresh_badges_impl();
    }

    fn refresh_badges(self: Pin<&mut Self>) {
        self.refresh_badges_impl();
    }

    fn refresh_summary(self: Pin<&mut Self>) {
        self.refresh_summary_impl();
    }

    /// Apply settings from TrackSettingsDialog.
    /// 1:1 port of `TrackWidget._open_settings_dialog` apply block.
    fn apply_settings(mut self: Pin<&mut Self>, settings_json: QString) {
        let settings: serde_json::Value =
            serde_json::from_str(&settings_json.to_string()).unwrap_or_default();

        // Update hidden controls — 1:1 with Python
        if let Some(ocr) = settings.get("perform_ocr").and_then(|v| v.as_bool()) {
            self.as_mut().set_perform_ocr(ocr);
        }
        if let Some(conv) = settings.get("convert_to_ass").and_then(|v| v.as_bool()) {
            self.as_mut().set_convert_to_ass(conv);
        }
        if let Some(resc) = settings.get("rescale").and_then(|v| v.as_bool()) {
            self.as_mut().set_rescale(resc);
        }
        if let Some(mult) = settings.get("size_multiplier").and_then(|v| v.as_f64()) {
            self.as_mut().set_size_multiplier(mult);
        }

        // Store custom language in track_data — 1:1 with Python
        let custom_lang = settings
            .get("custom_lang")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        self.as_mut()
            .set_custom_lang(QString::from(custom_lang));
        if let Some(obj) = self.as_mut().rust_mut().track_data.as_object_mut() {
            if custom_lang.is_empty() {
                obj.remove("custom_lang");
            } else {
                obj.insert(
                    "custom_lang".to_string(),
                    serde_json::json!(custom_lang),
                );
            }
        }

        // Store custom name in track_data — 1:1 with Python
        let custom_name = settings
            .get("custom_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        self.as_mut()
            .set_custom_name(QString::from(custom_name));
        if let Some(obj) = self.as_mut().rust_mut().track_data.as_object_mut() {
            if custom_name.is_empty() {
                obj.remove("custom_name");
            } else {
                obj.insert(
                    "custom_name".to_string(),
                    serde_json::json!(custom_name),
                );
            }
        }

        // Store sync exclusion config in track_data — 1:1 with Python
        let sync_styles = settings
            .get("sync_exclusion_styles")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        if let Some(obj) = self.as_mut().rust_mut().track_data.as_object_mut() {
            if sync_styles.is_empty() {
                obj.remove("sync_exclusion_styles");
                obj.remove("sync_exclusion_mode");
                obj.remove("sync_exclusion_original_style_list");
            } else {
                obj.insert(
                    "sync_exclusion_styles".to_string(),
                    serde_json::json!(sync_styles),
                );
                if let Some(mode) = settings.get("sync_exclusion_mode") {
                    obj.insert("sync_exclusion_mode".to_string(), mode.clone());
                }
                if let Some(orig) = settings.get("sync_exclusion_original_style_list") {
                    obj.insert(
                        "sync_exclusion_original_style_list".to_string(),
                        orig.clone(),
                    );
                }
            }
        }

        self.as_mut().refresh_summary_impl();
        self.as_mut().refresh_badges_impl();
        self.as_mut().track_modified();
    }

    fn get_sync_sources(self: Pin<&mut Self>) -> QString {
        let mut sources = vec![serde_json::json!({"display": "Default (Source 1)", "value": "Source 1"})];
        for src in &self.rust().available_sources {
            if src != "Source 1" {
                sources.push(serde_json::json!({"display": src, "value": src}));
            }
        }
        let json = serde_json::to_string(&sources).unwrap_or_else(|_| "[]".to_string());
        QString::from(json.as_str())
    }

    // ── Internal implementations ──

    /// Refresh badges — 1:1 port of `logic.py::refresh_badges()`.
    fn refresh_badges_impl(mut self: Pin<&mut Self>) {
        let mut badges = Vec::new();
        let track_type = self.as_ref().track_type().to_string();
        let is_subs = track_type == "subtitles";

        // Generated track badge (most important, first)
        if *self.as_ref().is_generated() {
            let filter_cfg = self.rust().track_data.get("filter_config");
            let needs_config = get_bool(&self.rust().track_data, "needs_configuration")
                || filter_cfg
                    .and_then(|c| c.get("filter_styles"))
                    .and_then(|v| v.as_array())
                    .is_none_or(|a| a.is_empty());
            if needs_config {
                badges.push("Generated ⚠️ Needs Config".to_string());
            } else {
                badges.push("Generated".to_string());
            }
        }

        // Correlation settings badge for audio tracks from Source 2/3
        let source = self.as_ref().source_key().to_string();
        if track_type == "audio" && source != "Source 1" {
            let ss = self
                .rust()
                .source_settings
                .get(&source)
                .cloned()
                .unwrap_or_default();
            let has_corr = ss.get("correlation_source_track").is_some_and(|v| !v.is_null())
                || get_bool(&ss, "use_source_separation");
            if has_corr {
                badges.push("Correlation Settings".to_string());
            }
        }

        if *self.as_ref().is_default() {
            badges.push("Default".to_string());
        }
        if is_subs && *self.as_ref().is_forced() {
            badges.push("Forced".to_string());
        }

        // Sync exclusion badge
        if self
            .rust()
            .track_data
            .get("sync_exclusion_styles")
            .and_then(|v| v.as_array())
            .is_some_and(|a| !a.is_empty())
        {
            badges.push("Sync Exclusions".to_string());
        }

        // Edited/Styled badges
        if !get_str(&self.rust().track_data, "user_modified_path").is_empty() {
            badges.push("Edited".to_string());
        } else if self
            .rust()
            .track_data
            .get("style_patch")
            .and_then(|v| v.as_object())
            .is_some_and(|o| !o.is_empty())
        {
            badges.push("Styled".to_string());
        }

        // Font replacements badge
        if let Some(font_count) = self
            .rust()
            .track_data
            .get("font_replacements")
            .and_then(|v| v.as_object())
            .map(|o| o.len())
        {
            if font_count > 0 {
                badges.push(format!("Fonts: {font_count}"));
            }
        }

        // Paste warnings badge
        if get_bool(&self.rust().track_data, "pasted_warnings") {
            badges.push("⚠️ Paste Warnings".to_string());
        }

        // Custom language badge
        let orig_lang = get_str(&self.rust().track_data, "lang");
        let custom_lang = self.as_ref().custom_lang().to_string();
        if !custom_lang.is_empty() && custom_lang != orig_lang {
            badges.push(format!("Lang: {custom_lang}"));
        }

        // Custom name badge
        let custom_name = self.as_ref().custom_name().to_string();
        if !custom_name.is_empty() {
            let display_name = if custom_name.len() <= 20 {
                custom_name.clone()
            } else {
                format!("{}...", &custom_name[..17])
            };
            badges.push(format!("Name: {display_name}"));
        }

        let badge_str = badges.join(" | ");
        self.as_mut()
            .set_badge_text(QString::from(badge_str.as_str()));
    }

    /// Refresh summary — 1:1 port of `logic.py::refresh_summary()`.
    fn refresh_summary_impl(mut self: Pin<&mut Self>) {
        let track_type = self.as_ref().track_type().to_string();
        let id = *self.as_ref().track_id();
        let source = self.as_ref().source_key().to_string();
        let description = self.as_ref().description().to_string();
        let is_subs = track_type == "subtitles";

        // Main summary — Python's summary_label
        let original_lang = get_str(&self.rust().track_data, "lang");
        let custom_lang = self.as_ref().custom_lang().to_string();
        let lang_indicator = if !custom_lang.is_empty() && custom_lang != original_lang {
            format!(" → {custom_lang}")
        } else {
            String::new()
        };

        let type_letter = match track_type.as_str() {
            "video" => "V",
            "audio" => "A",
            "subtitles" => "S",
            _ => "?",
        };

        let is_generated = *self.as_ref().is_generated();
        let gen_indicator = if is_generated {
            let gen_source = get_str(&self.rust().track_data, "source");
            let gen_track_id = self
                .rust()
                .track_data
                .get("source_track_id")
                .and_then(|v| v.as_i64())
                .map(|v| v.to_string())
                .unwrap_or_else(|| "N/A".to_string());
            format!(" [Generated from {gen_source} Track {gen_track_id}]")
        } else {
            String::new()
        };

        let summary = format!(
            "[{source}] [{type_letter}-{id}] {description}{lang_indicator}{gen_indicator}"
        );
        self.as_mut()
            .set_summary_text(QString::from(summary.as_str()));

        // Inline options summary — Python's source_label
        let mut parts = Vec::new();

        // Correlation settings for audio tracks from Source 2/3
        if track_type == "audio" && source != "Source 1" {
            let ss = self
                .rust()
                .source_settings
                .get(&source)
                .cloned()
                .unwrap_or_default();
            let mut corr_parts = Vec::new();
            if let Some(track) = ss.get("correlation_source_track").and_then(|v| v.as_i64()) {
                corr_parts.push(format!("Using Track {track}"));
            }
            if get_bool(&ss, "use_source_separation") {
                corr_parts.push("Source Separation".to_string());
            }
            if !corr_parts.is_empty() {
                parts.push(format!("Corr: {}", corr_parts.join(", ")));
            }
        }

        // Generated track filter info
        if is_generated {
            let filter_cfg = self
                .rust()
                .track_data
                .get("filter_config")
                .cloned()
                .unwrap_or_default();
            let gen_mode = filter_cfg
                .get("filter_mode")
                .and_then(|v| v.as_str())
                .unwrap_or("exclude");
            let gen_styles: Vec<String> = filter_cfg
                .get("filter_styles")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            if !gen_styles.is_empty() {
                let mut styles_str: String = gen_styles[..gen_styles.len().min(3)].join(", ");
                if gen_styles.len() > 3 {
                    styles_str.push_str(&format!(", +{} more", gen_styles.len() - 3));
                }
                let mode_word = if gen_mode == "exclude" {
                    "Excluding"
                } else {
                    "Including"
                };
                parts.push(format!("{mode_word}: {styles_str}"));
            }
        }

        // Subtitle-specific inline options
        if is_subs {
            if *self.as_ref().perform_ocr() {
                parts.push("OCR".to_string());
            }
            if *self.as_ref().convert_to_ass() {
                parts.push("→ASS".to_string());
            }
            if *self.as_ref().rescale() {
                parts.push("Rescale".to_string());
            }

            let size_mult = *self.as_ref().size_multiplier();
            if (size_mult - 1.0).abs() > 1e-6 {
                parts.push(format!("{size_mult:.2}x Size"));
            }

            // Sync exclusion details
            let sync_styles: Vec<String> = self
                .rust()
                .track_data
                .get("sync_exclusion_styles")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            if !sync_styles.is_empty() {
                let sync_mode = get_str(&self.rust().track_data, "sync_exclusion_mode");
                let mode_word = if sync_mode == "include" {
                    "Including"
                } else {
                    "Excluding"
                };
                let mut styles_str: String = sync_styles[..sync_styles.len().min(2)].join(", ");
                if sync_styles.len() > 2 {
                    styles_str.push_str(&format!(" +{} more", sync_styles.len() - 2));
                }
                parts.push(format!("{mode_word} sync: {styles_str}"));
            }

            // Font replacements
            if let Some(font_count) = self
                .rust()
                .track_data
                .get("font_replacements")
                .and_then(|v| v.as_object())
                .map(|o| o.len())
            {
                if font_count > 0 {
                    parts.push(format!("Fonts: {font_count}"));
                }
            }
        }

        let source_label = if parts.is_empty() {
            String::new()
        } else {
            format!("└ ⚙ {}", parts.join(", "))
        };
        self.as_mut()
            .set_source_label_text(QString::from(source_label.as_str()));
    }
}
