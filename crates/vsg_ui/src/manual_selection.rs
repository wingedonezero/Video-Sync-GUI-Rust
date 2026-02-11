//! Manual Selection Dialog component.
//!
//! Matches the PySide ManualSelectionDialog layout:
//! - Left pane: scrollable source track lists (one per source, grouped in Frames)
//! - Right pane: final output list (drag-to-reorder, TrackWidget per row) + attachments
//! - OK/Cancel at bottom
//! - Double-click source track to add to final list
//! - Keyboard: Ctrl+Up/Down to move, Delete to remove from final list
//! - Video tracks from non-Source-1 are blocked
//! - Enforces single default per track type, single forced subtitle

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use gtk::glib;
use gtk::prelude::*;
use relm4::prelude::*;

use vsg_core::extraction::{
    build_track_description, get_detailed_stream_info, probe_file,
    TrackType as ExtractionTrackType,
};
use vsg_core::jobs::{FinalTrackEntry, ManualLayout, TrackConfig};
use vsg_core::models::TrackType;

/// Convert extraction TrackType to models TrackType.
fn convert_track_type(et: ExtractionTrackType) -> TrackType {
    match et {
        ExtractionTrackType::Video => TrackType::Video,
        ExtractionTrackType::Audio => TrackType::Audio,
        ExtractionTrackType::Subtitles => TrackType::Subtitles,
    }
}

// ─── Track data representation used in the UI ───────────────────────────────

/// Simplified track data for UI display and interaction.
/// Mirrors the Python track dict used in PySide.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct UiTrackData {
    /// Track ID within source file.
    pub track_id: usize,
    /// Track type.
    pub track_type: TrackType,
    /// Source key (e.g., "Source 1").
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
}

/// Get the type prefix for display (V, A, S).
fn track_type_prefix(tt: TrackType) -> &'static str {
    match tt {
        TrackType::Video => "V",
        TrackType::Audio => "A",
        TrackType::Subtitles => "S",
    }
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
#[derive(Debug, Clone)]
struct FinalTrack {
    /// The underlying track data.
    data: UiTrackData,
    /// User config options.
    is_default: bool,
    is_forced: bool,
    apply_track_name: bool,
    custom_name: Option<String>,
    custom_lang: Option<String>,
}

impl FinalTrack {
    fn new(data: UiTrackData) -> Self {
        Self {
            is_default: data.is_default,
            is_forced: data.is_forced,
            apply_track_name: false,
            custom_name: None,
            custom_lang: None,
            data,
        }
    }

    /// Build badge text for display.
    fn badges(&self) -> String {
        let mut badges = Vec::new();
        if self.is_default {
            badges.push("Default");
        }
        if self.data.track_type == TrackType::Subtitles && self.is_forced {
            badges.push("Forced");
        }
        if let Some(ref lang) = self.custom_lang {
            if !lang.is_empty() {
                badges.push("Lang");
            }
        }
        if let Some(ref name) = self.custom_name {
            if !name.is_empty() {
                badges.push("Named");
            }
        }
        badges.join(" | ")
    }

    /// Convert to backend FinalTrackEntry.
    fn to_entry(&self, order_index: usize, position_in_source_type: usize) -> FinalTrackEntry {
        let mut entry =
            FinalTrackEntry::new(self.data.track_id, self.data.source_key.clone(), self.data.track_type);
        entry.user_order_index = order_index;
        entry.position_in_source_type = position_in_source_type;
        entry.config = TrackConfig {
            is_default: self.is_default,
            is_forced_display: self.is_forced,
            apply_track_name: self.apply_track_name,
            custom_name: self.custom_name.clone(),
            custom_lang: self.custom_lang.clone(),
            ..TrackConfig::default()
        };
        entry
    }
}

// ─── GObject wrapper for final list items ───────────────────────────────────

mod final_item_imp {
    use super::*;
    use glib::subclass::prelude::*;

    #[derive(Default)]
    pub struct FinalTrackItemInner {
        pub(super) data: RefCell<Option<FinalTrack>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FinalTrackItemInner {
        const NAME: &'static str = "VsgFinalTrackItem";
        type Type = super::FinalTrackItem;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for FinalTrackItemInner {}
}

glib::wrapper! {
    pub struct FinalTrackItem(ObjectSubclass<final_item_imp::FinalTrackItemInner>);
}

impl FinalTrackItem {
    fn new(track: FinalTrack) -> Self {
        use gtk::glib::subclass::prelude::ObjectSubclassIsExt;
        let obj: Self = glib::Object::new();
        *obj.imp().data.borrow_mut() = Some(track);
        obj
    }

    fn data(&self) -> Option<FinalTrack> {
        use gtk::glib::subclass::prelude::ObjectSubclassIsExt;
        self.imp().data.borrow().clone()
    }

    fn set_data(&self, track: FinalTrack) {
        use gtk::glib::subclass::prelude::ObjectSubclassIsExt;
        *self.imp().data.borrow_mut() = Some(track);
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
    /// Add a track from the source list to the final list (double-click or button).
    AddTrackToFinal(UiTrackData),

    // ── Final list management ──
    /// Move selected final track up.
    MoveUp,
    /// Move selected final track down.
    MoveDown,
    /// Remove selected final track.
    RemoveSelected,
    /// Toggle default flag on selected final track.
    ToggleDefault,
    /// Toggle forced flag on selected final track (subtitles only).
    ToggleForced,
    /// Selection changed in final list.
    FinalSelectionChanged,

    // ── Internal ──
    /// Probing completed — populate the UI.
    ProbeComplete(ProbeData),
    /// Probing failed.
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
    /// Which sources have attachments.
    source_has_attachments: HashMap<String, bool>,
    /// Final list store.
    final_store: gtk::gio::ListStore,
    /// Final list selection model.
    final_selection: gtk::SingleSelection,
    /// Whether we need to refresh the final list display.
    final_list_dirty: Cell<bool>,
    /// Attachment checkboxes (source_key -> CheckButton widget).
    attachment_checks: HashMap<String, gtk::CheckButton>,
    /// Info label for status messages.
    info_text: String,
    /// Previous layout for prepopulation.
    previous_layout: Option<ManualLayout>,
    /// Container for source lists (built imperatively).
    source_pane: gtk::Box,
    /// Container for attachment checkboxes (built imperatively).
    attachment_box: gtk::Box,
    /// The final ListView widget (for context menu).
    #[allow(dead_code)]
    final_list_view: gtk::ListView,
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

                // Info label (top)
                #[name = "info_label"]
                gtk::Label {
                    set_halign: gtk::Align::Center,
                    add_css_class: "success",
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

                    // Left pane: source track lists
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
                            set_label: Some("Final Output (select and use buttons to reorder)"),
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
                        #[name = "attachment_frame"]
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
        root.set_transient_for(Some(&parent));

        // Build final list store and selection
        let final_store = gtk::gio::ListStore::new::<FinalTrackItem>();
        let final_selection = gtk::SingleSelection::new(Some(final_store.clone()));

        // Build final ListView with factory
        let factory = gtk::SignalListItemFactory::new();

        factory.connect_setup(|_factory, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
            // Two-line track widget: summary + badges on top, controls on bottom
            let outer = gtk::Box::new(gtk::Orientation::Vertical, 2);
            outer.set_margin_top(4);
            outer.set_margin_bottom(4);
            outer.set_margin_start(4);
            outer.set_margin_end(4);

            let top_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let summary = gtk::Label::new(None);
            summary.set_xalign(0.0);
            summary.set_hexpand(true);
            summary.set_ellipsize(gtk::pango::EllipsizeMode::End);
            summary.set_widget_name("track-summary");

            let badges = gtk::Label::new(None);
            badges.set_widget_name("track-badges");
            badges.add_css_class("accent");

            let source_label = gtk::Label::new(None);
            source_label.set_widget_name("track-source");
            source_label.set_xalign(1.0);

            top_row.append(&summary);
            top_row.append(&badges);
            top_row.append(&source_label);

            outer.append(&top_row);

            list_item.set_child(Some(&outer));
        });

        factory.connect_bind(|_factory, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
            let item = list_item
                .item()
                .and_downcast::<FinalTrackItem>()
                .expect("Item must be FinalTrackItem");

            if let Some(track) = item.data() {
                let outer = list_item
                    .child()
                    .and_downcast::<gtk::Box>()
                    .unwrap();
                let top_row = outer.first_child().and_downcast::<gtk::Box>().unwrap();

                // Find labels by name
                let mut child = top_row.first_child();
                while let Some(widget) = child {
                    if let Some(label) = widget.downcast_ref::<gtk::Label>() {
                        match label.widget_name().as_str() {
                            "track-summary" => {
                                label.set_label(&format!(
                                    "[{}] [{}] {}",
                                    track.data.source_key,
                                    track.data.source_list_label(),
                                    ""
                                ));
                            }
                            "track-badges" => {
                                let badge_text = track.badges();
                                label.set_label(&badge_text);
                                label.set_visible(!badge_text.is_empty());
                            }
                            "track-source" => {
                                label.set_label(&track.data.source_key);
                            }
                            _ => {}
                        }
                    }
                    child = widget.next_sibling();
                }
            }
        });

        let final_list_view = gtk::ListView::new(Some(final_selection.clone()), Some(factory));
        final_list_view.set_show_separators(true);

        // Selection change signal
        let sender_sel = sender.input_sender().clone();
        final_selection.connect_selection_changed(move |_sel, _pos, _n| {
            sender_sel.emit(ManualSelectionMsg::FinalSelectionChanged);
        });

        // Activate (double-click) on final list — no-op for now
        // (double-click on final list doesn't do anything in PySide either)

        // Context menu for final list
        let menu_model = gtk::gio::Menu::new();
        menu_model.append(Some("Move Up"), Some("manual.move-up"));
        menu_model.append(Some("Move Down"), Some("manual.move-down"));
        let section2 = gtk::gio::Menu::new();
        section2.append(Some("Toggle Default"), Some("manual.toggle-default"));
        section2.append(Some("Toggle Forced"), Some("manual.toggle-forced"));
        menu_model.append_section(None, &section2);
        let section3 = gtk::gio::Menu::new();
        section3.append(Some("Remove"), Some("manual.remove"));
        menu_model.append_section(None, &section3);

        let popover = gtk::PopoverMenu::from_model(Some(&menu_model));
        popover.set_parent(&final_list_view);
        popover.set_has_arrow(false);

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
        final_list_view.insert_action_group("manual", Some(&action_group));

        // Right-click gesture
        let gesture = gtk::GestureClick::new();
        gesture.set_button(3);
        let popover_rc = Rc::new(RefCell::new(popover));
        gesture.connect_released(move |_gesture, _n, x, y| {
            let pop = popover_rc.borrow();
            pop.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            pop.popup();
        });
        final_list_view.add_controller(gesture);

        // Keyboard shortcuts
        let key_controller = gtk::EventControllerKey::new();
        let sender_key = sender.input_sender().clone();
        key_controller.connect_key_pressed(move |_ctrl, key, _code, modifier| {
            let ctrl = modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK);
            match key {
                gtk::gdk::Key::Delete => {
                    sender_key.emit(ManualSelectionMsg::RemoveSelected);
                    glib::Propagation::Stop
                }
                gtk::gdk::Key::Up if ctrl => {
                    sender_key.emit(ManualSelectionMsg::MoveUp);
                    glib::Propagation::Stop
                }
                gtk::gdk::Key::Down if ctrl => {
                    sender_key.emit(ManualSelectionMsg::MoveDown);
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        root.add_controller(key_controller);

        // Source pane and attachment box — built imperatively, injected after view_output
        let source_pane = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let attachment_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);

        let model = ManualSelectionDialog {
            visible: false,
            job_name: String::new(),
            source_tracks: Vec::new(),
            source_has_attachments: HashMap::new(),
            final_store,
            final_selection,
            final_list_dirty: Cell::new(false),
            attachment_checks: HashMap::new(),
            info_text: String::new(),
            previous_layout: None,
            source_pane: source_pane.clone(),
            attachment_box: attachment_box.clone(),
            final_list_view: final_list_view.clone(),
        };

        let widgets = view_output!();

        // Inject the final ListView into the scroll window
        widgets.final_scroll.set_child(Some(&final_list_view));

        // Inject source_pane into source scroll content
        widgets.source_scroll_content.append(&source_pane);

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
                self.final_store.remove_all();
                self.source_tracks.clear();
                self.source_has_attachments.clear();
                self.attachment_checks.clear();

                // Clear source pane children
                while let Some(child) = self.source_pane.first_child() {
                    self.source_pane.remove(&child);
                }
                // Clear attachment box children
                while let Some(child) = self.attachment_box.first_child() {
                    self.attachment_box.remove(&child);
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
                        "Pre-populated with the layout from the previous configuration.".to_string();
                }

                self.final_list_dirty.set(true);
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
                self.final_store.append(&FinalTrackItem::new(ft));
                self.final_list_dirty.set(true);
            }

            ManualSelectionMsg::MoveUp => {
                let pos = self.final_selection.selected();
                if pos > 0 && pos < self.final_store.n_items() {
                    // Swap items at pos and pos-1
                    let item_a = self.final_store.item(pos - 1).unwrap();
                    let item_b = self.final_store.item(pos).unwrap();
                    let data_a = item_a.downcast_ref::<FinalTrackItem>().unwrap().data();
                    let data_b = item_b.downcast_ref::<FinalTrackItem>().unwrap().data();
                    if let (Some(a), Some(b)) = (data_a, data_b) {
                        self.final_store.remove(pos);
                        self.final_store.remove(pos - 1);
                        self.final_store.insert(pos - 1, &FinalTrackItem::new(b));
                        self.final_store.insert(pos, &FinalTrackItem::new(a));
                        self.final_selection.set_selected(pos - 1);
                    }
                    self.final_list_dirty.set(true);
                }
            }

            ManualSelectionMsg::MoveDown => {
                let pos = self.final_selection.selected();
                let n = self.final_store.n_items();
                if pos < n.saturating_sub(1) {
                    let item_a = self.final_store.item(pos).unwrap();
                    let item_b = self.final_store.item(pos + 1).unwrap();
                    let data_a = item_a.downcast_ref::<FinalTrackItem>().unwrap().data();
                    let data_b = item_b.downcast_ref::<FinalTrackItem>().unwrap().data();
                    if let (Some(a), Some(b)) = (data_a, data_b) {
                        self.final_store.remove(pos + 1);
                        self.final_store.remove(pos);
                        self.final_store.insert(pos, &FinalTrackItem::new(b));
                        self.final_store.insert(pos + 1, &FinalTrackItem::new(a));
                        self.final_selection.set_selected(pos + 1);
                    }
                    self.final_list_dirty.set(true);
                }
            }

            ManualSelectionMsg::RemoveSelected => {
                let pos = self.final_selection.selected();
                if pos < self.final_store.n_items() {
                    self.final_store.remove(pos);
                    self.final_list_dirty.set(true);
                }
            }

            ManualSelectionMsg::ToggleDefault => {
                let pos = self.final_selection.selected();
                if pos < self.final_store.n_items() {
                    let item = self.final_store.item(pos).unwrap();
                    let fi = item.downcast_ref::<FinalTrackItem>().unwrap();
                    if let Some(mut track) = fi.data() {
                        let track_type = track.data.track_type;
                        let new_default = !track.is_default;
                        track.is_default = new_default;
                        fi.set_data(track);

                        // If setting as default, unset all others of same type
                        if new_default {
                            for i in 0..self.final_store.n_items() {
                                if i == pos {
                                    continue;
                                }
                                let other = self.final_store.item(i).unwrap();
                                let ofi = other.downcast_ref::<FinalTrackItem>().unwrap();
                                if let Some(mut ot) = ofi.data() {
                                    if ot.data.track_type == track_type && ot.is_default {
                                        ot.is_default = false;
                                        ofi.set_data(ot);
                                    }
                                }
                            }
                        }
                        self.final_list_dirty.set(true);
                    }
                }
            }

            ManualSelectionMsg::ToggleForced => {
                let pos = self.final_selection.selected();
                if pos < self.final_store.n_items() {
                    let item = self.final_store.item(pos).unwrap();
                    let fi = item.downcast_ref::<FinalTrackItem>().unwrap();
                    if let Some(mut track) = fi.data() {
                        if track.data.track_type != TrackType::Subtitles {
                            return;
                        }
                        let new_forced = !track.is_forced;
                        track.is_forced = new_forced;
                        fi.set_data(track);

                        // If setting as forced, unset all other forced subtitles
                        if new_forced {
                            for i in 0..self.final_store.n_items() {
                                if i == pos {
                                    continue;
                                }
                                let other = self.final_store.item(i).unwrap();
                                let ofi = other.downcast_ref::<FinalTrackItem>().unwrap();
                                if let Some(mut ot) = ofi.data() {
                                    if ot.data.track_type == TrackType::Subtitles && ot.is_forced {
                                        ot.is_forced = false;
                                        ofi.set_data(ot);
                                    }
                                }
                            }
                        }
                        self.final_list_dirty.set(true);
                    }
                }
            }

            ManualSelectionMsg::FinalSelectionChanged => {
                self.final_list_dirty.set(true);
            }
        }
    }

    fn post_view() {
        if self.final_list_dirty.get() {
            self.final_list_dirty.set(false);

            let pos = self.final_selection.selected();
            let n = self.final_store.n_items();
            let has_sel = pos < n;

            move_up_btn.set_sensitive(has_sel && pos > 0);
            move_down_btn.set_sensitive(has_sel && pos + 1 < n);
            remove_btn.set_sensitive(has_sel);
            default_btn.set_sensitive(has_sel);

            // Forced button only for subtitle tracks
            let is_sub = has_sel
                && self
                    .final_store
                    .item(pos)
                    .and_then(|item| item.downcast_ref::<FinalTrackItem>().and_then(|fi| fi.data()))
                    .map(|t| t.data.track_type == TrackType::Subtitles)
                    .unwrap_or(false);
            forced_btn.set_sensitive(is_sub);

            // Force re-render of all items to update badges
            // GTK ListView doesn't have a simple "refresh" — we notify items changed
            self.final_store.items_changed(0, n, n);
        }
    }
}

impl ManualSelectionDialog {
    /// Build the source list panes (one Frame per source with a ListBox of tracks).
    fn build_source_panes(&mut self, sender: &ComponentSender<Self>) {
        // Clear previous
        while let Some(child) = self.source_pane.first_child() {
            self.source_pane.remove(&child);
        }

        for (source_key, tracks) in &self.source_tracks {
            // Build title with filename
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

            // Store tracks in Rc for shared access in activate handler
            let tracks_rc = Rc::new(tracks.clone());

            for track in tracks {
                let row = gtk::ListBoxRow::new();
                let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                hbox.set_margin_all(4);

                let label = gtk::Label::new(Some(&track.source_list_label()));
                label.set_xalign(0.0);
                label.set_hexpand(true);
                label.set_ellipsize(gtk::pango::EllipsizeMode::End);

                // Grey out blocked video tracks
                if track.is_blocked_video() {
                    label.add_css_class("dim-label");
                    label.set_tooltip_text(Some(
                        "Video from other sources is disabled.\nOnly Source 1 video is allowed.",
                    ));
                    row.set_activatable(false);
                    row.set_sensitive(false);
                }

                hbox.append(&label);
                row.set_child(Some(&hbox));
                list_box.append(&row);
            }

            // Connect row-activated once for this ListBox
            let sender_activate = sender.input_sender().clone();
            let tracks_for_activate = tracks_rc.clone();
            list_box.connect_row_activated(move |_lb, activated_row| {
                let idx = activated_row.index();
                if idx >= 0 && (idx as usize) < tracks_for_activate.len() {
                    let track = &tracks_for_activate[idx as usize];
                    if !track.is_blocked_video() {
                        sender_activate
                            .emit(ManualSelectionMsg::AddTrackToFinal(track.clone()));
                    }
                }
            });

            frame.set_child(Some(&list_box));
            self.source_pane.append(&frame);
        }
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
            cb.set_active(has_attachments); // Default: checked if source has attachments
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
                // Skip blocked video
                if track_data.is_blocked_video() {
                    continue;
                }

                let mut ft = FinalTrack::new(track_data.clone());
                // Apply saved config
                ft.is_default = entry.config.is_default;
                ft.is_forced = entry.config.is_forced_display;
                ft.apply_track_name = entry.config.apply_track_name;
                ft.custom_name = entry.config.custom_name.clone();
                ft.custom_lang = entry.config.custom_lang.clone();

                self.final_store.append(&FinalTrackItem::new(ft));
            }
        }

        // Restore attachment checkboxes
        for source_key in &layout.attachment_sources {
            if let Some(cb) = self.attachment_checks.get(source_key) {
                cb.set_active(true);
            }
        }
    }

    /// Build the ManualLayout from the current final list state.
    fn build_final_layout(&self) -> ManualLayout {
        let mut tracks = Vec::new();
        let mut type_counters: HashMap<(String, TrackType), usize> = HashMap::new();

        for i in 0..self.final_store.n_items() {
            let item = self.final_store.item(i).unwrap();
            let fi = item.downcast_ref::<FinalTrackItem>().unwrap();
            if let Some(track) = fi.data() {
                let key = (track.data.source_key.clone(), track.data.track_type);
                let pos = type_counters.get(&key).copied().unwrap_or(0);
                *type_counters.entry(key).or_insert(0) += 1;

                tracks.push(track.to_entry(i as usize, pos));
            }
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
            source_settings: HashMap::new(),
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
        let num_a = a.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse::<u32>().unwrap_or(0);
        let num_b = b.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse::<u32>().unwrap_or(0);
        num_a.cmp(&num_b)
    });

    for source_key in &keys {
        let path = &sources[source_key];

        let probe_result = probe_file(path)
            .map_err(|e| format!("Failed to probe {}: {e}", path.display()))?;

        // Try to get detailed ffprobe info too (optional — don't fail if ffprobe is missing)
        let ffprobe_info = get_detailed_stream_info(path).ok();

        let mut tracks = Vec::new();
        for track_info in &probe_result.tracks {
            let description = build_track_description(
                track_info,
                ffprobe_info
                    .as_ref()
                    .and_then(|fi| fi.get(&track_info.id)),
            );

            let is_text_sub = track_info
                .properties
                .text_subtitles
                .unwrap_or(false);

            tracks.push(UiTrackData {
                track_id: track_info.id,
                track_type: convert_track_type(track_info.track_type),
                source_key: source_key.clone(),
                codec_id: track_info.codec_id.clone(),
                description,
                language: track_info.language.clone().unwrap_or_else(|| "und".to_string()),
                name: track_info.name.clone(),
                original_path: path.clone(),
                is_default: track_info.is_default,
                is_forced: track_info.is_forced,
                is_text_subtitle: is_text_sub,
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
