// src/ui/options_dialog.rs

use crate::core::config::AppConfig;
use crate::Message as AppMessage;
use iced::widget::{button, checkbox, column, container, pick_list, row, text, text_input, Column};
use iced::{Alignment, Element, Length, Theme};
use iced_aw::{TabLabel, Tabs};

#[derive(Debug, Clone)]
pub struct OptionsDialog {
    // We hold a temporary copy of the config to edit.
    pub pending_config: AppConfig,
    active_tab: usize,
}

#[derive(Debug, Clone)]
pub enum DialogMessage {
    TabSelected(usize),
    // Storage
    OutputFolderChanged(String),
    TempRootChanged(String),
    VideodiffPathChanged(String),
    // Analysis
    AnalysisModeSelected(String),
    ScanChunkCountChanged(String),
    ScanChunkDurationChanged(String),
    // ... other fields would have messages here
    Save,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TabId {
    Storage,
    Analysis,
    Chapters,
    Merge,
    Logging,
}

impl OptionsDialog {
    pub fn new(config: AppConfig) -> Self {
        Self {
            pending_config: config,
            active_tab: 0,
        }
    }

    pub fn update(&mut self, message: DialogMessage) {
        match message {
            DialogMessage::TabSelected(tab_index) => self.active_tab = tab_index,
            DialogMessage::OutputFolderChanged(val) => self.pending_config.output_folder = val,
            DialogMessage::TempRootChanged(val) => self.pending_config.temp_root = val,
            DialogMessage::VideodiffPathChanged(val) => self.pending_config.videodiff_path = val,
            DialogMessage::AnalysisModeSelected(val) => self.pending_config.analysis_mode = val,
            DialogMessage::ScanChunkCountChanged(val) => {
                if let Ok(num) = val.parse::<u32>() {
                    self.pending_config.scan_chunk_count = num;
                }
            }
            DialogMessage::ScanChunkDurationChanged(val) => {
                if let Ok(num) = val.parse::<u32>() {
                    self.pending_config.scan_chunk_duration = num;
                }
            }
            _ => {} // Save and Cancel are handled in lib.rs
        }
    }
}

pub fn view(state: &OptionsDialog) -> Element<AppMessage> {
    let tabs = Tabs::new(state.active_tab, |i| {
        let (tab_label, tab_content) = match i {
            0 => (
                TabId::Storage,
                view_storage_tab(&state.pending_config),
            ),
            1 => (
                TabId::Analysis,
                view_analysis_tab(&state.pending_config),
            ),
            _ => (
                TabId::Logging,
                column![text("Other settings will go here.")].into()
            ),
        };
        (tab_label.into(), tab_content.map(AppMessage::OptionsMessage))
    })
    .on_select(|i| AppMessage::OptionsMessage(DialogMessage::TabSelected(i)));

    let controls = row![
        button("Save").on_press(AppMessage::OptionsMessage(DialogMessage::Save)),
        button("Cancel").on_press(AppMessage::OptionsMessage(DialogMessage::Cancel)),
    ]
    .spacing(10);

    let content_card = container(
        column![tabs, controls]
        .spacing(15)
        .padding(10)
        .align_items(Alignment::Center)
    )
    .style(iced::theme::Container::Box) // Use a standard theme style
    .max_width(600);

    // Modal background overlay
    container(content_card)
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x()
    .center_y()
    .into()
}

fn view_storage_tab(config: &AppConfig) -> Element<DialogMessage> {
    let content = column![
        row![
            text("Output Directory:").width(150),
            text_input("path...", &config.output_folder)
            .on_input(DialogMessage::OutputFolderChanged),
        ].spacing(10).align_items(Alignment::Center),
        row![
            text("Temporary Directory:").width(150),
            text_input("path...", &config.temp_root)
            .on_input(DialogMessage::TempRootChanged),
        ].spacing(10).align_items(Alignment::Center),
        row![
            text("VideoDiff Path:").width(150),
            text_input("optional...", &config.videodiff_path)
            .on_input(DialogMessage::VideodiffPathChanged),
        ].spacing(10).align_items(Alignment::Center),
    ]
    .spacing(10);
    container(content).padding(20).into()
}

fn view_analysis_tab(config: &AppConfig) -> Element<DialogMessage> {
    let modes = vec!["Audio Correlation".to_string(), "VideoDiff".to_string()];
    let content = column![
        row![
            text("Analysis Mode:").width(150),
            pick_list(modes, Some(config.analysis_mode.clone()), DialogMessage::AnalysisModeSelected)
        ].spacing(10).align_items(Alignment::Center),
        row![
            text("Audio: Scan Chunks:").width(150),
            text_input("count", &config.scan_chunk_count.to_string())
            .on_input(DialogMessage::ScanChunkCountChanged),
        ].spacing(10).align_items(Alignment::Center),
        row![
            text("Audio: Chunk Duration (s):").width(150),
            text_input("seconds", &config.scan_chunk_duration.to_string())
            .on_input(DialogMessage::ScanChunkDurationChanged),
        ].spacing(10).align_items(Alignment::Center),
    ]
    .spacing(10);
    container(content).padding(20).into()
}

impl From<TabId> for TabLabel {
    fn from(id: TabId) -> Self {
        TabLabel::Text(format!("{:?}", id))
    }
}
