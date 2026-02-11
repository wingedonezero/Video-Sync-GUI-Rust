//! Manual Selection Dialog component.
//!
//! Matches the PySide ManualSelectionDialog layout:
//! - Left pane: scrollable source track lists (one per source, grouped in Frames)
//!   + "Add External Subtitle(s)..." button + external subtitles group
//! - Right pane: final output list (drag-to-reorder, TrackWidget per row) + attachments
//! - OK/Cancel at bottom
//! - Double-click or drag from source list to add to final list
//! - Drag within final list to reorder
//! - Keyboard: Ctrl+Up/Down to move, Delete to remove from final list
//! - Video tracks from non-Source-1 are blocked
//! - Enforces single default per track type, single forced subtitle
//! - Context menus on source items, source group headers, and final list items
//! - Rich per-track widget with inline checkboxes (Default, Forced, Set Name)
//! - Stub buttons for Style Editor, Track Settings, Source Correlation Settings

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use relm4::prelude::*;

use vsg_core::extraction::{
    build_track_description, get_detailed_stream_info, probe_file, TrackType as ExtractionTrackType,
};
use vsg_core::jobs::{FinalTrackEntry, ManualLayout, SourceCorrelationSettings, TrackConfig};
use vsg_core::models::TrackType;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Convert extraction TrackType to models TrackType.
fn convert_track_type(et: ExtractionTrackType) -> TrackType {
    match et {
        ExtractionTrackType::Video => TrackType::Video,
        ExtractionTrackType::Audio => TrackType::Audio,
        ExtractionTrackType::Subtitles => TrackType::Subtitles,
    }
}

/// Get the type prefix for display (V, A, S).
fn track_type_prefix(tt: TrackType) -> &'static str {
    match tt {
        TrackType::Video => "V",
        TrackType::Audio => "A",
        TrackType::Subtitles => "S",
    }
}

/// Map file extension to subtitle codec_id (matches Python's mapping).
fn extension_to_codec_id(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "srt" => "S_TEXT/UTF8",
        "ass" => "S_TEXT/ASS",
        "ssa" => "S_TEXT/SSA",
        "sup" => "S_HDMV/PGS",
        _ => "S_TEXT/UTF8",
    }
}

/// Check if a codec_id is a text-based subtitle.
fn is_text_codec(codec_id: &str) -> bool {
    matches!(
        codec_id,
        "S_TEXT/UTF8" | "S_TEXT/ASS" | "S_TEXT/SSA" | "S_TEXT/WEBVTT"
    )
}

/// Load CSS for the manual selection dialog.
fn load_dialog_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(
        r#"
        .badge-label {
            color: #E0A800;
            font-weight: bold;
        }
        .summary-label {
            font-weight: bold;
        }
        .info-success {
            color: green;
            font-weight: bold;
        }
        .source-label-dim {
            color: #808080;
            font-size: 0.9em;
        }
        .track-widget-row {
            padding: 5px;
        }
        "#,
    );
    gtk::style_context_add_provider_for_display(
        &gdk::Display::default().expect("Could not get default display"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

// ─── Track data representation used in the UI ───────────────────────────────

/// Simplified track data for UI display and interaction.
/// Mirrors the Python track dict used in PySide.
#[derive(Debug, Clone)]
pub struct UiTrackData {
    /// Track ID within source file.
    pub track_id: usize,
    /// Track type.
    pub track_type: TrackType,
    /// Source key (e.g., "Source 1", "External").
    pub source_key: String,
    /// Codec ID (e.g., "A_AAC", "S_TEXT/UTF8").
    pub codec_id: String,
    /// Display description (rich MediaInfo-like).
    pub description: String,
    /// Language code.
    pub language: String,
    /// Track name (from container).
    pub name: Option<String>,
    /// Original file path.
    pub original_path: PathBuf,
    /// Whether this was the default track in the source.
    pub is_default: bool,
    /// Whether this was forced in the source.
    pub is_forced: bool,
    /// Whether this is a text-based subtitle.
    pub is_text_subtitle: bool,
    /// Whether this is a generated (filtered) track.
    pub is_generated: bool,
    /// Source track ID this was generated from.
    pub generated_source_track_id: Option<usize>,
    /// Whether this generated track needs configuration.
    pub needs_configuration: bool,
}

impl UiTrackData {
    /// Build a display label for the source list.
    /// e.g. "[A-1] FLAC (jpn) 'Commentary' | 48000 Hz, 5.1"
    fn source_list_label(&self) -> String {
        format!(
            "[{}-{}] {}",
            track_type_prefix(self.track_type),
            self.track_id,
            self.description
        )
    }

    /// Check if this is a video track from a non-reference source (blocked).
    fn is_blocked_video(&self) -> bool {
        self.track_type == TrackType::Video && self.source_key != "Source 1"
    }
}

// ─── Final list track entry (with user config) ─────────────────────────────

/// A track in the final output list with user-configurable options.
/// Matches all fields from Python's TrackWidget + TrackWidgetLogic.
#[derive(Debug, Clone)]
struct FinalTrack {
    /// The underlying track data.
    data: UiTrackData,

    // ── User config (matches TrackConfig) ──
    is_default: bool,
    is_forced: bool,
    apply_track_name: bool,
    custom_name: Option<String>,
    custom_lang: Option<String>,

    // ── Subtitle options ──
    perform_ocr: bool,
    convert_to_ass: bool,
    rescale: bool,
    size_multiplier: f32,
    sync_to_source: Option<String>,

    // ── Sync exclusions ──
    sync_exclusion_styles: Vec<String>,
    sync_exclusion_mode: String,
    sync_exclusion_original_style_list: Vec<String>,

    // ── Style/font edits (stored, not editable in UI yet) ──
    style_patch: Option<HashMap<String, serde_json::Value>>,
    font_replacements: Option<HashMap<String, String>>,
    user_modified_path: Option<PathBuf>,

    // ── Generated track fields ──
    is_generated: bool,
    generated_source_track_id: Option<usize>,
    generated_filter_mode: String,
    generated_filter_styles: Vec<String>,
    generated_original_style_list: Vec<String>,
    needs_configuration: bool,

    // ── Paste warnings ──
    has_paste_warnings: bool,
}

impl FinalTrack {
    fn new(data: UiTrackData) -> Self {
        let is_generated = data.is_generated;
        let generated_source_track_id = data.generated_source_track_id;
        let needs_configuration = data.needs_configuration;
        Self {
            is_default: data.is_default,
            is_forced: data.is_forced,
            apply_track_name: false,
            custom_name: None,
            custom_lang: None,
            perform_ocr: false,
            convert_to_ass: false,
            rescale: false,
            size_multiplier: 1.0,
            sync_to_source: None,
            sync_exclusion_styles: Vec::new(),
            sync_exclusion_mode: "exclude".to_string(),
            sync_exclusion_original_style_list: Vec::new(),
            style_patch: None,
            font_replacements: None,
            user_modified_path: None,
            is_generated,
            generated_source_track_id,
            generated_filter_mode: "exclude".to_string(),
            generated_filter_styles: Vec::new(),
            generated_original_style_list: Vec::new(),
            needs_configuration,
            has_paste_warnings: false,
            data,
        }
    }

    /// Build badge text for display (matches Qt badge system exactly).
    fn badges(&self, source_settings: &HashMap<String, SourceCorrelationSettings>) -> String {
        let mut badges: Vec<String> = Vec::new();

        // 1. Generated track status
        if self.is_generated {
            if self.needs_configuration {
                badges.push("\u{1F517} Generated \u{26A0}\u{FE0F} Needs Config".to_string());
            } else {
                badges.push("\u{1F517} Generated".to_string());
            }
        }

        // 2. Correlation settings (for audio tracks from Source 2/3 with custom settings)
        if self.data.track_type == TrackType::Audio && self.data.source_key != "Source 1" {
            if let Some(settings) = source_settings.get(&self.data.source_key) {
                if settings.correlation_track.is_some() || settings.use_source_separation {
                    badges.push("\u{1F3AF} Correlation Settings".to_string());
                }
            }
        }

        // 3. Default
        if self.is_default {
            badges.push("Default".to_string());
        }

        // 4. Forced (subtitles only)
        if self.data.track_type == TrackType::Subtitles && self.is_forced {
            badges.push("Forced".to_string());
        }

        // 5. Sync exclusions
        if !self.sync_exclusion_styles.is_empty() {
            badges.push("\u{26A1} Sync Exclusions".to_string());
        }

        // 6. Edited (has temp modified file)
        if self.user_modified_path.is_some() {
            badges.push("Edited".to_string());
        }

        // 7. Styled (has style patch)
        if self.style_patch.is_some() {
            badges.push("Styled".to_string());
        }

        // 8. Font replacements
        if let Some(ref fonts) = self.font_replacements {
            if !fonts.is_empty() {
                badges.push(format!("Fonts: {}", fonts.len()));
            }
        }

        // 9. Paste warnings
        if self.has_paste_warnings {
            badges.push("\u{26A0}\u{FE0F} Paste Warnings".to_string());
        }

        // 10. Custom language
        if let Some(ref lang) = self.custom_lang {
            if !lang.is_empty() {
                badges.push(format!("Lang: {lang}"));
            }
        }

        // 11. Custom name
        if let Some(ref name) = self.custom_name {
            if !name.is_empty() {
                let truncated = if name.len() > 20 {
                    format!("{}...", &name[..20])
                } else {
                    name.clone()
                };
                badges.push(format!("Name: {truncated}"));
            }
        }

        badges.join(" | ")
    }

    /// Build the summary label text (matches Qt summary_label).
    fn summary_text(&self) -> String {
        let base = format!(
            "[{}] [{}]",
            self.data.source_key,
            self.data.source_list_label()
        );

        // Language change indicator
        let lang_indicator = if let Some(ref lang) = self.custom_lang {
            if !lang.is_empty() && lang != &self.data.language {
                format!(" \u{2192} {lang}")
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        // Generated track indicator
        let gen_indicator = if self.is_generated {
            if let Some(src_id) = self.generated_source_track_id {
                let styles_preview = if self.generated_filter_styles.is_empty() {
                    String::new()
                } else {
                    let preview: Vec<&str> = self
                        .generated_filter_styles
                        .iter()
                        .take(3)
                        .map(|s| s.as_str())
                        .collect();
                    let extra = if self.generated_filter_styles.len() > 3 {
                        format!(", +{} more", self.generated_filter_styles.len() - 3)
                    } else {
                        String::new()
                    };
                    format!(" ({}{})", preview.join(", "), extra)
                };
                format!(
                    " [Generated from {} Track {}{}]",
                    self.data.source_key, src_id, styles_preview
                )
            } else {
                " [Generated]".to_string()
            }
        } else {
            String::new()
        };

        format!("{base}{lang_indicator}{gen_indicator}")
    }

    /// Build inline config summary (matches Qt source_label).
    fn inline_summary(
        &self,
        source_settings: &HashMap<String, SourceCorrelationSettings>,
    ) -> String {
        let mut parts: Vec<String> = Vec::new();

        // Correlation settings for audio
        if self.data.track_type == TrackType::Audio && self.data.source_key != "Source 1" {
            if let Some(settings) = source_settings.get(&self.data.source_key) {
                let mut corr_parts = Vec::new();
                if let Some(track) = settings.correlation_track {
                    corr_parts.push(format!("\u{1F3AF} Using Track {track}"));
                }
                if settings.use_source_separation {
                    corr_parts.push("Source Separation".to_string());
                }
                if !corr_parts.is_empty() {
                    parts.push(corr_parts.join(", "));
                }
            }
        }

        // Subtitle options
        if self.data.track_type == TrackType::Subtitles {
            let mut sub_opts = Vec::new();
            if self.perform_ocr {
                sub_opts.push("OCR".to_string());
            }
            if self.convert_to_ass {
                sub_opts.push("\u{2192}ASS".to_string());
            }
            if self.rescale {
                sub_opts.push("Rescale".to_string());
            }
            if (self.size_multiplier - 1.0).abs() > 0.01 {
                sub_opts.push(format!("{:.2}x Size", self.size_multiplier));
            }

            // Sync exclusions
            if !self.sync_exclusion_styles.is_empty() {
                let mode = if self.sync_exclusion_mode == "include" {
                    "Including"
                } else {
                    "Excluding"
                };
                let preview: Vec<&str> = self
                    .sync_exclusion_styles
                    .iter()
                    .take(3)
                    .map(|s| s.as_str())
                    .collect();
                sub_opts.push(format!("\u{26A1} {mode} sync: {}", preview.join(", ")));
            }

            // Font replacements count
            if let Some(ref fonts) = self.font_replacements {
                if !fonts.is_empty() {
                    sub_opts.push(format!("Fonts: {}", fonts.len()));
                }
            }

            if !sub_opts.is_empty() {
                parts.push(sub_opts.join(", "));
            }
        }

        if parts.is_empty() {
            String::new()
        } else {
            format!("\u{2514} \u{2699} {}", parts.join(", "))
        }
    }

    /// Convert to backend FinalTrackEntry.
    fn to_entry(&self, order_index: usize, position_in_source_type: usize) -> FinalTrackEntry {
        let mut entry = FinalTrackEntry::new(
            self.data.track_id,
            self.data.source_key.clone(),
            self.data.track_type,
        );
        entry.codec_id = self.data.codec_id.clone();
        entry.user_order_index = order_index;
        entry.position_in_source_type = position_in_source_type;
        entry.config = TrackConfig {
            sync_to_source: self.sync_to_source.clone(),
            is_default: self.is_default,
            is_forced_display: self.is_forced,
            custom_name: self.custom_name.clone(),
            custom_lang: self.custom_lang.clone(),
            apply_track_name: self.apply_track_name,
            perform_ocr: self.perform_ocr,
            convert_to_ass: self.convert_to_ass,
            rescale: self.rescale,
            size_multiplier: self.size_multiplier,
            sync_exclusion_styles: self.sync_exclusion_styles.clone(),
            sync_exclusion_mode: self.sync_exclusion_mode.clone(),
            sync_exclusion_original_style_list: self.sync_exclusion_original_style_list.clone(),
            skip_frame_validation: false,
            style_patch: self.style_patch.clone(),
            font_replacements: self.font_replacements.clone(),
            aspect_ratio: None,
        };

        // Generated track fields
        entry.is_generated = self.is_generated;
        entry.generated_source_track_id = self.generated_source_track_id;
        entry.generated_filter_mode = self.generated_filter_mode.clone();
        entry.generated_filter_styles = self.generated_filter_styles.clone();
        entry.generated_original_style_list = self.generated_original_style_list.clone();

        // External track: store the original file path
        if self.data.source_key == "External" {
            entry.generated_source_path =
                Some(self.data.original_path.to_string_lossy().to_string());
        }

        entry
    }
}

// ─── Messages ───────────────────────────────────────────────────────────────

/// Messages for the Manual Selection Dialog.
#[derive(Debug)]
pub enum ManualSelectionMsg {
    /// Show the dialog with probed track info for each source.
    Show {
        sources: HashMap<String, PathBuf>,
        previous_layout: Option<ManualLayout>,
        job_name: String,
    },
    /// Close (Cancel).
    Close,
    /// Accept (OK) — build layout and emit output.
    Accept,

    // ── Source list interactions ──
    /// Add a track from the source list to the final list.
    AddTrackToFinal(UiTrackData),

    // ── Final list management ──
    MoveUp,
    MoveDown,
    RemoveSelected,
    ToggleDefault,
    ToggleForced,
    ToggleSetName,
    FinalSelectionChanged,

    // ── External subtitles ──
    AddExternalSubtitles,
    ExternalSubsSelected(Vec<PathBuf>),

    // ── Stub actions (not yet implemented) ──
    CreateSignsTrack(UiTrackData),
    OpenStyleEditor,
    OpenTrackSettings,
    OpenSourceSettings(String),
    #[allow(dead_code)]
    ClearSourceSettings(String),
    CopyStyleEdits,
    PasteStyleEdits,
    EditGeneratedTrack,

    /// Reorder: move track from source index to target index (drag-and-drop).
    ReorderTrack { from: usize, to: usize },

    // ── Internal ──
    ProbeComplete(ProbeData),
    ProbeFailed(String),
}

/// Probed data for all sources.
#[derive(Debug)]
pub struct ProbeData {
    /// Track info per source, in order.
    pub source_tracks: Vec<(String, Vec<UiTrackData>)>,
    /// Whether each source has attachments.
    pub source_has_attachments: HashMap<String, bool>,
}

/// Output from the manual selection dialog.
#[derive(Debug)]
pub enum ManualSelectionOutput {
    /// User accepted — here is the layout.
    Applied(ManualLayout),
    /// Log message.
    Log(String),
}

// ─── Component ──────────────────────────────────────────────────────────────

/// Manual Selection Dialog state.
pub struct ManualSelectionDialog {
    visible: bool,
    job_name: String,
    /// Source track data (keyed by source key).
    source_tracks: Vec<(String, Vec<UiTrackData>)>,
    /// External subtitle tracks.
    external_tracks: Vec<UiTrackData>,
    /// Which sources have attachments.
    source_has_attachments: HashMap<String, bool>,
    /// Ordered list of final tracks (the source of truth).
    final_tracks: Vec<FinalTrack>,
    /// Index of currently selected final track (None if no selection).
    selected_final: Option<usize>,
    /// Attachment checkboxes (source_key -> CheckButton widget).
    attachment_checks: HashMap<String, gtk::CheckButton>,
    /// Source correlation settings (preserved across dialog sessions).
    source_settings: HashMap<String, SourceCorrelationSettings>,
    /// Info label for status messages.
    info_text: String,
    /// Previous layout for prepopulation.
    previous_layout: Option<ManualLayout>,
    /// Available source keys (for sync_to dropdown).
    available_sources: Vec<String>,

    // ── Widget references (built imperatively) ──
    source_pane: gtk::Box,
    attachment_box: gtk::Box,
    final_list_box: gtk::ListBox,
    external_frame: gtk::Frame,
    external_list_box: gtk::ListBox,
    root_window: Option<gtk::Window>,

    /// Style edit clipboard (stub).
    #[allow(dead_code)]
    style_edit_clipboard: Option<()>,

    /// Whether final list needs UI refresh.
    final_list_dirty: Cell<bool>,

    /// Current context menu popover (shared with gesture handler for cleanup).
    context_popover: Rc<RefCell<Option<gtk::PopoverMenu>>>,
}

#[relm4::component(pub)]
impl SimpleComponent for ManualSelectionDialog {
    type Init = gtk::Window;
    type Input = ManualSelectionMsg;
    type Output = ManualSelectionOutput;

    view! {
        #[root]
        gtk::Window {
            set_title: Some("Manual Track Selection"),
            set_default_width: 1200,
            set_default_height: 700,
            set_modal: true,
            #[watch]
            set_visible: model.visible,
            connect_close_request[sender] => move |_| {
                sender.input(ManualSelectionMsg::Close);
                glib::Propagation::Stop
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 6,
                set_margin_all: 8,

                // Info label (top, centered, green bold)
                #[name = "info_label"]
                gtk::Label {
                    set_halign: gtk::Align::Center,
                    add_css_class: "info-success",
                    #[watch]
                    set_label: &model.info_text,
                    #[watch]
                    set_visible: !model.info_text.is_empty(),
                },

                // Main content: left pane + right pane
                gtk::Paned {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_vexpand: true,
                    set_position: 400,

                    // Left pane: source track lists + external subs button
                    #[wrap(Some)]
                    set_start_child = &gtk::ScrolledWindow {
                        set_hscrollbar_policy: gtk::PolicyType::Never,
                        set_vscrollbar_policy: gtk::PolicyType::Automatic,
                        set_hexpand: true,
                        set_vexpand: true,

                        #[name = "source_scroll_content"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 6,
                            set_margin_all: 4,
                        },
                    },

                    // Right pane: final output list + attachments
                    #[wrap(Some)]
                    set_end_child = &gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 6,
                        set_hexpand: true,

                        // Final output frame
                        gtk::Frame {
                            set_label: Some("Final Output (Drag to reorder)"),
                            set_vexpand: true,

                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 4,

                                #[name = "final_scroll"]
                                gtk::ScrolledWindow {
                                    set_vexpand: true,
                                    set_hscrollbar_policy: gtk::PolicyType::Never,
                                    set_vscrollbar_policy: gtk::PolicyType::Automatic,
                                },

                                // Button row for final list management
                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 4,
                                    set_margin_start: 4,
                                    set_margin_end: 4,
                                    set_margin_bottom: 4,

                                    #[name = "move_up_btn"]
                                    gtk::Button {
                                        set_label: "Move Up",
                                        set_sensitive: false,
                                        connect_clicked => ManualSelectionMsg::MoveUp,
                                    },
                                    #[name = "move_down_btn"]
                                    gtk::Button {
                                        set_label: "Move Down",
                                        set_sensitive: false,
                                        connect_clicked => ManualSelectionMsg::MoveDown,
                                    },
                                    #[name = "remove_btn"]
                                    gtk::Button {
                                        set_label: "Remove",
                                        set_sensitive: false,
                                        connect_clicked => ManualSelectionMsg::RemoveSelected,
                                    },
                                    // Spacer
                                    gtk::Box { set_hexpand: true },
                                    #[name = "default_btn"]
                                    gtk::Button {
                                        set_label: "Toggle Default",
                                        set_sensitive: false,
                                        connect_clicked => ManualSelectionMsg::ToggleDefault,
                                    },
                                    #[name = "forced_btn"]
                                    gtk::Button {
                                        set_label: "Toggle Forced",
                                        set_sensitive: false,
                                        connect_clicked => ManualSelectionMsg::ToggleForced,
                                    },
                                },
                            },
                        },

                        // Attachments frame
                        gtk::Frame {
                            set_label: Some("Attachments"),
                            #[name = "attachment_content"]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 8,
                                set_margin_all: 4,

                                gtk::Label {
                                    set_label: "Include attachments from:",
                                },
                            },
                        },
                    },
                },

                // Dialog buttons: Cancel / OK
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_halign: gtk::Align::End,
                    set_spacing: 8,

                    gtk::Button {
                        set_label: "Cancel",
                        connect_clicked => ManualSelectionMsg::Close,
                    },
                    gtk::Button {
                        set_label: "OK",
                        add_css_class: "suggested-action",
                        connect_clicked => ManualSelectionMsg::Accept,
                    },
                },
            },
        }
    }

    fn init(
        parent: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // Load CSS
        load_dialog_css();

        root.set_transient_for(Some(&parent));

        // Create the final ListBox
        let final_list_box = gtk::ListBox::new();
        final_list_box.set_selection_mode(gtk::SelectionMode::Single);
        final_list_box.set_show_separators(true);

        // Selection change signal on final list
        let sender_sel = sender.input_sender().clone();
        final_list_box.connect_row_selected(move |_lb, _row| {
            sender_sel.emit(ManualSelectionMsg::FinalSelectionChanged);
        });

        // Drop target on final list for adding tracks from source lists
        let drop_target = gtk::DropTarget::new(glib::types::Type::STRING, gdk::DragAction::COPY);
        let sender_drop = sender.input_sender().clone();
        drop_target.connect_drop(move |_target, value, _x, _y| {
            if let Ok(json_str) = value.get::<String>() {
                if let Ok(track_data) = serde_json::from_str::<SerializableTrackData>(&json_str) {
                    sender_drop.emit(ManualSelectionMsg::AddTrackToFinal(track_data.into()));
                    return true;
                }
            }
            false
        });
        final_list_box.add_controller(drop_target);

        // Context menu model for final list (reused for each right-click)
        let menu_model = {
            let m = gtk::gio::Menu::new();

            let move_section = gtk::gio::Menu::new();
            move_section.append(Some("Move Up"), Some("manual.move-up"));
            move_section.append(Some("Move Down"), Some("manual.move-down"));
            m.append_section(None, &move_section);

            let edit_section = gtk::gio::Menu::new();
            edit_section.append(
                Some("Edit Generated Track..."),
                Some("manual.edit-generated"),
            );
            edit_section.append(Some("Copy Style Edits"), Some("manual.copy-style"));
            edit_section.append(Some("Paste Style Edits"), Some("manual.paste-style"));
            m.append_section(None, &edit_section);

            let flag_section = gtk::gio::Menu::new();
            flag_section.append(Some("Make Default"), Some("manual.toggle-default"));
            flag_section.append(Some("Toggle Forced"), Some("manual.toggle-forced"));
            m.append_section(None, &flag_section);

            let delete_section = gtk::gio::Menu::new();
            delete_section.append(Some("Delete"), Some("manual.remove"));
            m.append_section(None, &delete_section);

            m
        };

        // Action group (attached to the list box, not the popover)
        let action_group = gtk::gio::SimpleActionGroup::new();
        {
            let s = sender.input_sender().clone();
            let a = gtk::gio::SimpleAction::new("move-up", None);
            a.connect_activate(move |_, _| s.emit(ManualSelectionMsg::MoveUp));
            action_group.add_action(&a);
        }
        {
            let s = sender.input_sender().clone();
            let a = gtk::gio::SimpleAction::new("move-down", None);
            a.connect_activate(move |_, _| s.emit(ManualSelectionMsg::MoveDown));
            action_group.add_action(&a);
        }
        {
            let s = sender.input_sender().clone();
            let a = gtk::gio::SimpleAction::new("toggle-default", None);
            a.connect_activate(move |_, _| s.emit(ManualSelectionMsg::ToggleDefault));
            action_group.add_action(&a);
        }
        {
            let s = sender.input_sender().clone();
            let a = gtk::gio::SimpleAction::new("toggle-forced", None);
            a.connect_activate(move |_, _| s.emit(ManualSelectionMsg::ToggleForced));
            action_group.add_action(&a);
        }
        {
            let s = sender.input_sender().clone();
            let a = gtk::gio::SimpleAction::new("remove", None);
            a.connect_activate(move |_, _| s.emit(ManualSelectionMsg::RemoveSelected));
            action_group.add_action(&a);
        }
        {
            let s = sender.input_sender().clone();
            let a = gtk::gio::SimpleAction::new("edit-generated", None);
            a.connect_activate(move |_, _| s.emit(ManualSelectionMsg::EditGeneratedTrack));
            action_group.add_action(&a);
        }
        {
            let s = sender.input_sender().clone();
            let a = gtk::gio::SimpleAction::new("copy-style", None);
            a.connect_activate(move |_, _| s.emit(ManualSelectionMsg::CopyStyleEdits));
            action_group.add_action(&a);
        }
        {
            let s = sender.input_sender().clone();
            let a = gtk::gio::SimpleAction::new("paste-style", None);
            a.connect_activate(move |_, _| s.emit(ManualSelectionMsg::PasteStyleEdits));
            action_group.add_action(&a);
        }
        final_list_box.insert_action_group("manual", Some(&action_group));

        // Right-click gesture — create popover on-demand each time
        // This avoids the set_parent issue with ListBox.remove_all()
        let gesture = gtk::GestureClick::new();
        gesture.set_button(3);
        let final_list_box_for_ctx = final_list_box.clone();
        let menu_model_rc = Rc::new(menu_model);
        // Store the current popover so we can clean it up
        let current_popover: Rc<RefCell<Option<gtk::PopoverMenu>>> = Rc::new(RefCell::new(None));
        let current_popover_for_gesture = current_popover.clone();
        gesture.connect_released(move |_gesture, _n, x, y| {
            // Unparent the previous popover if any
            if let Some(prev) = current_popover_for_gesture.borrow_mut().take() {
                prev.unparent();
            }
            let pop = gtk::PopoverMenu::from_model(Some(menu_model_rc.as_ref()));
            pop.set_has_arrow(false);
            pop.set_parent(&final_list_box_for_ctx);
            pop.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            pop.popup();
            *current_popover_for_gesture.borrow_mut() = Some(pop);
        });
        final_list_box.add_controller(gesture);

        // Keyboard shortcuts
        let key_controller = gtk::EventControllerKey::new();
        let sender_key = sender.input_sender().clone();
        key_controller.connect_key_pressed(move |_ctrl, key, _code, modifier| {
            let ctrl = modifier.contains(gdk::ModifierType::CONTROL_MASK);
            match key {
                gdk::Key::Delete => {
                    sender_key.emit(ManualSelectionMsg::RemoveSelected);
                    glib::Propagation::Stop
                }
                gdk::Key::Up if ctrl => {
                    sender_key.emit(ManualSelectionMsg::MoveUp);
                    glib::Propagation::Stop
                }
                gdk::Key::Down if ctrl => {
                    sender_key.emit(ManualSelectionMsg::MoveDown);
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        root.add_controller(key_controller);

        // Source pane, attachment box, external subtitle frame
        let source_pane = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let attachment_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let external_frame = gtk::Frame::new(Some("External Subtitles"));
        external_frame.set_visible(false);
        let external_list_box = gtk::ListBox::new();
        external_list_box.set_selection_mode(gtk::SelectionMode::Single);
        external_list_box.set_margin_all(4);
        external_frame.set_child(Some(&external_list_box));

        // "Add External Subtitle(s)..." button
        let add_ext_btn = gtk::Button::with_label("Add External Subtitle(s)...");
        let sender_ext = sender.input_sender().clone();
        add_ext_btn.connect_clicked(move |_| {
            sender_ext.emit(ManualSelectionMsg::AddExternalSubtitles);
        });

        let model = ManualSelectionDialog {
            visible: false,
            job_name: String::new(),
            source_tracks: Vec::new(),
            external_tracks: Vec::new(),
            source_has_attachments: HashMap::new(),
            final_tracks: Vec::new(),
            selected_final: None,
            attachment_checks: HashMap::new(),
            source_settings: HashMap::new(),
            info_text: String::new(),
            previous_layout: None,
            available_sources: Vec::new(),
            source_pane: source_pane.clone(),
            attachment_box: attachment_box.clone(),
            final_list_box: final_list_box.clone(),
            external_frame: external_frame.clone(),
            external_list_box: external_list_box.clone(),
            root_window: Some(root.clone()),
            style_edit_clipboard: None,
            final_list_dirty: Cell::new(false),
            context_popover: current_popover,
        };

        let widgets = view_output!();

        // Inject final ListBox into the scroll window
        widgets.final_scroll.set_child(Some(&final_list_box));

        // Inject source_pane, external subtitle button+frame into source scroll content
        widgets.source_scroll_content.append(&source_pane);
        widgets.source_scroll_content.append(&add_ext_btn);
        widgets.source_scroll_content.append(&external_frame);

        // Inject attachment_box into attachment content
        widgets.attachment_content.append(&attachment_box);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            ManualSelectionMsg::Show {
                sources,
                previous_layout,
                job_name,
            } => {
                self.job_name = job_name;
                self.previous_layout = previous_layout;
                self.info_text = "Probing source files...".to_string();
                self.visible = true;

                // Clear existing state
                self.final_tracks.clear();
                self.selected_final = None;
                self.source_tracks.clear();
                self.external_tracks.clear();
                self.source_has_attachments.clear();
                self.attachment_checks.clear();
                self.available_sources = sources.keys().cloned().collect();
                self.available_sources.sort();

                // Clear source pane children
                while let Some(child) = self.source_pane.first_child() {
                    self.source_pane.remove(&child);
                }
                // Clear attachment box children
                while let Some(child) = self.attachment_box.first_child() {
                    self.attachment_box.remove(&child);
                }
                // Clear final list
                self.clear_final_list_ui();
                // Hide external subtitle frame
                self.external_frame.set_visible(false);
                while let Some(child) = self.external_list_box.first_child() {
                    self.external_list_box.remove(&child);
                }
                // Probe files in background thread
                let sender_probe = sender.input_sender().clone();
                std::thread::spawn(move || {
                    let result = probe_sources(&sources);
                    match result {
                        Ok(probe_data) => {
                            sender_probe.emit(ManualSelectionMsg::ProbeComplete(probe_data));
                        }
                        Err(e) => {
                            sender_probe.emit(ManualSelectionMsg::ProbeFailed(e));
                        }
                    }
                });
            }

            ManualSelectionMsg::ProbeComplete(probe_data) => {
                self.source_tracks = probe_data.source_tracks;
                self.source_has_attachments = probe_data.source_has_attachments;
                self.info_text.clear();

                // Build source list panes
                self.build_source_panes(&sender);

                // Build attachment checkboxes
                self.build_attachment_checkboxes();

                // Prepopulate from previous layout if available
                if let Some(layout) = self.previous_layout.take() {
                    self.prepopulate_from_layout(&layout);
                    self.info_text =
                        "\u{2705} Pre-populated with the layout from the previous file."
                            .to_string();
                }

                self.rebuild_final_list_ui(&sender);
                let _ = sender.output(ManualSelectionOutput::Log(format!(
                    "Probed {} source(s) successfully.",
                    self.source_tracks.len()
                )));
            }

            ManualSelectionMsg::ProbeFailed(err) => {
                self.info_text = format!("Probing failed: {err}");
                let _ = sender.output(ManualSelectionOutput::Log(format!(
                    "[ERROR] Failed to probe sources: {err}"
                )));
            }

            ManualSelectionMsg::ReorderTrack { from, to } => {
                if from < self.final_tracks.len() && to < self.final_tracks.len() && from != to {
                    let track = self.final_tracks.remove(from);
                    self.final_tracks.insert(to, track);
                    self.selected_final = Some(to);
                    self.rebuild_final_list_ui(&sender);
                }
            }

            ManualSelectionMsg::Close => {
                self.visible = false;
            }

            ManualSelectionMsg::Accept => {
                let layout = self.build_final_layout();
                self.visible = false;
                let _ = sender.output(ManualSelectionOutput::Applied(layout));
            }

            ManualSelectionMsg::AddTrackToFinal(track_data) => {
                if track_data.is_blocked_video() {
                    return;
                }
                let ft = FinalTrack::new(track_data);
                self.final_tracks.push(ft);
                self.rebuild_final_list_ui(&sender);
            }

            ManualSelectionMsg::MoveUp => {
                if let Some(pos) = self.selected_final {
                    if pos > 0 {
                        self.final_tracks.swap(pos, pos - 1);
                        self.selected_final = Some(pos - 1);
                        self.rebuild_final_list_ui(&sender);
                    }
                }
            }

            ManualSelectionMsg::MoveDown => {
                if let Some(pos) = self.selected_final {
                    if pos + 1 < self.final_tracks.len() {
                        self.final_tracks.swap(pos, pos + 1);
                        self.selected_final = Some(pos + 1);
                        self.rebuild_final_list_ui(&sender);
                    }
                }
            }

            ManualSelectionMsg::RemoveSelected => {
                if let Some(pos) = self.selected_final {
                    if pos < self.final_tracks.len() {
                        self.final_tracks.remove(pos);
                        // Adjust selection
                        if self.final_tracks.is_empty() {
                            self.selected_final = None;
                        } else if pos >= self.final_tracks.len() {
                            self.selected_final = Some(self.final_tracks.len() - 1);
                        }
                        self.rebuild_final_list_ui(&sender);
                    }
                }
            }

            ManualSelectionMsg::ToggleDefault => {
                if let Some(pos) = self.selected_final {
                    if pos < self.final_tracks.len() {
                        let track_type = self.final_tracks[pos].data.track_type;
                        let new_default = !self.final_tracks[pos].is_default;
                        self.final_tracks[pos].is_default = new_default;

                        // If setting as default, unset all others of same type
                        if new_default {
                            for (i, ft) in self.final_tracks.iter_mut().enumerate() {
                                if i != pos && ft.data.track_type == track_type {
                                    ft.is_default = false;
                                }
                            }
                        }
                        self.rebuild_final_list_ui(&sender);
                    }
                }
            }

            ManualSelectionMsg::ToggleForced => {
                if let Some(pos) = self.selected_final {
                    if pos < self.final_tracks.len() {
                        if self.final_tracks[pos].data.track_type != TrackType::Subtitles {
                            return;
                        }
                        let new_forced = !self.final_tracks[pos].is_forced;
                        self.final_tracks[pos].is_forced = new_forced;

                        // If setting as forced, unset all other forced subtitles
                        if new_forced {
                            for (i, ft) in self.final_tracks.iter_mut().enumerate() {
                                if i != pos
                                    && ft.data.track_type == TrackType::Subtitles
                                    && ft.is_forced
                                {
                                    ft.is_forced = false;
                                }
                            }
                        }
                        self.rebuild_final_list_ui(&sender);
                    }
                }
            }

            ManualSelectionMsg::ToggleSetName => {
                if let Some(pos) = self.selected_final {
                    if pos < self.final_tracks.len() {
                        self.final_tracks[pos].apply_track_name =
                            !self.final_tracks[pos].apply_track_name;
                        self.rebuild_final_list_ui(&sender);
                    }
                }
            }

            ManualSelectionMsg::FinalSelectionChanged => {
                // Update selected_final from ListBox
                if let Some(row) = self.final_list_box.selected_row() {
                    self.selected_final = Some(row.index() as usize);
                } else {
                    self.selected_final = None;
                }
                self.final_list_dirty.set(true);
            }

            ManualSelectionMsg::AddExternalSubtitles => {
                let dialog = gtk::FileDialog::builder()
                    .title("Add External Subtitle(s)")
                    .modal(true)
                    .build();

                let filter = gtk::FileFilter::new();
                filter.set_name(Some("Subtitle Files"));
                filter.add_pattern("*.srt");
                filter.add_pattern("*.ass");
                filter.add_pattern("*.ssa");
                filter.add_pattern("*.sup");
                let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
                filters.append(&filter);

                let all_filter = gtk::FileFilter::new();
                all_filter.set_name(Some("All Files"));
                all_filter.add_pattern("*");
                filters.append(&all_filter);

                dialog.set_filters(Some(&filters));

                let sender_file = sender.input_sender().clone();
                let window = self.root_window.clone();
                dialog.open_multiple(
                    window.as_ref(),
                    None::<&gtk::gio::Cancellable>,
                    move |result| {
                        if let Ok(files) = result {
                            let paths: Vec<PathBuf> = (0..files.n_items())
                                .filter_map(|i| {
                                    files
                                        .item(i)
                                        .and_then(|f| f.downcast::<gtk::gio::File>().ok())
                                        .and_then(|f| f.path())
                                })
                                .collect();
                            if !paths.is_empty() {
                                sender_file.emit(ManualSelectionMsg::ExternalSubsSelected(paths));
                            }
                        }
                    },
                );
            }

            ManualSelectionMsg::ExternalSubsSelected(paths) => {
                for path in paths {
                    let ext = path
                        .extension()
                        .map(|e| e.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let codec_id = extension_to_codec_id(&ext).to_string();
                    let name_stem = path
                        .file_stem()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "subtitle".to_string());

                    let track_data = UiTrackData {
                        track_id: 0,
                        track_type: TrackType::Subtitles,
                        source_key: "External".to_string(),
                        codec_id: codec_id.clone(),
                        description: format!(
                            "{} ({})",
                            name_stem,
                            path.file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default()
                        ),
                        language: "und".to_string(),
                        name: Some(name_stem),
                        original_path: path,
                        is_default: false,
                        is_forced: false,
                        is_text_subtitle: is_text_codec(&codec_id),
                        is_generated: false,
                        generated_source_track_id: None,
                        needs_configuration: false,
                    };

                    // Add to external list UI
                    self.add_external_track_row(&track_data, &sender);
                    self.external_tracks.push(track_data);
                }
                self.external_frame
                    .set_visible(!self.external_tracks.is_empty());
            }

            // ── Stub actions ──
            ManualSelectionMsg::CreateSignsTrack(_track_data) => {
                // Stub: would create a generated track from a text subtitle
                self.info_text = "Create Signs Track: Not yet implemented.".to_string();
            }

            ManualSelectionMsg::OpenStyleEditor => {
                self.info_text = "Style Editor: Not yet implemented.".to_string();
            }

            ManualSelectionMsg::OpenTrackSettings => {
                self.info_text = "Track Settings: Not yet implemented.".to_string();
            }

            ManualSelectionMsg::OpenSourceSettings(source_key) => {
                self.info_text =
                    format!("Source Correlation Settings for {source_key}: Not yet implemented.");
            }

            ManualSelectionMsg::ClearSourceSettings(source_key) => {
                self.source_settings.remove(&source_key);
                self.info_text = format!("Correlation settings cleared for {source_key}.");
                self.rebuild_final_list_ui(&sender);
            }

            ManualSelectionMsg::CopyStyleEdits => {
                self.info_text = "Copy Style Edits: Not yet implemented.".to_string();
            }

            ManualSelectionMsg::PasteStyleEdits => {
                self.info_text = "Paste Style Edits: Not yet implemented.".to_string();
            }

            ManualSelectionMsg::EditGeneratedTrack => {
                self.info_text = "Edit Generated Track: Not yet implemented.".to_string();
            }
        }
    }

    fn post_view() {
        if self.final_list_dirty.get() {
            self.final_list_dirty.set(false);

            let has_sel = self.selected_final.is_some();
            let pos = self.selected_final.unwrap_or(0);
            let n = self.final_tracks.len();

            move_up_btn.set_sensitive(has_sel && pos > 0);
            move_down_btn.set_sensitive(has_sel && pos + 1 < n);
            remove_btn.set_sensitive(has_sel);
            default_btn.set_sensitive(has_sel);

            // Forced button only for subtitle tracks
            let is_sub = has_sel
                && pos < n
                && self.final_tracks[pos].data.track_type == TrackType::Subtitles;
            forced_btn.set_sensitive(is_sub);
        }
    }
}

impl ManualSelectionDialog {
    /// Clear the final list box UI.
    /// Unparents the context menu popover first, then removes all rows.
    fn clear_final_list_ui(&self) {
        // Unparent any existing context menu popover before clearing
        if let Some(popover) = self.context_popover.borrow_mut().take() {
            popover.unparent();
        }
        self.final_list_box.remove_all();
    }

    /// Rebuild the entire final list box UI from self.final_tracks.
    fn rebuild_final_list_ui(&self, sender: &ComponentSender<Self>) {
        self.clear_final_list_ui();

        for (i, ft) in self.final_tracks.iter().enumerate() {
            let row = self.build_track_widget_row(i, ft, sender);
            self.final_list_box.append(&row);
        }

        // Restore selection
        if let Some(sel) = self.selected_final {
            if let Some(row) = self.final_list_box.row_at_index(sel as i32) {
                self.final_list_box.select_row(Some(&row));
            }
        }

        self.final_list_dirty.set(true);
    }

    /// Build a single TrackWidget row for the final list.
    /// Matches Qt's TrackWidget: 2-row layout with summary/badges/controls.
    fn build_track_widget_row(
        &self,
        index: usize,
        ft: &FinalTrack,
        sender: &ComponentSender<Self>,
    ) -> gtk::ListBoxRow {
        let row = gtk::ListBoxRow::new();
        row.add_css_class("track-widget-row");

        let outer = gtk::Box::new(gtk::Orientation::Vertical, 2);
        outer.set_margin_top(5);
        outer.set_margin_bottom(5);
        outer.set_margin_start(5);
        outer.set_margin_end(5);

        // ── Top row: summary + badges + source label ──
        let top_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);

        let summary_label = gtk::Label::new(Some(&ft.summary_text()));
        summary_label.set_xalign(0.0);
        summary_label.set_hexpand(true);
        summary_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        summary_label.add_css_class("summary-label");

        let badge_text = ft.badges(&self.source_settings);
        let badge_label = gtk::Label::new(if badge_text.is_empty() {
            None
        } else {
            Some(badge_text.as_str())
        });
        badge_label.add_css_class("badge-label");
        badge_label.set_visible(!badge_text.is_empty());

        let inline = ft.inline_summary(&self.source_settings);
        let source_label = gtk::Label::new(if inline.is_empty() {
            None
        } else {
            Some(inline.as_str())
        });
        source_label.add_css_class("source-label-dim");
        source_label.set_xalign(1.0);
        source_label.set_visible(!inline.is_empty());

        top_row.append(&summary_label);
        top_row.append(&badge_label);
        top_row.append(&source_label);

        // ── Bottom row: controls ──
        let bottom_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);

        // Spacer to push controls right
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        bottom_row.append(&spacer);

        // Sync to source dropdown (only for External subtitles)
        if ft.data.source_key == "External" {
            let sync_label = gtk::Label::new(Some("Sync to Source:"));
            let mut items = vec!["Default (Source 1)".to_string()];
            for source in &self.available_sources {
                if source != "Source 1" {
                    items.push(source.clone());
                }
            }
            let items_arr: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
            let sync_dropdown = gtk::DropDown::from_strings(&items_arr);
            sync_dropdown.set_selected(0);
            bottom_row.append(&sync_label);
            bottom_row.append(&sync_dropdown);
        }

        // Default checkbox
        let cb_default = gtk::CheckButton::with_label("Default");
        cb_default.set_active(ft.is_default);
        {
            let sender_cb = sender.input_sender().clone();
            let idx = index;
            cb_default.connect_toggled(move |cb| {
                // We handle this by reading the checkbox state in the toggle message
                let _ = (cb, idx);
                sender_cb.emit(ManualSelectionMsg::ToggleDefault);
            });
        }

        // Forced checkbox (subtitles only)
        let cb_forced = gtk::CheckButton::with_label("Forced");
        cb_forced.set_active(ft.is_forced);
        cb_forced.set_visible(ft.data.track_type == TrackType::Subtitles);
        {
            let sender_cb = sender.input_sender().clone();
            cb_forced.connect_toggled(move |_| {
                sender_cb.emit(ManualSelectionMsg::ToggleForced);
            });
        }

        // Set Name checkbox
        let cb_name = gtk::CheckButton::with_label("Set Name");
        cb_name.set_active(ft.apply_track_name);
        {
            let sender_cb = sender.input_sender().clone();
            cb_name.connect_toggled(move |_| {
                sender_cb.emit(ManualSelectionMsg::ToggleSetName);
            });
        }

        // Style Editor button (subtitles only, stub)
        let style_btn = gtk::Button::with_label("Style Editor...");
        style_btn.set_visible(ft.data.track_type == TrackType::Subtitles);
        {
            let sender_btn = sender.input_sender().clone();
            style_btn.connect_clicked(move |_| {
                sender_btn.emit(ManualSelectionMsg::OpenStyleEditor);
            });
        }

        // Settings button (stub)
        let settings_btn = gtk::Button::with_label("Settings...");
        {
            let sender_btn = sender.input_sender().clone();
            settings_btn.connect_clicked(move |_| {
                sender_btn.emit(ManualSelectionMsg::OpenTrackSettings);
            });
        }

        bottom_row.append(&cb_default);
        bottom_row.append(&cb_forced);
        bottom_row.append(&cb_name);
        bottom_row.append(&style_btn);
        bottom_row.append(&settings_btn);

        outer.append(&top_row);
        outer.append(&bottom_row);

        // ── Drag source on final list row (for reordering) ──
        let drag_source = gtk::DragSource::new();
        drag_source.set_actions(gdk::DragAction::MOVE);
        let drag_index = index;
        drag_source.connect_prepare(move |_src, _x, _y| {
            Some(gdk::ContentProvider::for_value(
                &(drag_index as i64).to_value(),
            ))
        });
        row.add_controller(drag_source);

        // Drop target on final list row (for reordering)
        let drop_target = gtk::DropTarget::new(glib::types::Type::I64, gdk::DragAction::MOVE);
        let sender_reorder = sender.input_sender().clone();
        let target_index = index;
        drop_target.connect_drop(move |_target, value, _x, _y| {
            if let Ok(source_index) = value.get::<i64>() {
                let from = source_index as usize;
                let to = target_index;
                if from != to {
                    sender_reorder.emit(ManualSelectionMsg::ReorderTrack { from, to });
                }
                return true;
            }
            false
        });
        row.add_controller(drop_target);

        row.set_child(Some(&outer));
        row
    }

    /// Build the source list panes (one Frame per source with a ListBox of tracks).
    fn build_source_panes(&mut self, sender: &ComponentSender<Self>) {
        while let Some(child) = self.source_pane.first_child() {
            self.source_pane.remove(&child);
        }

        for (source_key, tracks) in &self.source_tracks {
            let filename = tracks
                .first()
                .map(|t| {
                    t.original_path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "N/A".to_string())
                })
                .unwrap_or_else(|| "N/A".to_string());

            let title = if source_key == "Source 1" {
                format!("{source_key} (Reference) Tracks ('{filename}')")
            } else {
                format!("{source_key} Tracks ('{filename}')")
            };

            let frame = gtk::Frame::new(Some(&title));
            let list_box = gtk::ListBox::new();
            list_box.set_selection_mode(gtk::SelectionMode::Single);
            list_box.set_margin_all(4);

            // Context menu for source group box (right-click on frame label)
            let frame_gesture = gtk::GestureClick::new();
            frame_gesture.set_button(3);
            let source_key_for_ctx = source_key.clone();
            let sender_frame = sender.input_sender().clone();
            let source_settings_ref = self.source_settings.clone();
            frame_gesture.connect_released(move |_gesture, _n, x, y| {
                let menu = gtk::gio::Menu::new();
                let has_settings = source_settings_ref.contains_key(&source_key_for_ctx);
                let configure_label = if has_settings {
                    "Configure Correlation Settings... (Modified)"
                } else {
                    "Configure Correlation Settings..."
                };
                menu.append(
                    Some(configure_label),
                    Some(&format!("source.configure-{}", source_key_for_ctx)),
                );
                if has_settings {
                    menu.append(
                        Some("Clear Source Settings"),
                        Some(&format!("source.clear-{}", source_key_for_ctx)),
                    );
                }

                // Use a simple approach: emit the message directly
                // For now, the context menu on frame is complex — just emit messages
                let _ = (x, y, &menu);
                sender_frame.emit(ManualSelectionMsg::OpenSourceSettings(
                    source_key_for_ctx.clone(),
                ));
            });
            frame.add_controller(frame_gesture);

            let tracks_rc = Rc::new(tracks.clone());

            for track in tracks {
                let row = gtk::ListBoxRow::new();
                let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                hbox.set_margin_all(4);

                let label = gtk::Label::new(Some(&track.source_list_label()));
                label.set_xalign(0.0);
                label.set_hexpand(true);
                label.set_ellipsize(gtk::pango::EllipsizeMode::End);

                let is_blocked = track.is_blocked_video();

                if is_blocked {
                    label.add_css_class("dim-label");
                    label.set_tooltip_text(Some(
                        "Video from other sources is disabled.\nOnly Source 1 video is allowed.",
                    ));
                    row.set_activatable(false);
                    row.set_sensitive(false);
                }

                hbox.append(&label);
                row.set_child(Some(&hbox));

                // Drag source on source list rows (for dragging to final list)
                if !is_blocked {
                    let drag_source = gtk::DragSource::new();
                    drag_source.set_actions(gdk::DragAction::COPY);
                    let track_json =
                        serde_json::to_string(&SerializableTrackData::from(track.clone()))
                            .unwrap_or_default();
                    drag_source.connect_prepare(move |_src, _x, _y| {
                        Some(gdk::ContentProvider::for_value(&track_json.to_value()))
                    });
                    row.add_controller(drag_source);
                }

                // Context menu for subtitle tracks
                if track.is_text_subtitle && !is_blocked {
                    let ctx_gesture = gtk::GestureClick::new();
                    ctx_gesture.set_button(3);
                    let track_for_ctx = track.clone();
                    let sender_ctx = sender.input_sender().clone();
                    ctx_gesture.connect_released(move |_gesture, _n, _x, _y| {
                        sender_ctx
                            .emit(ManualSelectionMsg::CreateSignsTrack(track_for_ctx.clone()));
                    });
                    row.add_controller(ctx_gesture);
                }

                list_box.append(&row);
            }

            // Double-click (row-activated) adds track to final list
            let sender_activate = sender.input_sender().clone();
            let tracks_for_activate = tracks_rc.clone();
            list_box.connect_row_activated(move |_lb, activated_row| {
                let idx = activated_row.index();
                if idx >= 0 && (idx as usize) < tracks_for_activate.len() {
                    let track = &tracks_for_activate[idx as usize];
                    if !track.is_blocked_video() {
                        sender_activate.emit(ManualSelectionMsg::AddTrackToFinal(track.clone()));
                    }
                }
            });

            frame.set_child(Some(&list_box));
            self.source_pane.append(&frame);
        }
    }

    /// Add a single external track row to the external list box.
    fn add_external_track_row(&self, track_data: &UiTrackData, sender: &ComponentSender<Self>) {
        let row = gtk::ListBoxRow::new();
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        hbox.set_margin_all(4);

        let label = gtk::Label::new(Some(&track_data.source_list_label()));
        label.set_xalign(0.0);
        label.set_hexpand(true);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);

        hbox.append(&label);
        row.set_child(Some(&hbox));

        // Drag source for external track
        let drag_source = gtk::DragSource::new();
        drag_source.set_actions(gdk::DragAction::COPY);
        let track_json = serde_json::to_string(&SerializableTrackData::from(track_data.clone()))
            .unwrap_or_default();
        drag_source.connect_prepare(move |_src, _x, _y| {
            Some(gdk::ContentProvider::for_value(&track_json.to_value()))
        });
        row.add_controller(drag_source);

        // Double-click to add
        let track_clone = track_data.clone();
        let sender_activate = sender.input_sender().clone();
        self.external_list_box
            .connect_row_activated(move |_lb, _row| {
                sender_activate.emit(ManualSelectionMsg::AddTrackToFinal(track_clone.clone()));
            });

        self.external_list_box.append(&row);
    }

    /// Build attachment checkboxes.
    fn build_attachment_checkboxes(&mut self) {
        while let Some(child) = self.attachment_box.first_child() {
            self.attachment_box.remove(&child);
        }
        self.attachment_checks.clear();

        for (source_key, _tracks) in &self.source_tracks {
            let has_attachments = self
                .source_has_attachments
                .get(source_key)
                .copied()
                .unwrap_or(false);

            let cb = gtk::CheckButton::with_label(source_key);
            cb.set_active(has_attachments);
            cb.set_sensitive(has_attachments);
            if !has_attachments {
                cb.set_tooltip_text(Some("No attachments in this source file."));
            }
            self.attachment_box.append(&cb);
            self.attachment_checks.insert(source_key.clone(), cb);
        }
    }

    /// Prepopulate the final list from a previous layout.
    fn prepopulate_from_layout(&mut self, layout: &ManualLayout) {
        // Build pools: (source_key, track_type, position) -> UiTrackData
        let mut pools: HashMap<(String, TrackType, usize), UiTrackData> = HashMap::new();
        let mut counters: HashMap<(String, TrackType), usize> = HashMap::new();

        for (source_key, tracks) in &self.source_tracks {
            for track in tracks {
                let key = (source_key.clone(), track.track_type);
                let idx = counters.get(&key).copied().unwrap_or(0);
                pools.insert((source_key.clone(), track.track_type, idx), track.clone());
                *counters.entry(key).or_insert(0) += 1;
            }
        }

        // Match layout entries to pool
        let mut match_counters: HashMap<(String, TrackType), usize> = HashMap::new();

        for entry in &layout.final_tracks {
            let key = (entry.source_key.clone(), entry.track_type);
            let idx = match_counters.get(&key).copied().unwrap_or(0);
            *match_counters.entry(key.clone()).or_insert(0) += 1;

            if let Some(track_data) = pools.get(&(entry.source_key.clone(), entry.track_type, idx))
            {
                if track_data.is_blocked_video() {
                    continue;
                }

                let mut ft = FinalTrack::new(track_data.clone());

                // Restore all config fields
                ft.is_default = entry.config.is_default;
                ft.is_forced = entry.config.is_forced_display;
                ft.apply_track_name = entry.config.apply_track_name;
                ft.custom_name = entry.config.custom_name.clone();
                ft.custom_lang = entry.config.custom_lang.clone();
                ft.sync_to_source = entry.config.sync_to_source.clone();
                ft.perform_ocr = entry.config.perform_ocr;
                ft.convert_to_ass = entry.config.convert_to_ass;
                ft.rescale = entry.config.rescale;
                ft.size_multiplier = entry.config.size_multiplier;
                ft.sync_exclusion_styles = entry.config.sync_exclusion_styles.clone();
                ft.sync_exclusion_mode = entry.config.sync_exclusion_mode.clone();
                ft.sync_exclusion_original_style_list =
                    entry.config.sync_exclusion_original_style_list.clone();
                ft.style_patch = entry.config.style_patch.clone();
                ft.font_replacements = entry.config.font_replacements.clone();

                // Generated track fields
                ft.is_generated = entry.is_generated;
                ft.generated_source_track_id = entry.generated_source_track_id;
                ft.generated_filter_mode = entry.generated_filter_mode.clone();
                ft.generated_filter_styles = entry.generated_filter_styles.clone();
                ft.generated_original_style_list = entry.generated_original_style_list.clone();

                self.final_tracks.push(ft);
            }
        }

        // Restore attachment checkboxes
        for source_key in &layout.attachment_sources {
            if let Some(cb) = self.attachment_checks.get(source_key) {
                cb.set_active(true);
            }
        }

        // Restore source correlation settings
        self.source_settings = layout.source_settings.clone();
    }

    /// Build the ManualLayout from the current final list state.
    fn build_final_layout(&self) -> ManualLayout {
        let mut tracks = Vec::new();
        let mut type_counters: HashMap<(String, TrackType), usize> = HashMap::new();

        for (i, ft) in self.final_tracks.iter().enumerate() {
            let key = (ft.data.source_key.clone(), ft.data.track_type);
            let pos = type_counters.get(&key).copied().unwrap_or(0);
            *type_counters.entry(key).or_insert(0) += 1;

            tracks.push(ft.to_entry(i, pos));
        }

        // Normalize: ensure exactly one default per audio type (force first if none)
        let mut has_audio_default = false;
        for entry in &tracks {
            if entry.track_type == TrackType::Audio && entry.config.is_default {
                has_audio_default = true;
                break;
            }
        }
        if !has_audio_default {
            for entry in &mut tracks {
                if entry.track_type == TrackType::Audio {
                    entry.config.is_default = true;
                    break;
                }
            }
        }

        let attachment_sources: Vec<String> = self
            .attachment_checks
            .iter()
            .filter(|(_, cb)| cb.is_active())
            .map(|(key, _)| key.clone())
            .collect();

        ManualLayout {
            final_tracks: tracks,
            attachment_sources,
            source_settings: self.source_settings.clone(),
        }
    }
}

// ─── Serializable track data for DnD ────────────────────────────────────────

/// Serializable version of UiTrackData for drag-and-drop JSON transfer.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SerializableTrackData {
    track_id: usize,
    track_type: String,
    source_key: String,
    codec_id: String,
    description: String,
    language: String,
    name: Option<String>,
    original_path: String,
    is_default: bool,
    is_forced: bool,
    is_text_subtitle: bool,
    is_generated: bool,
    generated_source_track_id: Option<usize>,
    needs_configuration: bool,
}

impl From<UiTrackData> for SerializableTrackData {
    fn from(t: UiTrackData) -> Self {
        Self {
            track_id: t.track_id,
            track_type: format!("{:?}", t.track_type),
            source_key: t.source_key,
            codec_id: t.codec_id,
            description: t.description,
            language: t.language,
            name: t.name,
            original_path: t.original_path.to_string_lossy().to_string(),
            is_default: t.is_default,
            is_forced: t.is_forced,
            is_text_subtitle: t.is_text_subtitle,
            is_generated: t.is_generated,
            generated_source_track_id: t.generated_source_track_id,
            needs_configuration: t.needs_configuration,
        }
    }
}

impl From<SerializableTrackData> for UiTrackData {
    fn from(s: SerializableTrackData) -> Self {
        let track_type = match s.track_type.as_str() {
            "Video" => TrackType::Video,
            "Audio" => TrackType::Audio,
            "Subtitles" => TrackType::Subtitles,
            _ => TrackType::Subtitles,
        };
        Self {
            track_id: s.track_id,
            track_type,
            source_key: s.source_key,
            codec_id: s.codec_id,
            description: s.description,
            language: s.language,
            name: s.name,
            original_path: PathBuf::from(s.original_path),
            is_default: s.is_default,
            is_forced: s.is_forced,
            is_text_subtitle: s.is_text_subtitle,
            is_generated: s.is_generated,
            generated_source_track_id: s.generated_source_track_id,
            needs_configuration: s.needs_configuration,
        }
    }
}

// ─── Probing logic (runs on background thread) ─────────────────────────────

/// Probe all sources and build UiTrackData for each.
fn probe_sources(sources: &HashMap<String, PathBuf>) -> Result<ProbeData, String> {
    let mut source_tracks = Vec::new();
    let mut source_has_attachments = HashMap::new();

    // Sort source keys naturally (Source 1, Source 2, Source 3...)
    let mut keys: Vec<String> = sources.keys().cloned().collect();
    keys.sort_by(|a, b| {
        let num_a = a
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u32>()
            .unwrap_or(0);
        let num_b = b
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u32>()
            .unwrap_or(0);
        num_a.cmp(&num_b)
    });

    for source_key in &keys {
        let path = &sources[source_key];
        let probe_result =
            probe_file(path).map_err(|e| format!("Failed to probe {}: {e}", path.display()))?;

        // Try to get detailed ffprobe info (optional — don't fail if ffprobe is missing)
        let ffprobe_info = get_detailed_stream_info(path).ok();

        let mut tracks = Vec::new();
        for track_info in &probe_result.tracks {
            let description = build_track_description(
                track_info,
                ffprobe_info.as_ref().and_then(|fi| fi.get(&track_info.id)),
            );

            let is_text_sub = track_info.properties.text_subtitles.unwrap_or(false);

            tracks.push(UiTrackData {
                track_id: track_info.id,
                track_type: convert_track_type(track_info.track_type),
                source_key: source_key.clone(),
                codec_id: track_info.codec_id.clone(),
                description,
                language: track_info
                    .language
                    .clone()
                    .unwrap_or_else(|| "und".to_string()),
                name: track_info.name.clone(),
                original_path: path.clone(),
                is_default: track_info.is_default,
                is_forced: track_info.is_forced,
                is_text_subtitle: is_text_sub,
                is_generated: false,
                generated_source_track_id: None,
                needs_configuration: false,
            });
        }

        source_has_attachments.insert(source_key.clone(), !probe_result.attachments.is_empty());
        source_tracks.push((source_key.clone(), tracks));
    }

    Ok(ProbeData {
        source_tracks,
        source_has_attachments,
    })
}
