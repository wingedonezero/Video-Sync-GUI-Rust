//! Job Queue dialog component.
//!
//! Matches the PySide JobQueueDialog layout:
//! - Full-width table with 3 columns: #, Status, Sources
//! - Horizontal button row below table: Add Job(s)..., Move Up, Move Down, Remove Selected
//! - Dialog-style bottom: "Start Processing Queue" / Cancel
//! - Context menu: Configure..., Remove, Copy Layout, Paste Layout
//! - Drag-and-drop files opens AddJobDialog pre-populated
//! - Double-click to configure
//! - Keyboard shortcuts: Ctrl+Up/Down to move, Delete to remove

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use gtk::gdk;
use gtk::glib;
use gtk::glib::subclass::prelude::ObjectSubclassIsExt;
use gtk::prelude::*;
use relm4::prelude::*;

use vsg_core::jobs::{JobQueue, JobQueueEntry, JobQueueStatus};

use crate::add_job_dialog::{AddJobDialog, AddJobMsg, AddJobOutput};
use crate::manual_selection::{ManualSelectionDialog, ManualSelectionMsg, ManualSelectionOutput};

/// Messages for the job queue dialog.
#[derive(Debug)]
pub enum JobQueueMsg {
    /// Show the dialog.
    Show,
    /// Close the dialog (Cancel).
    Close,
    /// Open Add Job dialog (with optional pre-populated paths from DnD).
    OpenAddJobDialog(Vec<PathBuf>),
    /// Jobs discovered from the add job dialog.
    JobsDiscovered(Vec<JobQueueEntry>),
    /// Move selected jobs up.
    MoveUp,
    /// Move selected jobs down.
    MoveDown,
    /// Remove selected jobs.
    RemoveSelected,
    /// Configure selected job (double-click or context menu).
    ConfigureSelected,
    /// Copy layout from selected job.
    CopyLayout,
    /// Paste layout to selected jobs.
    PasteLayout,
    /// Start processing the queue (OK button).
    StartProcessing,
    /// Selection changed — triggers sensitivity update.
    SelectionChanged,
    /// Log message (forwarded from child dialogs).
    Log(String),
    /// Manual selection dialog returned a layout (uses configuring_job_id to find the job).
    LayoutApplied(vsg_core::jobs::ManualLayout),
}

/// Output from job queue dialog to parent.
#[derive(Debug)]
pub enum JobQueueOutput {
    /// User accepted — start processing these jobs.
    StartProcessing(Vec<JobQueueEntry>),
    /// Log message to parent.
    Log(String),
}

/// GObject wrapper for a job row in the ColumnView list store.
#[derive(Debug, Clone)]
pub(crate) struct JobRow {
    pub index: u32,
    pub status: String,
    pub sources: String,
    #[allow(dead_code)]
    pub job_id: String,
}

mod imp {
    use super::*;
    use glib::subclass::prelude::*;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct JobRowObjectInner {
        pub(super) data: RefCell<Option<JobRow>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for JobRowObjectInner {
        const NAME: &'static str = "VsgJobRowObject";
        type Type = super::JobRowObject;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for JobRowObjectInner {}
}

glib::wrapper! {
    pub struct JobRowObject(ObjectSubclass<imp::JobRowObjectInner>);
}

impl JobRowObject {
    pub(crate) fn new(row: JobRow) -> Self {
        let obj: Self = glib::Object::new();
        *obj.imp().data.borrow_mut() = Some(row);
        obj
    }

    pub(crate) fn data(&self) -> JobRow {
        self.imp().data.borrow().clone().unwrap_or(JobRow {
            index: 0,
            status: String::new(),
            sources: String::new(),
            job_id: String::new(),
        })
    }
}

/// Format sources map into a display string (filename only, joined with " + ").
fn format_sources_short(sources: &HashMap<String, PathBuf>) -> String {
    let mut keys: Vec<&String> = sources.keys().collect();
    keys.sort();
    let names: Vec<String> = keys
        .iter()
        .filter_map(|key| {
            sources.get(*key).map(|path| {
                path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string())
            })
        })
        .collect();
    names.join(" + ")
}

/// Create a ColumnView factory for a text column.
fn text_column_factory(
    get_text: impl Fn(&JobRow) -> String + 'static,
) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    let get_text = Rc::new(get_text);
    let get_text_bind = get_text.clone();

    factory.connect_setup(|_factory, list_item| {
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
        list_item.set_child(Some(&label));
    });

    factory.connect_bind(move |_factory, list_item| {
        let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
        let item = list_item
            .item()
            .and_downcast::<JobRowObject>()
            .expect("Item must be JobRowObject");
        let label = list_item
            .child()
            .and_downcast::<gtk::Label>()
            .expect("Child must be Label");
        label.set_label(&get_text_bind(&item.data()));
    });

    factory
}

/// Job Queue dialog state.
pub struct JobQueueDialog {
    visible: bool,
    /// The actual job queue (backend).
    queue: JobQueue,
    /// ListStore backing the ColumnView.
    list_store: gtk::gio::ListStore,
    /// Selection model.
    selection_model: gtk::MultiSelection,
    /// Track if we need a sensitivity update (Cell for post_view access).
    sensitivity_dirty: Cell<bool>,
    /// Add Job sub-dialog.
    add_job_dialog: Controller<AddJobDialog>,
    /// Manual Selection sub-dialog.
    manual_selection_dialog: Controller<ManualSelectionDialog>,
    /// ID of the job currently being configured (for LayoutApplied).
    configuring_job_id: Option<String>,
}

#[relm4::component(pub)]
impl SimpleComponent for JobQueueDialog {
    type Init = gtk::Window;
    type Input = JobQueueMsg;
    type Output = JobQueueOutput;

    view! {
        #[root]
        gtk::Window {
            set_title: Some("Job Queue"),
            set_default_width: 1200,
            set_default_height: 600,
            set_modal: false,
            #[watch]
            set_visible: model.visible,
            connect_close_request[sender] => move |_| {
                sender.input(JobQueueMsg::Close);
                glib::Propagation::Stop
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 6,
                set_margin_all: 8,

                // Table (scrollable, full width)
                #[name = "scroll_window"]
                gtk::ScrolledWindow {
                    set_hexpand: true,
                    set_vexpand: true,
                    set_hscrollbar_policy: gtk::PolicyType::Automatic,
                    set_vscrollbar_policy: gtk::PolicyType::Automatic,
                },

                // Horizontal button row below table
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 6,

                    gtk::Button {
                        set_label: "Add Job(s)...",
                        connect_clicked => JobQueueMsg::OpenAddJobDialog(Vec::new()),
                    },

                    // Spacer
                    gtk::Box {
                        set_hexpand: true,
                    },

                    #[name = "move_up_btn"]
                    gtk::Button {
                        set_label: "Move Up",
                        set_sensitive: false,
                        connect_clicked => JobQueueMsg::MoveUp,
                    },

                    #[name = "move_down_btn"]
                    gtk::Button {
                        set_label: "Move Down",
                        set_sensitive: false,
                        connect_clicked => JobQueueMsg::MoveDown,
                    },

                    #[name = "remove_btn"]
                    gtk::Button {
                        set_label: "Remove Selected",
                        set_sensitive: false,
                        connect_clicked => JobQueueMsg::RemoveSelected,
                    },
                },

                // Dialog-style bottom: Start Processing Queue / Cancel
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_halign: gtk::Align::End,
                    set_spacing: 8,

                    gtk::Button {
                        set_label: "Cancel",
                        connect_clicked => JobQueueMsg::Close,
                    },

                    #[name = "start_btn"]
                    gtk::Button {
                        set_label: "Start Processing Queue",
                        set_sensitive: false,
                        add_css_class: "suggested-action",
                        connect_clicked => JobQueueMsg::StartProcessing,
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

        // Build list store and selection model
        let list_store = gtk::gio::ListStore::new::<JobRowObject>();
        let selection_model = gtk::MultiSelection::new(Some(list_store.clone()));

        // Build ColumnView
        let column_view = gtk::ColumnView::new(Some(selection_model.clone()));
        column_view.set_show_row_separators(true);
        column_view.set_show_column_separators(true);
        column_view.set_reorderable(false);

        // Column 1: # (row number)
        let num_factory = text_column_factory(|row| format!("{}", row.index + 1));
        let num_column = gtk::ColumnViewColumn::new(Some("#"), Some(num_factory));
        num_column.set_fixed_width(50);
        num_column.set_resizable(false);
        column_view.append_column(&num_column);

        // Column 2: Status
        let status_factory = text_column_factory(|row| row.status.clone());
        let status_column = gtk::ColumnViewColumn::new(Some("Status"), Some(status_factory));
        status_column.set_fixed_width(180);
        status_column.set_resizable(true);
        column_view.append_column(&status_column);

        // Column 3: Sources
        let sources_factory = text_column_factory(|row| row.sources.clone());
        let sources_column = gtk::ColumnViewColumn::new(Some("Sources"), Some(sources_factory));
        sources_column.set_expand(true);
        sources_column.set_resizable(true);
        column_view.append_column(&sources_column);

        // Selection change → update button sensitivity
        let sender_sel = sender.input_sender().clone();
        selection_model.connect_selection_changed(move |_model, _pos, _n| {
            sender_sel.emit(JobQueueMsg::SelectionChanged);
        });

        // Double-click → configure
        let sender_dbl = sender.input_sender().clone();
        column_view.connect_activate(move |_cv, _pos| {
            sender_dbl.emit(JobQueueMsg::ConfigureSelected);
        });

        // Drag-and-drop on column view → open AddJobDialog pre-populated
        let drop_target = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
        let sender_drop = sender.input_sender().clone();
        drop_target.connect_drop(move |_target, value, _x, _y| {
            if let Ok(file_list) = value.get::<gdk::FileList>() {
                let paths: Vec<PathBuf> =
                    file_list.files().iter().filter_map(|f| f.path()).collect();
                if !paths.is_empty() {
                    sender_drop.emit(JobQueueMsg::OpenAddJobDialog(paths));
                    return true;
                }
            }
            false
        });
        column_view.add_controller(drop_target);

        // Keyboard shortcuts on the window
        let key_controller = gtk::EventControllerKey::new();
        let sender_key = sender.input_sender().clone();
        key_controller.connect_key_pressed(move |_ctrl, key, _code, modifier| {
            let ctrl = modifier.contains(gdk::ModifierType::CONTROL_MASK);
            match key {
                gdk::Key::Delete => {
                    sender_key.emit(JobQueueMsg::RemoveSelected);
                    glib::Propagation::Stop
                }
                gdk::Key::Up if ctrl => {
                    sender_key.emit(JobQueueMsg::MoveUp);
                    glib::Propagation::Stop
                }
                gdk::Key::Down if ctrl => {
                    sender_key.emit(JobQueueMsg::MoveDown);
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        root.add_controller(key_controller);

        // Context menu (right-click) on column view
        let menu_model = gtk::gio::Menu::new();
        menu_model.append(Some("Configure..."), Some("jq.configure"));
        menu_model.append(Some("Remove from Queue"), Some("jq.remove"));
        let sep = gtk::gio::Menu::new();
        sep.append(Some("Copy Layout"), Some("jq.copy-layout"));
        sep.append(Some("Paste Layout"), Some("jq.paste-layout"));
        menu_model.append_section(None, &sep);

        let popover = gtk::PopoverMenu::from_model(Some(&menu_model));
        popover.set_parent(&column_view);
        popover.set_has_arrow(false);

        // Action group for context menu actions
        let action_group = gtk::gio::SimpleActionGroup::new();

        let s = sender.input_sender().clone();
        let action_configure = gtk::gio::SimpleAction::new("configure", None);
        action_configure.connect_activate(move |_, _| s.emit(JobQueueMsg::ConfigureSelected));
        action_group.add_action(&action_configure);

        let s = sender.input_sender().clone();
        let action_copy = gtk::gio::SimpleAction::new("copy-layout", None);
        action_copy.connect_activate(move |_, _| s.emit(JobQueueMsg::CopyLayout));
        action_group.add_action(&action_copy);

        let s = sender.input_sender().clone();
        let action_paste = gtk::gio::SimpleAction::new("paste-layout", None);
        action_paste.connect_activate(move |_, _| s.emit(JobQueueMsg::PasteLayout));
        action_group.add_action(&action_paste);

        let s = sender.input_sender().clone();
        let action_remove = gtk::gio::SimpleAction::new("remove", None);
        action_remove.connect_activate(move |_, _| s.emit(JobQueueMsg::RemoveSelected));
        action_group.add_action(&action_remove);

        column_view.insert_action_group("jq", Some(&action_group));

        // Right-click gesture
        let gesture = gtk::GestureClick::new();
        gesture.set_button(3);
        let popover_rc = Rc::new(RefCell::new(popover));
        gesture.connect_released(move |_gesture, _n, x, y| {
            let pop = popover_rc.borrow();
            pop.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            pop.popup();
        });
        column_view.add_controller(gesture);

        // Create Add Job sub-dialog
        let add_job_dialog = AddJobDialog::builder()
            .launch(root.clone().upcast::<gtk::Window>())
            .forward(sender.input_sender(), |output| match output {
                AddJobOutput::JobsDiscovered(jobs) => JobQueueMsg::JobsDiscovered(jobs),
                AddJobOutput::Log(msg) => JobQueueMsg::Log(msg),
            });

        // Create Manual Selection sub-dialog
        let manual_selection_dialog = ManualSelectionDialog::builder()
            .launch(root.clone().upcast::<gtk::Window>())
            .forward(sender.input_sender(), |output| match output {
                ManualSelectionOutput::Applied(layout) => JobQueueMsg::LayoutApplied(layout),
                ManualSelectionOutput::Log(msg) => JobQueueMsg::Log(msg),
            });

        // Create model
        let model = JobQueueDialog {
            visible: false,
            queue: JobQueue::in_memory(),
            list_store,
            selection_model,
            sensitivity_dirty: Cell::new(false),
            add_job_dialog,
            manual_selection_dialog,
            configuring_job_id: None,
        };

        let widgets = view_output!();

        // Inject column_view into the scroll window
        widgets.scroll_window.set_child(Some(&column_view));

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            JobQueueMsg::Show => {
                self.visible = true;
                self.refresh_list();
                self.sensitivity_dirty.set(true);
            }
            JobQueueMsg::Close => {
                // Match PySide behavior: closing/cancelling clears all jobs and layouts
                self.queue.clear();
                self.configuring_job_id = None;
                self.refresh_list();
                self.sensitivity_dirty.set(true);
                self.visible = false;
            }
            JobQueueMsg::OpenAddJobDialog(paths) => {
                self.add_job_dialog.emit(AddJobMsg::Show(paths));
            }
            JobQueueMsg::JobsDiscovered(mut jobs) => {
                // Sort by Source 1 filename (natural sort)
                jobs.sort_by(|a, b| {
                    let a_name = a
                        .sources
                        .get("Source 1")
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let b_name = b
                        .sources
                        .get("Source 1")
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    natural_sort_key(&a_name).cmp(&natural_sort_key(&b_name))
                });
                let count = jobs.len();
                self.queue.add_all(jobs);
                self.refresh_list();
                self.sensitivity_dirty.set(true);
                let _ = sender.output(JobQueueOutput::Log(format!(
                    "Added {count} job(s) to queue."
                )));
            }
            JobQueueMsg::MoveUp => {
                let indices = self.selected_indices();
                if !indices.is_empty() {
                    self.queue.move_up(&indices);
                    self.refresh_list();
                    // Reselect at new positions
                    for &idx in &indices {
                        if idx > 0 {
                            self.selection_model.select_item(idx as u32 - 1, false);
                        }
                    }
                    self.sensitivity_dirty.set(true);
                }
            }
            JobQueueMsg::MoveDown => {
                let indices = self.selected_indices();
                if !indices.is_empty() {
                    self.queue.move_down(&indices);
                    self.refresh_list();
                    for &idx in &indices {
                        if idx + 1 < self.queue.len() {
                            self.selection_model.select_item(idx as u32 + 1, false);
                        }
                    }
                    self.sensitivity_dirty.set(true);
                }
            }
            JobQueueMsg::RemoveSelected => {
                let indices = self.selected_indices();
                if !indices.is_empty() {
                    self.queue.remove_indices(indices);
                    self.refresh_list();
                    self.sensitivity_dirty.set(true);
                }
            }
            JobQueueMsg::ConfigureSelected => {
                let indices = self.selected_indices();
                if let Some(&idx) = indices.first() {
                    if let Some(job) = self.queue.get(idx) {
                        self.configuring_job_id = Some(job.id.clone());
                        self.manual_selection_dialog.emit(ManualSelectionMsg::Show {
                            sources: job.sources.clone(),
                            previous_layout: job.layout.clone(),
                            job_name: job.name.clone(),
                        });
                    }
                }
            }
            JobQueueMsg::CopyLayout => {
                let indices = self.selected_indices();
                if let Some(&idx) = indices.first() {
                    if self.queue.copy_layout(idx) {
                        let _ = sender
                            .output(JobQueueOutput::Log("Layout copied to clipboard.".into()));
                    } else {
                        let _ = sender.output(JobQueueOutput::Log(
                            "No layout to copy — job is not configured.".into(),
                        ));
                    }
                    self.sensitivity_dirty.set(true);
                }
            }
            JobQueueMsg::PasteLayout => {
                let indices = self.selected_indices();
                if !indices.is_empty() {
                    let count = self.queue.paste_layout(&indices);
                    if count > 0 {
                        let _ = sender.output(JobQueueOutput::Log(format!(
                            "Layout pasted to {count} job(s)."
                        )));
                        self.refresh_list();
                        self.sensitivity_dirty.set(true);
                    }
                }
            }
            JobQueueMsg::LayoutApplied(layout) => {
                // Use the stored configuring_job_id
                if let Some(job_id) = self.configuring_job_id.take() {
                    if let Some(idx) = self.queue.jobs().iter().position(|j| j.id == job_id) {
                        if let Some(job) = self.queue.get_mut(idx) {
                            job.layout = Some(layout);
                            job.status = JobQueueStatus::Configured;
                            let _ = sender.output(JobQueueOutput::Log(format!(
                                "Job '{}' configured with manual layout.",
                                job.name
                            )));
                        }
                        self.refresh_list();
                        self.sensitivity_dirty.set(true);
                    }
                }
            }
            JobQueueMsg::StartProcessing => {
                let ready: Vec<JobQueueEntry> =
                    self.queue.jobs_ready().into_iter().cloned().collect();
                if ready.is_empty() {
                    let _ = sender.output(JobQueueOutput::Log(
                        "No configured jobs ready for processing.".into(),
                    ));
                } else {
                    let count = ready.len();
                    let _ = sender.output(JobQueueOutput::StartProcessing(ready));
                    let _ = sender.output(JobQueueOutput::Log(format!(
                        "Starting processing of {count} job(s)..."
                    )));
                    // Clear queue after extracting jobs, so reopening shows clean state
                    self.queue.clear();
                    self.configuring_job_id = None;
                    self.refresh_list();
                    self.visible = false;
                }
            }
            JobQueueMsg::SelectionChanged => {
                self.sensitivity_dirty.set(true);
            }
            JobQueueMsg::Log(msg) => {
                // Forward log from child dialogs to parent
                let _ = sender.output(JobQueueOutput::Log(msg));
            }
        }
    }

    /// post_view() runs after #[watch] updates.
    /// Named widgets from the view macro are available as local variables.
    fn post_view() {
        if self.sensitivity_dirty.get() {
            self.sensitivity_dirty.set(false);

            let indices = self.selected_indices();
            let has_selection = !indices.is_empty();
            let single_selection = indices.len() == 1;
            let has_clipboard = self.queue.has_clipboard();
            let has_configured = !self.queue.jobs_ready().is_empty();

            let can_move_up = has_selection && indices[0] > 0;
            let can_move_down =
                has_selection && indices.last().copied().unwrap_or(0) + 1 < self.queue.len();

            move_up_btn.set_sensitive(can_move_up);
            move_down_btn.set_sensitive(can_move_down);
            remove_btn.set_sensitive(has_selection);
            start_btn.set_sensitive(has_configured);

            // Context menu actions are always available (they check selection themselves)
            let _ = has_clipboard;
            let _ = single_selection;
        }
    }
}

impl JobQueueDialog {
    /// Get sorted list of selected row indices.
    fn selected_indices(&self) -> Vec<usize> {
        let bitset = self.selection_model.selection();
        let n = self.list_store.n_items();
        let mut indices = Vec::new();
        for i in 0..n {
            if bitset.contains(i) {
                indices.push(i as usize);
            }
        }
        indices
    }

    /// Refresh the list store from the queue.
    fn refresh_list(&self) {
        self.list_store.remove_all();
        for (i, job) in self.queue.jobs().iter().enumerate() {
            let status_str = match job.status {
                JobQueueStatus::Pending => "Needs Configuration".to_string(),
                JobQueueStatus::Configured => "Configured".to_string(),
                JobQueueStatus::Processing => "Processing...".to_string(),
                JobQueueStatus::Complete => "Complete".to_string(),
                JobQueueStatus::Error => {
                    if let Some(ref msg) = job.error_message {
                        format!("Error: {msg}")
                    } else {
                        "Error".to_string()
                    }
                }
            };
            let row = JobRow {
                index: i as u32,
                status: status_str,
                sources: format_sources_short(&job.sources),
                job_id: job.id.clone(),
            };
            self.list_store.append(&JobRowObject::new(row));
        }
    }
}

/// Natural sort key for filename ordering.
fn natural_sort_key(s: &str) -> Vec<NaturalSortPart> {
    let mut parts = Vec::new();
    let mut current_num = String::new();
    let mut current_text = String::new();

    for ch in s.chars() {
        if ch.is_ascii_digit() {
            if !current_text.is_empty() {
                parts.push(NaturalSortPart::Text(current_text.to_lowercase()));
                current_text.clear();
            }
            current_num.push(ch);
        } else {
            if !current_num.is_empty() {
                parts.push(NaturalSortPart::Number(
                    current_num.parse::<u64>().unwrap_or(0),
                ));
                current_num.clear();
            }
            current_text.push(ch);
        }
    }

    if !current_text.is_empty() {
        parts.push(NaturalSortPart::Text(current_text.to_lowercase()));
    }
    if !current_num.is_empty() {
        parts.push(NaturalSortPart::Number(
            current_num.parse::<u64>().unwrap_or(0),
        ));
    }

    parts
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum NaturalSortPart {
    Text(String),
    Number(u64),
}
