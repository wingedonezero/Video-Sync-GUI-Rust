//! Settings dialog component.
//!
//! A separate window with 5 tabs matching the vsg_core Settings structure:
//! Paths, Logging, Analysis, Chapters, Post-Processing.
//!
//! Opens with a clone of Settings — Cancel discards, OK sends back to App.

use std::path::PathBuf;

use gtk::glib;
use gtk::prelude::*;
use relm4::prelude::*;

use vsg_core::config::Settings;
use vsg_core::models::{
    CorrelationMethod, DelaySelectionMode, FilteringMethod, SnapMode, SyncMode,
};

/// Which path field a folder browse result targets.
#[derive(Debug, Clone, Copy)]
pub enum PathField {
    OutputFolder,
    TempRoot,
    LogsFolder,
}

/// Messages for the settings dialog.
#[derive(Debug)]
pub enum SettingsMsg {
    /// Show dialog with current settings
    Show(Box<Settings>),
    /// OK — save and close
    Accept,
    /// Cancel — discard and close
    Cancel,
    /// Browse for a folder
    BrowseFolder(PathField),
    /// Folder selected from dialog
    FolderSelected(PathField, PathBuf),
}

/// Output from settings dialog to parent.
#[derive(Debug)]
pub enum SettingsOutput {
    /// User accepted — apply these settings
    Applied(Box<Settings>),
}

/// Settings dialog state.
pub struct SettingsDialog {
    settings: Settings,
    visible: bool,
    // Widget refs for fields built imperatively (dropdowns, spin buttons)
    // These are needed to repopulate on Show and read on Accept.
    output_folder_entry: gtk::Entry,
    temp_root_entry: gtk::Entry,
    logs_folder_entry: gtk::Entry,
    compact_check: gtk::CheckButton,
    autoscroll_check: gtk::CheckButton,
    archive_logs_check: gtk::CheckButton,
    show_options_pretty_check: gtk::CheckButton,
    show_options_json_check: gtk::CheckButton,
    error_tail_spin: gtk::SpinButton,
    progress_step_spin: gtk::SpinButton,
    correlation_method_dropdown: gtk::DropDown,
    lang_source1_entry: gtk::Entry,
    lang_others_entry: gtk::Entry,
    chunk_count_spin: gtk::SpinButton,
    chunk_duration_spin: gtk::SpinButton,
    scan_start_spin: gtk::SpinButton,
    scan_end_spin: gtk::SpinButton,
    min_match_pct_spin: gtk::SpinButton,
    min_accepted_chunks_spin: gtk::SpinButton,
    use_soxr_check: gtk::CheckButton,
    audio_peak_fit_check: gtk::CheckButton,
    filtering_method_dropdown: gtk::DropDown,
    filter_low_spin: gtk::SpinButton,
    filter_high_spin: gtk::SpinButton,
    multi_corr_enabled_check: gtk::CheckButton,
    multi_corr_scc_check: gtk::CheckButton,
    multi_corr_gcc_phat_check: gtk::CheckButton,
    multi_corr_gcc_scot_check: gtk::CheckButton,
    multi_corr_whitened_check: gtk::CheckButton,
    delay_selection_dropdown: gtk::DropDown,
    first_stable_min_chunks_spin: gtk::SpinButton,
    first_stable_skip_unstable_check: gtk::CheckButton,
    early_cluster_window_spin: gtk::SpinButton,
    early_cluster_threshold_spin: gtk::SpinButton,
    sync_mode_dropdown: gtk::DropDown,
    chapter_rename_check: gtk::CheckButton,
    snap_enabled_check: gtk::CheckButton,
    snap_mode_dropdown: gtk::DropDown,
    snap_threshold_spin: gtk::SpinButton,
    snap_starts_only_check: gtk::CheckButton,
    disable_track_stats_check: gtk::CheckButton,
    disable_header_compression_check: gtk::CheckButton,
    apply_dialog_norm_check: gtk::CheckButton,
}

// -- Helper functions for building form rows --

/// Create a labeled row with a widget.
fn label_row(label_text: &str, widget: &impl IsA<gtk::Widget>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let label = gtk::Label::new(Some(label_text));
    label.set_xalign(0.0);
    label.set_width_request(200);
    row.append(&label);
    row.append(widget);
    row
}

/// Create a labeled entry with a browse button.
fn path_row(
    label_text: &str,
    entry: &gtk::Entry,
    sender: &relm4::Sender<SettingsMsg>,
    field: PathField,
) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let label = gtk::Label::new(Some(label_text));
    label.set_xalign(0.0);
    label.set_width_request(200);
    entry.set_hexpand(true);
    let browse = gtk::Button::with_label("Browse...");
    let s = sender.clone();
    browse.connect_clicked(move |_| {
        s.emit(SettingsMsg::BrowseFolder(field));
    });
    row.append(&label);
    row.append(entry);
    row.append(&browse);
    row
}

/// Create a spin button for integer values.
fn spin_int(value: u32, min: f64, max: f64, step: f64) -> gtk::SpinButton {
    let adj = gtk::Adjustment::new(value as f64, min, max, step, step * 10.0, 0.0);
    let spin = gtk::SpinButton::new(Some(&adj), 1.0, 0);
    spin.set_hexpand(true);
    spin
}

/// Create a spin button for float values.
fn spin_float(value: f64, min: f64, max: f64, step: f64, digits: u32) -> gtk::SpinButton {
    let adj = gtk::Adjustment::new(value, min, max, step, step * 10.0, 0.0);
    let spin = gtk::SpinButton::new(Some(&adj), 1.0, digits);
    spin.set_hexpand(true);
    spin
}

/// Create a dropdown from an enum that has `all()` and `Display`.
fn enum_dropdown<T: std::fmt::Display>(variants: &[T], current_index: u32) -> gtk::DropDown {
    let names: Vec<String> = variants.iter().map(|v| v.to_string()).collect();
    let str_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    let dd = gtk::DropDown::from_strings(&str_refs);
    dd.set_selected(current_index);
    dd.set_hexpand(true);
    dd
}

/// Find the index of a value in an enum's `all()` list.
fn enum_index<T: PartialEq>(variants: &[T], value: &T) -> u32 {
    variants.iter().position(|v| v == value).unwrap_or(0) as u32
}

/// Create a frame with a vertical box inside.
fn framed_vbox(title: &str, spacing: i32) -> (gtk::Frame, gtk::Box) {
    let frame = gtk::Frame::new(Some(title));
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, spacing);
    vbox.set_margin_all(8);
    frame.set_child(Some(&vbox));
    (frame, vbox)
}

#[relm4::component(pub)]
impl SimpleComponent for SettingsDialog {
    type Init = gtk::Window;
    type Input = SettingsMsg;
    type Output = SettingsOutput;

    view! {
        #[root]
        gtk::Window {
            set_title: Some("Settings"),
            set_default_width: 750,
            set_default_height: 600,
            set_modal: true,
            #[watch]
            set_visible: model.visible,
            connect_close_request[sender] => move |_| {
                sender.input(SettingsMsg::Cancel);
                glib::Propagation::Stop
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 8,
                set_margin_all: 12,

                // Notebook placeholder — filled imperatively
                #[name = "notebook"]
                gtk::Notebook {
                    set_vexpand: true,
                },

                // Button bar
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_halign: gtk::Align::End,
                    set_spacing: 8,

                    gtk::Button {
                        set_label: "Cancel",
                        connect_clicked => SettingsMsg::Cancel,
                    },
                    gtk::Button {
                        set_label: "OK",
                        add_css_class: "suggested-action",
                        connect_clicked => SettingsMsg::Accept,
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

        let settings = Settings::default();
        let s = sender.input_sender();

        // ===== Build all widgets imperatively =====

        // --- Tab 1: Paths ---
        let output_folder_entry = gtk::Entry::new();
        let temp_root_entry = gtk::Entry::new();
        let logs_folder_entry = gtk::Entry::new();

        let paths_page = gtk::Box::new(gtk::Orientation::Vertical, 8);
        paths_page.set_margin_all(12);
        paths_page.append(&path_row(
            "Output Folder:",
            &output_folder_entry,
            s,
            PathField::OutputFolder,
        ));
        paths_page.append(&path_row(
            "Temp Root:",
            &temp_root_entry,
            s,
            PathField::TempRoot,
        ));
        paths_page.append(&path_row(
            "Logs Folder:",
            &logs_folder_entry,
            s,
            PathField::LogsFolder,
        ));

        // --- Tab 2: Logging ---
        let compact_check = gtk::CheckButton::with_label("Compact log format (filter progress)");
        let autoscroll_check = gtk::CheckButton::with_label("Auto-scroll log output");
        let archive_logs_check = gtk::CheckButton::with_label("Archive logs on completion");
        let show_options_pretty_check =
            gtk::CheckButton::with_label("Show mkvmerge options (pretty)");
        let show_options_json_check = gtk::CheckButton::with_label("Show mkvmerge options (JSON)");
        let error_tail_spin = spin_int(settings.logging.error_tail, 1.0, 100.0, 1.0);
        let progress_step_spin = spin_int(settings.logging.progress_step, 1.0, 100.0, 5.0);

        let logging_page = gtk::Box::new(gtk::Orientation::Vertical, 6);
        logging_page.set_margin_all(12);
        logging_page.append(&compact_check);
        logging_page.append(&autoscroll_check);
        logging_page.append(&archive_logs_check);
        logging_page.append(&show_options_pretty_check);
        logging_page.append(&show_options_json_check);
        logging_page.append(&label_row("Error tail lines:", &error_tail_spin));
        logging_page.append(&label_row("Progress step %:", &progress_step_spin));

        // --- Tab 3: Analysis ---
        let correlation_method_dropdown = enum_dropdown(
            CorrelationMethod::all(),
            enum_index(
                CorrelationMethod::all(),
                &settings.analysis.correlation_method,
            ),
        );
        let lang_source1_entry = gtk::Entry::new();
        lang_source1_entry.set_placeholder_text(Some("e.g. eng, jpn (empty = auto)"));
        lang_source1_entry.set_hexpand(true);
        let lang_others_entry = gtk::Entry::new();
        lang_others_entry.set_placeholder_text(Some("e.g. eng, jpn (empty = auto)"));
        lang_others_entry.set_hexpand(true);
        let chunk_count_spin = spin_int(settings.analysis.chunk_count, 1.0, 100.0, 1.0);
        let chunk_duration_spin = spin_int(settings.analysis.chunk_duration, 1.0, 120.0, 1.0);
        let scan_start_spin = spin_float(settings.analysis.scan_start_pct, 0.0, 100.0, 1.0, 1);
        let scan_end_spin = spin_float(settings.analysis.scan_end_pct, 0.0, 100.0, 1.0, 1);
        let min_match_pct_spin = spin_float(settings.analysis.min_match_pct, 0.0, 100.0, 0.5, 1);
        let min_accepted_chunks_spin =
            spin_int(settings.analysis.min_accepted_chunks, 1.0, 50.0, 1.0);
        let use_soxr_check = gtk::CheckButton::with_label("Use SOXR high-quality resampling");
        let audio_peak_fit_check =
            gtk::CheckButton::with_label("Quadratic peak fitting (sub-sample accuracy)");
        let filtering_method_dropdown = enum_dropdown(
            FilteringMethod::all(),
            enum_index(FilteringMethod::all(), &settings.analysis.filtering_method),
        );
        let filter_low_spin = spin_float(
            settings.analysis.filter_low_cutoff_hz,
            20.0,
            20000.0,
            10.0,
            0,
        );
        let filter_high_spin = spin_float(
            settings.analysis.filter_high_cutoff_hz,
            20.0,
            20000.0,
            10.0,
            0,
        );
        let multi_corr_enabled_check =
            gtk::CheckButton::with_label("Enable multi-correlation comparison");
        let multi_corr_scc_check = gtk::CheckButton::with_label("SCC");
        let multi_corr_gcc_phat_check = gtk::CheckButton::with_label("GCC-PHAT");
        let multi_corr_gcc_scot_check = gtk::CheckButton::with_label("GCC-SCOT");
        let multi_corr_whitened_check = gtk::CheckButton::with_label("Whitened");
        let delay_selection_dropdown = enum_dropdown(
            DelaySelectionMode::all(),
            enum_index(
                DelaySelectionMode::all(),
                &settings.analysis.delay_selection_mode,
            ),
        );
        let first_stable_min_chunks_spin =
            spin_int(settings.analysis.first_stable_min_chunks, 1.0, 50.0, 1.0);
        let first_stable_skip_unstable_check =
            gtk::CheckButton::with_label("Skip unstable segments");
        let early_cluster_window_spin =
            spin_int(settings.analysis.early_cluster_window, 1.0, 50.0, 1.0);
        let early_cluster_threshold_spin =
            spin_int(settings.analysis.early_cluster_threshold, 1.0, 50.0, 1.0);
        let sync_mode_dropdown = enum_dropdown(
            SyncMode::all(),
            enum_index(SyncMode::all(), &settings.analysis.sync_mode),
        );

        // Build the analysis page with sub-frames
        let analysis_scroll = gtk::ScrolledWindow::new();
        let analysis_page = gtk::Box::new(gtk::Orientation::Vertical, 8);
        analysis_page.set_margin_all(12);

        // Core
        let (core_frame, core_box) = framed_vbox("Correlation", 6);
        core_box.append(&label_row("Method:", &correlation_method_dropdown));
        analysis_page.append(&core_frame);

        // Language
        let (lang_frame, lang_box) = framed_vbox("Language Filters", 6);
        lang_box.append(&label_row("Source 1 language:", &lang_source1_entry));
        lang_box.append(&label_row("Other sources language:", &lang_others_entry));
        analysis_page.append(&lang_frame);

        // Chunks
        let (chunk_frame, chunk_box) = framed_vbox("Chunk Scanning", 6);
        chunk_box.append(&label_row("Chunk count:", &chunk_count_spin));
        chunk_box.append(&label_row("Chunk duration (s):", &chunk_duration_spin));
        chunk_box.append(&label_row("Scan start %:", &scan_start_spin));
        chunk_box.append(&label_row("Scan end %:", &scan_end_spin));
        analysis_page.append(&chunk_frame);

        // Match thresholds
        let (match_frame, match_box) = framed_vbox("Match Thresholds", 6);
        match_box.append(&label_row("Min match %:", &min_match_pct_spin));
        match_box.append(&label_row(
            "Min accepted chunks:",
            &min_accepted_chunks_spin,
        ));
        analysis_page.append(&match_frame);

        // Audio processing
        let (audio_frame, audio_box) = framed_vbox("Audio Processing", 6);
        audio_box.append(&use_soxr_check);
        audio_box.append(&audio_peak_fit_check);
        analysis_page.append(&audio_frame);

        // Filtering
        let (filter_frame, filter_box) = framed_vbox("Filtering", 6);
        filter_box.append(&label_row("Method:", &filtering_method_dropdown));
        filter_box.append(&label_row("Low cutoff (Hz):", &filter_low_spin));
        filter_box.append(&label_row("High cutoff (Hz):", &filter_high_spin));
        analysis_page.append(&filter_frame);

        // Multi-correlation
        let (multi_frame, multi_box) = framed_vbox("Multi-Correlation (Analyze Only)", 6);
        multi_box.append(&multi_corr_enabled_check);
        let multi_methods_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        multi_methods_box.set_margin_start(24);
        multi_methods_box.append(&multi_corr_scc_check);
        multi_methods_box.append(&multi_corr_gcc_phat_check);
        multi_methods_box.append(&multi_corr_gcc_scot_check);
        multi_methods_box.append(&multi_corr_whitened_check);
        multi_box.append(&multi_methods_box);
        analysis_page.append(&multi_frame);

        // Delay selection
        let (delay_frame, delay_box) = framed_vbox("Delay Selection", 6);
        delay_box.append(&label_row("Mode:", &delay_selection_dropdown));
        let (fs_frame, fs_box) = framed_vbox("First Stable Settings", 4);
        fs_box.append(&label_row(
            "Min consecutive chunks:",
            &first_stable_min_chunks_spin,
        ));
        fs_box.append(&first_stable_skip_unstable_check);
        delay_box.append(&fs_frame);
        let (ec_frame, ec_box) = framed_vbox("Early Cluster Settings", 4);
        ec_box.append(&label_row("Window size:", &early_cluster_window_spin));
        ec_box.append(&label_row("Threshold:", &early_cluster_threshold_spin));
        delay_box.append(&ec_frame);
        analysis_page.append(&delay_frame);

        // Sync
        let (sync_frame, sync_box) = framed_vbox("Sync Mode", 6);
        sync_box.append(&label_row("Mode:", &sync_mode_dropdown));
        analysis_page.append(&sync_frame);

        analysis_scroll.set_child(Some(&analysis_page));

        // --- Tab 4: Chapters ---
        let chapter_rename_check = gtk::CheckButton::with_label("Rename chapters");
        let snap_enabled_check = gtk::CheckButton::with_label("Snap chapters to keyframes");
        let snap_mode_dropdown = enum_dropdown(
            SnapMode::all(),
            enum_index(SnapMode::all(), &settings.chapters.snap_mode),
        );
        let snap_threshold_spin = spin_int(settings.chapters.snap_threshold_ms, 0.0, 5000.0, 50.0);
        let snap_starts_only_check = gtk::CheckButton::with_label("Snap starts only (not ends)");

        let chapters_page = gtk::Box::new(gtk::Orientation::Vertical, 6);
        chapters_page.set_margin_all(12);
        chapters_page.append(&chapter_rename_check);
        chapters_page.append(&snap_enabled_check);
        chapters_page.append(&label_row("Snap mode:", &snap_mode_dropdown));
        chapters_page.append(&label_row("Snap threshold (ms):", &snap_threshold_spin));
        chapters_page.append(&snap_starts_only_check);

        // --- Tab 5: Post-Processing ---
        let disable_track_stats_check =
            gtk::CheckButton::with_label("Disable track statistics tags");
        let disable_header_compression_check =
            gtk::CheckButton::with_label("Disable header compression");
        let apply_dialog_norm_check =
            gtk::CheckButton::with_label("Apply dialog normalization gain");

        let postprocess_page = gtk::Box::new(gtk::Orientation::Vertical, 6);
        postprocess_page.set_margin_all(12);
        postprocess_page.append(&disable_track_stats_check);
        postprocess_page.append(&disable_header_compression_check);
        postprocess_page.append(&apply_dialog_norm_check);

        // ===== Populate notebook =====
        let model = SettingsDialog {
            settings,
            visible: false,
            output_folder_entry,
            temp_root_entry,
            logs_folder_entry,
            compact_check,
            autoscroll_check,
            archive_logs_check,
            show_options_pretty_check,
            show_options_json_check,
            error_tail_spin,
            progress_step_spin,
            correlation_method_dropdown,
            lang_source1_entry,
            lang_others_entry,
            chunk_count_spin,
            chunk_duration_spin,
            scan_start_spin,
            scan_end_spin,
            min_match_pct_spin,
            min_accepted_chunks_spin,
            use_soxr_check,
            audio_peak_fit_check,
            filtering_method_dropdown,
            filter_low_spin,
            filter_high_spin,
            multi_corr_enabled_check,
            multi_corr_scc_check,
            multi_corr_gcc_phat_check,
            multi_corr_gcc_scot_check,
            multi_corr_whitened_check,
            delay_selection_dropdown,
            first_stable_min_chunks_spin,
            first_stable_skip_unstable_check,
            early_cluster_window_spin,
            early_cluster_threshold_spin,
            sync_mode_dropdown,
            chapter_rename_check,
            snap_enabled_check,
            snap_mode_dropdown,
            snap_threshold_spin,
            snap_starts_only_check,
            disable_track_stats_check,
            disable_header_compression_check,
            apply_dialog_norm_check,
        };

        let widgets = view_output!();

        // Add tabs to notebook
        widgets
            .notebook
            .append_page(&paths_page, Some(&gtk::Label::new(Some("Paths"))));
        widgets
            .notebook
            .append_page(&logging_page, Some(&gtk::Label::new(Some("Logging"))));
        widgets
            .notebook
            .append_page(&analysis_scroll, Some(&gtk::Label::new(Some("Analysis"))));
        widgets
            .notebook
            .append_page(&chapters_page, Some(&gtk::Label::new(Some("Chapters"))));
        widgets.notebook.append_page(
            &postprocess_page,
            Some(&gtk::Label::new(Some("Post-Processing"))),
        );

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            SettingsMsg::Show(settings) => {
                self.settings = *settings;
                self.populate_widgets();
                self.visible = true;
            }
            SettingsMsg::Accept => {
                self.read_widgets();
                self.visible = false;
                let _ = sender.output(SettingsOutput::Applied(Box::new(self.settings.clone())));
            }
            SettingsMsg::Cancel => {
                self.visible = false;
            }
            SettingsMsg::BrowseFolder(field) => {
                let sender = sender.clone();
                let parent = relm4::main_application().active_window();
                let dialog = gtk::FileDialog::builder()
                    .title(match field {
                        PathField::OutputFolder => "Select Output Folder",
                        PathField::TempRoot => "Select Temp Root",
                        PathField::LogsFolder => "Select Logs Folder",
                    })
                    .modal(true)
                    .build();

                dialog.select_folder(
                    parent.as_ref(),
                    gtk::gio::Cancellable::NONE,
                    move |result| {
                        if let Ok(file) = result {
                            if let Some(path) = file.path() {
                                sender.input(SettingsMsg::FolderSelected(field, path));
                            }
                        }
                    },
                );
            }
            SettingsMsg::FolderSelected(field, path) => {
                let path_str = path.display().to_string();
                match field {
                    PathField::OutputFolder => {
                        self.output_folder_entry.set_text(&path_str);
                    }
                    PathField::TempRoot => {
                        self.temp_root_entry.set_text(&path_str);
                    }
                    PathField::LogsFolder => {
                        self.logs_folder_entry.set_text(&path_str);
                    }
                }
            }
        }
    }
}

impl SettingsDialog {
    /// Push model settings into all widgets.
    fn populate_widgets(&self) {
        let s = &self.settings;

        // Paths
        self.output_folder_entry.set_text(&s.paths.output_folder);
        self.temp_root_entry.set_text(&s.paths.temp_root);
        self.logs_folder_entry.set_text(&s.paths.logs_folder);

        // Logging
        self.compact_check.set_active(s.logging.compact);
        self.autoscroll_check.set_active(s.logging.autoscroll);
        self.archive_logs_check.set_active(s.logging.archive_logs);
        self.show_options_pretty_check
            .set_active(s.logging.show_options_pretty);
        self.show_options_json_check
            .set_active(s.logging.show_options_json);
        self.error_tail_spin.set_value(s.logging.error_tail as f64);
        self.progress_step_spin
            .set_value(s.logging.progress_step as f64);

        // Analysis
        self.correlation_method_dropdown.set_selected(enum_index(
            CorrelationMethod::all(),
            &s.analysis.correlation_method,
        ));
        self.lang_source1_entry
            .set_text(s.analysis.lang_source1.as_deref().unwrap_or(""));
        self.lang_others_entry
            .set_text(s.analysis.lang_others.as_deref().unwrap_or(""));
        self.chunk_count_spin
            .set_value(s.analysis.chunk_count as f64);
        self.chunk_duration_spin
            .set_value(s.analysis.chunk_duration as f64);
        self.scan_start_spin.set_value(s.analysis.scan_start_pct);
        self.scan_end_spin.set_value(s.analysis.scan_end_pct);
        self.min_match_pct_spin.set_value(s.analysis.min_match_pct);
        self.min_accepted_chunks_spin
            .set_value(s.analysis.min_accepted_chunks as f64);
        self.use_soxr_check.set_active(s.analysis.use_soxr);
        self.audio_peak_fit_check
            .set_active(s.analysis.audio_peak_fit);
        self.filtering_method_dropdown.set_selected(enum_index(
            FilteringMethod::all(),
            &s.analysis.filtering_method,
        ));
        self.filter_low_spin
            .set_value(s.analysis.filter_low_cutoff_hz);
        self.filter_high_spin
            .set_value(s.analysis.filter_high_cutoff_hz);
        self.multi_corr_enabled_check
            .set_active(s.analysis.multi_correlation_enabled);
        self.multi_corr_scc_check
            .set_active(s.analysis.multi_corr_scc);
        self.multi_corr_gcc_phat_check
            .set_active(s.analysis.multi_corr_gcc_phat);
        self.multi_corr_gcc_scot_check
            .set_active(s.analysis.multi_corr_gcc_scot);
        self.multi_corr_whitened_check
            .set_active(s.analysis.multi_corr_whitened);
        self.delay_selection_dropdown.set_selected(enum_index(
            DelaySelectionMode::all(),
            &s.analysis.delay_selection_mode,
        ));
        self.first_stable_min_chunks_spin
            .set_value(s.analysis.first_stable_min_chunks as f64);
        self.first_stable_skip_unstable_check
            .set_active(s.analysis.first_stable_skip_unstable);
        self.early_cluster_window_spin
            .set_value(s.analysis.early_cluster_window as f64);
        self.early_cluster_threshold_spin
            .set_value(s.analysis.early_cluster_threshold as f64);
        self.sync_mode_dropdown
            .set_selected(enum_index(SyncMode::all(), &s.analysis.sync_mode));

        // Chapters
        self.chapter_rename_check.set_active(s.chapters.rename);
        self.snap_enabled_check.set_active(s.chapters.snap_enabled);
        self.snap_mode_dropdown
            .set_selected(enum_index(SnapMode::all(), &s.chapters.snap_mode));
        self.snap_threshold_spin
            .set_value(s.chapters.snap_threshold_ms as f64);
        self.snap_starts_only_check
            .set_active(s.chapters.snap_starts_only);

        // Post-processing
        self.disable_track_stats_check
            .set_active(s.postprocess.disable_track_stats_tags);
        self.disable_header_compression_check
            .set_active(s.postprocess.disable_header_compression);
        self.apply_dialog_norm_check
            .set_active(s.postprocess.apply_dialog_norm);

        // Update sensitivity for conditional fields
        self.update_sensitivity();
    }

    /// Read all widget values back into model settings.
    fn read_widgets(&mut self) {
        let s = &mut self.settings;

        // Paths
        s.paths.output_folder = self.output_folder_entry.text().to_string();
        s.paths.temp_root = self.temp_root_entry.text().to_string();
        s.paths.logs_folder = self.logs_folder_entry.text().to_string();

        // Logging
        s.logging.compact = self.compact_check.is_active();
        s.logging.autoscroll = self.autoscroll_check.is_active();
        s.logging.archive_logs = self.archive_logs_check.is_active();
        s.logging.show_options_pretty = self.show_options_pretty_check.is_active();
        s.logging.show_options_json = self.show_options_json_check.is_active();
        s.logging.error_tail = self.error_tail_spin.value() as u32;
        s.logging.progress_step = self.progress_step_spin.value() as u32;

        // Analysis
        let cm_idx = self.correlation_method_dropdown.selected() as usize;
        s.analysis.correlation_method = CorrelationMethod::all()
            .get(cm_idx)
            .copied()
            .unwrap_or_default();
        let lang1 = self.lang_source1_entry.text().to_string();
        s.analysis.lang_source1 = if lang1.is_empty() { None } else { Some(lang1) };
        let lang_o = self.lang_others_entry.text().to_string();
        s.analysis.lang_others = if lang_o.is_empty() {
            None
        } else {
            Some(lang_o)
        };
        s.analysis.chunk_count = self.chunk_count_spin.value() as u32;
        s.analysis.chunk_duration = self.chunk_duration_spin.value() as u32;
        s.analysis.scan_start_pct = self.scan_start_spin.value();
        s.analysis.scan_end_pct = self.scan_end_spin.value();
        s.analysis.min_match_pct = self.min_match_pct_spin.value();
        s.analysis.min_accepted_chunks = self.min_accepted_chunks_spin.value() as u32;
        s.analysis.use_soxr = self.use_soxr_check.is_active();
        s.analysis.audio_peak_fit = self.audio_peak_fit_check.is_active();
        let fm_idx = self.filtering_method_dropdown.selected() as usize;
        s.analysis.filtering_method = FilteringMethod::all()
            .get(fm_idx)
            .copied()
            .unwrap_or_default();
        s.analysis.filter_low_cutoff_hz = self.filter_low_spin.value();
        s.analysis.filter_high_cutoff_hz = self.filter_high_spin.value();
        s.analysis.multi_correlation_enabled = self.multi_corr_enabled_check.is_active();
        s.analysis.multi_corr_scc = self.multi_corr_scc_check.is_active();
        s.analysis.multi_corr_gcc_phat = self.multi_corr_gcc_phat_check.is_active();
        s.analysis.multi_corr_gcc_scot = self.multi_corr_gcc_scot_check.is_active();
        s.analysis.multi_corr_whitened = self.multi_corr_whitened_check.is_active();
        let ds_idx = self.delay_selection_dropdown.selected() as usize;
        s.analysis.delay_selection_mode = DelaySelectionMode::all()
            .get(ds_idx)
            .copied()
            .unwrap_or_default();
        s.analysis.first_stable_min_chunks = self.first_stable_min_chunks_spin.value() as u32;
        s.analysis.first_stable_skip_unstable = self.first_stable_skip_unstable_check.is_active();
        s.analysis.early_cluster_window = self.early_cluster_window_spin.value() as u32;
        s.analysis.early_cluster_threshold = self.early_cluster_threshold_spin.value() as u32;
        let sm_idx = self.sync_mode_dropdown.selected() as usize;
        s.analysis.sync_mode = SyncMode::all().get(sm_idx).copied().unwrap_or_default();

        // Chapters
        s.chapters.rename = self.chapter_rename_check.is_active();
        s.chapters.snap_enabled = self.snap_enabled_check.is_active();
        let snap_idx = self.snap_mode_dropdown.selected() as usize;
        s.chapters.snap_mode = SnapMode::all().get(snap_idx).copied().unwrap_or_default();
        s.chapters.snap_threshold_ms = self.snap_threshold_spin.value() as u32;
        s.chapters.snap_starts_only = self.snap_starts_only_check.is_active();

        // Post-processing
        s.postprocess.disable_track_stats_tags = self.disable_track_stats_check.is_active();
        s.postprocess.disable_header_compression =
            self.disable_header_compression_check.is_active();
        s.postprocess.apply_dialog_norm = self.apply_dialog_norm_check.is_active();
    }

    /// Update widget sensitivity for conditional fields.
    fn update_sensitivity(&self) {
        let multi_enabled = self.multi_corr_enabled_check.is_active();
        self.multi_corr_scc_check.set_sensitive(multi_enabled);
        self.multi_corr_gcc_phat_check.set_sensitive(multi_enabled);
        self.multi_corr_gcc_scot_check.set_sensitive(multi_enabled);
        self.multi_corr_whitened_check.set_sensitive(multi_enabled);
    }
}
