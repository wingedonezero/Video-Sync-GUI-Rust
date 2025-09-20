use iced::widget::{button, column, container, row, text, text_input, checkbox, Space};
use iced::{Element, Length};
use iced_aw::widget::tabs;
use iced_aw::widget::tab_bar::TabLabel;

pub struct SettingsDialog {
    active_tab: TabId,
    // Storage settings
    output_folder: String,
    temp_root: String,
    // Analysis settings
    min_match_pct: f32,
    scan_chunk_count: i32,
    // Chapter settings
    rename_chapters: bool,
    snap_chapters: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TabId {
    Storage,
    Analysis,
    Chapters,
    MergeBehavior,
    Logging,
}

#[derive(Debug, Clone)]
pub enum Message {
    TabSelected(TabId),
    OutputFolderChanged(String),
    TempRootChanged(String),
    MinMatchChanged(f32),
    ChunkCountChanged(i32),
    RenameChaptersToggled(bool),
    SnapChaptersToggled(bool),
    Save,
    Cancel,
}

impl SettingsDialog {
    pub fn new(config: &crate::config::AppConfig) -> Self {
        Self {
            active_tab: TabId::Storage,
            output_folder: config.output_folder.clone(),
            temp_root: config.temp_root.clone(),
            min_match_pct: config.min_match_pct,
            scan_chunk_count: config.scan_chunk_count,
            rename_chapters: config.rename_chapters,
            snap_chapters: config.snap_chapters,
        }
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::TabSelected(tab) => {
                self.active_tab = tab;
            }
            Message::OutputFolderChanged(value) => {
                self.output_folder = value;
            }
            Message::TempRootChanged(value) => {
                self.temp_root = value;
            }
            Message::MinMatchChanged(value) => {
                self.min_match_pct = value;
            }
            Message::ChunkCountChanged(value) => {
                self.scan_chunk_count = value;
            }
            Message::RenameChaptersToggled(value) => {
                self.rename_chapters = value;
            }
            Message::SnapChaptersToggled(value) => {
                self.snap_chapters = value;
            }
            Message::Save => {
                // TODO: Save config and close
            }
            Message::Cancel => {
                // TODO: Close without saving
            }
        }
    }

    pub fn view(&self) -> Element<Message> {
        let tabs = tabs::Tabs::new(Message::TabSelected)
        .push(TabId::Storage, TabLabel::Text("Storage".to_string()), self.storage_tab())
        .push(TabId::Analysis, TabLabel::Text("Analysis".to_string()), self.analysis_tab())
        .push(TabId::Chapters, TabLabel::Text("Chapters".to_string()), self.chapters_tab())
        .push(TabId::MergeBehavior, TabLabel::Text("Merge Behavior".to_string()), self.merge_tab())
        .push(TabId::Logging, TabLabel::Text("Logging".to_string()), self.logging_tab())
        .set_active_tab(&self.active_tab);

        let dialog_buttons = row![
            Space::with_width(Length::Fill),
            button("Save").on_press(Message::Save),
            button("Cancel").on_press(Message::Cancel),
        ]
        .spacing(10)
        .padding(10);

        column![
            container(tabs).height(Length::FillPortion(5)),
            dialog_buttons,
        ]
        .into()
    }

    fn storage_tab(&self) -> Element<Message> {
        column![
            row![
                text("Output Directory:").width(Length::FillPortion(2)),
                text_input("", &self.output_folder)
                .on_input(Message::OutputFolderChanged)
                .width(Length::FillPortion(4)),
                button("Browse…").width(Length::FillPortion(1)),
            ]
            .spacing(10),

            row![
                text("Temporary Directory:").width(Length::FillPortion(2)),
                text_input("", &self.temp_root)
                .on_input(Message::TempRootChanged)
                .width(Length::FillPortion(4)),
                button("Browse…").width(Length::FillPortion(1)),
            ]
            .spacing(10),
        ]
        .spacing(10)
        .padding(20)
        .into()
    }

    fn analysis_tab(&self) -> Element<Message> {
        column![
            text("Analysis settings will go here"),
        ]
        .padding(20)
        .into()
    }

    fn chapters_tab(&self) -> Element<Message> {
        column![
            checkbox("Rename chapters to 'Chapter NN'", self.rename_chapters)
            .on_toggle(Message::RenameChaptersToggled),
            checkbox("Snap chapter timestamps to nearest keyframe", self.snap_chapters)
            .on_toggle(Message::SnapChaptersToggled),
        ]
        .spacing(10)
        .padding(20)
        .into()
    }

    fn merge_tab(&self) -> Element<Message> {
        column![
            text("Merge behavior settings will go here"),
        ]
        .padding(20)
        .into()
    }

    fn logging_tab(&self) -> Element<Message> {
        column![
            text("Logging settings will go here"),
        ]
        .padding(20)
        .into()
    }
}
