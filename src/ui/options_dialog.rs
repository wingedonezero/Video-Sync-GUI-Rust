// src/ui/options_dialog.rs

use crate::core::config::AppConfig;
use crate::Message as AppMessage;
use iced::widget::{button, checkbox, column, container, pick_list, row, text, text_input};
use iced::{Alignment, Element, Length};

#[derive(Debug, Clone)]
pub struct OptionsDialog {
    // We hold a temporary copy of the config to edit.
    pub pending_config: AppConfig,
    pub active_tab: usize,
}

#[derive(Debug, Clone)]
pub enum DialogMessage {
    // Tabs
    TabSelected(usize),

    // Storage
    OutputFolderChanged(String),
    TempRootChanged(String),
    VideodiffPathChanged(String),

    // Analysis
    AnalysisModeSelected(String),
    AnalysisLangRefChanged(String),
    AnalysisLangSecChanged(String),
    AnalysisLangTerChanged(String),
    ScanChunkCountChanged(String),
    ScanChunkDurationChanged(String),
    MinMatchPctChanged(String),
    VideodiffErrMinChanged(String),
    VideodiffErrMaxChanged(String),

    // Chapters
    RenameChaptersToggled(bool),
    SnapChaptersToggled(bool),
    SnapModeSelected(String),
    SnapThresholdMsChanged(String),
    SnapStartsOnlyToggled(bool),

    // Merge behavior
    ApplyDialogNormGainToggled(bool),
    DisableTrackStatsTagsToggled(bool),
    AutoApplyStrictToggled(bool),

    // Logging
    LogCompactToggled(bool),
    LogAutoscrollToggled(bool),
    LogErrorTailChanged(String),
    LogTailLinesChanged(String),
    LogProgressStepChanged(String),
    LogShowOptionsPrettyToggled(bool),
    LogShowOptionsJsonToggled(bool),
    ArchiveLogsToggled(bool),

    // Lifecycle
    Save,
    Cancel,
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
            // Tabs
            DialogMessage::TabSelected(tab_index) => self.active_tab = tab_index,

            // Storage
            DialogMessage::OutputFolderChanged(val) => self.pending_config.output_folder = val,
            DialogMessage::TempRootChanged(val) => self.pending_config.temp_root = val,
            DialogMessage::VideodiffPathChanged(val) => self.pending_config.videodiff_path = val,

            // Analysis
            DialogMessage::AnalysisModeSelected(val) => self.pending_config.analysis_mode = val,
            DialogMessage::AnalysisLangRefChanged(val) => self.pending_config.analysis_lang_ref = val,
            DialogMessage::AnalysisLangSecChanged(val) => self.pending_config.analysis_lang_sec = val,
            DialogMessage::AnalysisLangTerChanged(val) => self.pending_config.analysis_lang_ter = val,
            DialogMessage::ScanChunkCountChanged(val) => {
                if let Ok(v) = val.parse::<u32>() {
                    self.pending_config.scan_chunk_count = v;
                }
            }
            DialogMessage::ScanChunkDurationChanged(val) => {
                if let Ok(v) = val.parse::<u32>() {
                    self.pending_config.scan_chunk_duration = v;
                }
            }
            DialogMessage::MinMatchPctChanged(val) => {
                if let Ok(v) = val.parse::<f64>() {
                    self.pending_config.min_match_pct = v;
                }
            }
            DialogMessage::VideodiffErrMinChanged(val) => {
                if let Ok(v) = val.parse::<f64>() {
                    self.pending_config.videodiff_error_min = v;
                }
            }
            DialogMessage::VideodiffErrMaxChanged(val) => {
                if let Ok(v) = val.parse::<f64>() {
                    self.pending_config.videodiff_error_max = v;
                }
            }

            // Chapters
            DialogMessage::RenameChaptersToggled(v) => self.pending_config.rename_chapters = v,
            DialogMessage::SnapChaptersToggled(v) => self.pending_config.snap_chapters = v,
            DialogMessage::SnapModeSelected(val) => self.pending_config.snap_mode = val,
            DialogMessage::SnapThresholdMsChanged(val) => {
                if let Ok(v) = val.parse::<u32>() {
                    self.pending_config.snap_threshold_ms = v;
                }
            }
            DialogMessage::SnapStartsOnlyToggled(v) => self.pending_config.snap_starts_only = v,

            // Merge behavior
            DialogMessage::ApplyDialogNormGainToggled(v) => {
                self.pending_config.apply_dialog_norm_gain = v
            }
            DialogMessage::DisableTrackStatsTagsToggled(v) => {
                self.pending_config.disable_track_statistics_tags = v
            }
            DialogMessage::AutoApplyStrictToggled(v) => self.pending_config.auto_apply_strict = v,

            // Logging
            DialogMessage::LogCompactToggled(v) => self.pending_config.log_compact = v,
            DialogMessage::LogAutoscrollToggled(v) => self.pending_config.log_autoscroll = v,
            DialogMessage::LogErrorTailChanged(val) => {
                if let Ok(v) = val.parse::<u32>() {
                    self.pending_config.log_error_tail = v;
                }
            }
            DialogMessage::LogTailLinesChanged(val) => {
                if let Ok(v) = val.parse::<u32>() {
                    self.pending_config.log_tail_lines = v;
                }
            }
            DialogMessage::LogProgressStepChanged(val) => {
                if let Ok(v) = val.parse::<u32>() {
                    self.pending_config.log_progress_step = v;
                }
            }
            DialogMessage::LogShowOptionsPrettyToggled(v) => {
                self.pending_config.log_show_options_pretty = v
            }
            DialogMessage::LogShowOptionsJsonToggled(v) => {
                self.pending_config.log_show_options_json = v
            }
            DialogMessage::ArchiveLogsToggled(v) => self.pending_config.archive_logs = v,

            // Save / Cancel handled in lib.rs
            DialogMessage::Save | DialogMessage::Cancel => {}
        }
    }
}

pub fn view(state: &OptionsDialog) -> Element<AppMessage> {
    // Simple tabs row (buttons) to avoid iced_aw API mismatches, same UX
    let tabs_row = row![
        button(text("Storage")).on_press(AppMessage::OptionsMessage(DialogMessage::TabSelected(0))),
        button(text("Analysis")).on_press(AppMessage::OptionsMessage(DialogMessage::TabSelected(1))),
        button(text("Chapters")).on_press(AppMessage::OptionsMessage(DialogMessage::TabSelected(2))),
        button(text("Merge")).on_press(AppMessage::OptionsMessage(DialogMessage::TabSelected(3))),
        button(text("Logging")).on_press(AppMessage::OptionsMessage(DialogMessage::TabSelected(4))),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let tab_content = match state.active_tab {
        0 => view_storage_tab(&state.pending_config),
        1 => view_analysis_tab(&state.pending_config),
        2 => view_chapters_tab(&state.pending_config),
        3 => view_merge_tab(&state.pending_config),
        _ => view_logging_tab(&state.pending_config),
    }
    .map(AppMessage::OptionsMessage);

    let controls = row![
        button(text("Save")).on_press(AppMessage::OptionsMessage(DialogMessage::Save)),
        button(text("Cancel")).on_press(AppMessage::OptionsMessage(DialogMessage::Cancel)),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let content_card = container(
        column![tabs_row, tab_content, controls]
        .spacing(15)
        .padding(10u16)
        .align_x(Alignment::Center),
    )
    .max_width(640);

    // Modal overlay feel
    container(content_card)
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Shrink)
    .into()
}

fn view_storage_tab(config: &AppConfig) -> Element<DialogMessage> {
    let content = column![
        row![
            text("Output Directory:").width(150),
            text_input("path...", &config.output_folder).on_input(DialogMessage::OutputFolderChanged),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("Temporary Directory:").width(150),
            text_input("path...", &config.temp_root).on_input(DialogMessage::TempRootChanged),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("VideoDiff Path:").width(150),
            text_input("optional...", &config.videodiff_path).on_input(DialogMessage::VideodiffPathChanged),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    ]
    .spacing(10);
    container(content).padding(20u16).into()
}

fn view_analysis_tab(config: &AppConfig) -> Element<DialogMessage> {
    let modes = vec!["Audio Correlation".to_string(), "VideoDiff".to_string()];
    let content = column![
        row![
            text("Analysis Mode:").width(150),
            pick_list(modes, Some(config.analysis_mode.clone()), DialogMessage::AnalysisModeSelected)
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("Ref Lang (e.g. eng):").width(150),
            text_input("optional", &config.analysis_lang_ref).on_input(DialogMessage::AnalysisLangRefChanged),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("Sec Lang:").width(150),
            text_input("optional", &config.analysis_lang_sec).on_input(DialogMessage::AnalysisLangSecChanged),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("Ter Lang:").width(150),
            text_input("optional", &config.analysis_lang_ter).on_input(DialogMessage::AnalysisLangTerChanged),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("Audio: Scan Chunks:").width(150),
            text_input("count", &config.scan_chunk_count.to_string())
            .on_input(DialogMessage::ScanChunkCountChanged),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("Audio: Chunk Duration (s):").width(150),
            text_input("seconds", &config.scan_chunk_duration.to_string())
            .on_input(DialogMessage::ScanChunkDurationChanged),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("Min Match %:").width(150),
            text_input("e.g. 5.0", &format!("{}", config.min_match_pct))
            .on_input(DialogMessage::MinMatchPctChanged),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("VideoDiff Err Min:").width(150),
            text_input("e.g. 0.0", &format!("{}", config.videodiff_error_min))
            .on_input(DialogMessage::VideodiffErrMinChanged),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("VideoDiff Err Max:").width(150),
            text_input("e.g. 100.0", &format!("{}", config.videodiff_error_max))
            .on_input(DialogMessage::VideodiffErrMaxChanged),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    ]
    .spacing(10);
    container(content).padding(20u16).into()
}

fn view_chapters_tab(config: &AppConfig) -> Element<DialogMessage> {
    let snap_modes = vec!["previous".to_string(), "nearest".to_string(), "next".to_string()];
    let content = column![
        checkbox("Rename chapters to 'Chapter 01…'", config.rename_chapters)
        .on_toggle(DialogMessage::RenameChaptersToggled),
        checkbox("Snap chapter times to keyframes", config.snap_chapters)
        .on_toggle(DialogMessage::SnapChaptersToggled),
        row![
            text("Snap mode:").width(150),
            pick_list(snap_modes, Some(config.snap_mode.clone()), DialogMessage::SnapModeSelected)
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("Snap threshold (ms):").width(150),
            text_input("e.g. 250", &config.snap_threshold_ms.to_string())
            .on_input(DialogMessage::SnapThresholdMsChanged),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        checkbox("Snap starts only", config.snap_starts_only)
        .on_toggle(DialogMessage::SnapStartsOnlyToggled),
    ]
    .spacing(10);
    container(content).padding(20u16).into()
}

fn view_merge_tab(config: &AppConfig) -> Element<DialogMessage> {
    let content = column![
        checkbox(
            "Remove Dolby dialog normalization gain (AC3/E-AC3)",
                 config.apply_dialog_norm_gain
        )
        .on_toggle(DialogMessage::ApplyDialogNormGainToggled),
        checkbox("Disable track statistics tags", config.disable_track_statistics_tags)
        .on_toggle(DialogMessage::DisableTrackStatsTagsToggled),
        checkbox(
            "Auto-apply previous layout strictly (type+lang+codec)",
                 config.auto_apply_strict
        )
        .on_toggle(DialogMessage::AutoApplyStrictToggled),
    ]
    .spacing(10);
    container(content).padding(20u16).into()
}

fn view_logging_tab(config: &AppConfig) -> Element<DialogMessage> {
    let content = column![
        checkbox("Compact logging (progress coalescing)", config.log_compact)
        .on_toggle(DialogMessage::LogCompactToggled),
        checkbox("Auto-scroll log view", config.log_autoscroll)
        .on_toggle(DialogMessage::LogAutoscrollToggled),
        row![
            text("Error tail lines:").width(150),
            text_input("e.g. 20", &config.log_error_tail.to_string())
            .on_input(DialogMessage::LogErrorTailChanged),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("Tail lines on success:").width(150),
            text_input("e.g. 0", &config.log_tail_lines.to_string())
            .on_input(DialogMessage::LogTailLinesChanged),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("Progress step (%):").width(150),
            text_input("e.g. 20", &config.log_progress_step.to_string())
            .on_input(DialogMessage::LogProgressStepChanged),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        checkbox("Log mkvmerge options (pretty)", config.log_show_options_pretty)
        .on_toggle(DialogMessage::LogShowOptionsPrettyToggled),
        checkbox("Log mkvmerge options (JSON)", config.log_show_options_json)
        .on_toggle(DialogMessage::LogShowOptionsJsonToggled),
        checkbox("Archive logs on batch completion", config.archive_logs)
        .on_toggle(DialogMessage::ArchiveLogsToggled),
    ]
    .spacing(10);
    container(content).padding(20u16).into()
}
