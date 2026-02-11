//! Add Job dialog component.
//!
//! A dialog with dynamically-added source input rows (Source 1, Source 2, ...)
//! each with a text entry, browse button, and drag-and-drop support.
//! "Add Another Source" button adds more rows.
//! "Find & Add Jobs" validates and discovers jobs from the provided paths.
//! Matches the PySide AddJobDialog layout.

use std::collections::HashMap;
use std::path::PathBuf;

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use relm4::prelude::*;

use vsg_core::jobs::discover_jobs;

/// Messages for the add job dialog.
#[derive(Debug)]
pub enum AddJobMsg {
    /// Show the dialog (optionally pre-populated with dropped paths).
    Show(Vec<PathBuf>),
    /// Add another source input row.
    AddSourceRow,
    /// Browse for a source file (row index).
    BrowseSource(usize),
    /// A source path was selected from the file chooser.
    SourceSelected(usize, PathBuf),
    /// A file was dropped onto a source row.
    FileDropped(usize, PathBuf),
    /// Find & Add Jobs (validate + discover).
    FindAndAdd,
    /// Cancel / close.
    Cancel,
}

/// Output from add job dialog to parent.
#[derive(Debug)]
pub enum AddJobOutput {
    /// Jobs discovered and ready to add.
    JobsDiscovered(Vec<vsg_core::jobs::JobQueueEntry>),
    /// Log message.
    Log(String),
}

/// A single source input row's widget refs.
#[derive(Debug, Clone)]
struct SourceRow {
    entry: gtk::Entry,
    row_box: gtk::Box,
}

/// Add Job dialog state.
pub struct AddJobDialog {
    visible: bool,
    /// Source input rows (dynamically added).
    source_rows: Vec<SourceRow>,
    /// Container for source rows — built in init(), referenced in view via #[name].
    /// We build it before the model so the view macro can reference model.rows_container.
    rows_container: gtk::Box,
}

#[relm4::component(pub)]
impl SimpleComponent for AddJobDialog {
    type Init = gtk::Window;
    type Input = AddJobMsg;
    type Output = AddJobOutput;

    view! {
        #[root]
        gtk::Window {
            set_title: Some("Add Job(s) to Queue"),
            set_default_width: 700,
            set_default_height: 300,
            set_modal: true,
            #[watch]
            set_visible: model.visible,
            connect_close_request[sender] => move |_| {
                sender.input(AddJobMsg::Cancel);
                glib::Propagation::Stop
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 8,
                set_margin_all: 12,

                // Scrollable area for source inputs
                #[name = "scroll_area"]
                gtk::ScrolledWindow {
                    set_vexpand: true,
                    set_hscrollbar_policy: gtk::PolicyType::Never,
                    set_vscrollbar_policy: gtk::PolicyType::Automatic,
                },

                // Add Another Source button
                gtk::Button {
                    set_label: "Add Another Source",
                    connect_clicked => AddJobMsg::AddSourceRow,
                },

                // Button bar: Find & Add / Cancel
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_halign: gtk::Align::End,
                    set_spacing: 8,

                    gtk::Button {
                        set_label: "Cancel",
                        connect_clicked => AddJobMsg::Cancel,
                    },
                    gtk::Button {
                        set_label: "Find & Add Jobs",
                        add_css_class: "suggested-action",
                        connect_clicked => AddJobMsg::FindAndAdd,
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

        // Build the rows container BEFORE the model, so we own the widget
        let rows_container = gtk::Box::new(gtk::Orientation::Vertical, 6);

        let model = AddJobDialog {
            visible: false,
            source_rows: Vec::new(),
            rows_container: rows_container.clone(),
        };

        let widgets = view_output!();

        // Inject rows_container into the scroll area
        widgets.scroll_area.set_child(Some(&rows_container));

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            AddJobMsg::Show(pre_populate) => {
                // Clear existing rows
                self.clear_rows();

                if pre_populate.is_empty() {
                    // Start with 2 empty source inputs
                    self.add_source_row(&sender);
                    self.add_source_row(&sender);
                } else {
                    // Pre-populate with dropped paths
                    for path in &pre_populate {
                        self.add_source_row(&sender);
                        let last = self.source_rows.last().unwrap();
                        last.entry.set_text(&path.display().to_string());
                    }
                    // Ensure at least 2 rows
                    if self.source_rows.len() < 2 {
                        self.add_source_row(&sender);
                    }
                }

                self.visible = true;
            }
            AddJobMsg::Cancel => {
                self.visible = false;
            }
            AddJobMsg::AddSourceRow => {
                self.add_source_row(&sender);
            }
            AddJobMsg::BrowseSource(index) => {
                let sender = sender.clone();
                let parent = relm4::main_application().active_window();
                let dialog = gtk::FileDialog::builder()
                    .title(if index == 0 {
                        "Select Reference File"
                    } else {
                        "Select Source File"
                    })
                    .modal(true)
                    .build();

                dialog.open(
                    parent.as_ref(),
                    gtk::gio::Cancellable::NONE,
                    move |result| {
                        if let Ok(file) = result {
                            if let Some(path) = file.path() {
                                sender.input(AddJobMsg::SourceSelected(index, path));
                            }
                        }
                    },
                );
            }
            AddJobMsg::SourceSelected(index, path) | AddJobMsg::FileDropped(index, path) => {
                if let Some(row) = self.source_rows.get(index) {
                    row.entry.set_text(&path.display().to_string());
                }
            }
            AddJobMsg::FindAndAdd => {
                // Collect non-empty sources
                let mut sources: HashMap<String, PathBuf> = HashMap::new();
                for (i, row) in self.source_rows.iter().enumerate() {
                    let text = row.entry.text().to_string();
                    let text = text.trim().to_string();
                    if !text.is_empty() {
                        sources.insert(format!("Source {}", i + 1), PathBuf::from(&text));
                    }
                }

                if !sources.contains_key("Source 1") {
                    let _ = sender.output(AddJobOutput::Log(
                        "[ERROR] Source 1 (Reference) cannot be empty.".into(),
                    ));
                    return;
                }

                match discover_jobs(&sources) {
                    Ok(jobs) => {
                        if jobs.is_empty() {
                            let _ = sender.output(AddJobOutput::Log(
                                "No matching jobs found from provided paths.".into(),
                            ));
                        } else {
                            let count = jobs.len();
                            let _ = sender.output(AddJobOutput::JobsDiscovered(jobs));
                            let _ = sender.output(AddJobOutput::Log(format!(
                                "Discovered {count} job(s) from sources."
                            )));
                            self.visible = false;
                        }
                    }
                    Err(e) => {
                        let _ = sender.output(AddJobOutput::Log(format!(
                            "[ERROR] Job discovery failed: {e}"
                        )));
                    }
                }
            }
        }
    }
}

impl AddJobDialog {
    /// Clear all source rows.
    fn clear_rows(&mut self) {
        for row in &self.source_rows {
            self.rows_container.remove(&row.row_box);
        }
        self.source_rows.clear();
    }

    /// Add a new source input row.
    fn add_source_row(&mut self, sender: &ComponentSender<Self>) {
        let index = self.source_rows.len();
        let source_num = index + 1;

        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);

        let label_text = if source_num == 1 {
            format!("Source {source_num} (Reference):")
        } else {
            format!("Source {source_num}:")
        };
        let label = gtk::Label::new(Some(&label_text));
        label.set_xalign(0.0);
        label.set_width_request(180);

        let entry = gtk::Entry::new();
        entry.set_hexpand(true);
        entry.set_placeholder_text(Some(if source_num == 1 {
            "Path to reference file"
        } else {
            "Path to source file"
        }));

        let browse_btn = gtk::Button::with_label("Browse\u{2026}");
        let s = sender.input_sender().clone();
        browse_btn.connect_clicked(move |_| {
            s.emit(AddJobMsg::BrowseSource(index));
        });

        // Drag-and-drop per source entry
        let drop_target =
            gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
        let s = sender.input_sender().clone();
        drop_target.connect_drop(move |_target, value, _x, _y| {
            if let Ok(file_list) = value.get::<gdk::FileList>() {
                if let Some(file) = file_list.files().first() {
                    if let Some(path) = file.path() {
                        s.emit(AddJobMsg::FileDropped(index, path));
                        return true;
                    }
                }
            }
            false
        });
        entry.add_controller(drop_target);

        row_box.append(&label);
        row_box.append(&entry);
        row_box.append(&browse_btn);

        self.rows_container.append(&row_box);

        self.source_rows.push(SourceRow { entry, row_box });
    }
}
