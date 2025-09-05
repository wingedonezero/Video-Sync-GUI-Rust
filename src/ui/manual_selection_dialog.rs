// src/ui/manual_selection_dialog.rs

use iced::widget::{
    button, checkbox, column, container, row, scrollable, text, text_input, Space,
};
use iced::{Alignment, Element, Length, Task};

use crate::core::mkv_utils::{self, Track};
use crate::core::pipeline::{Job, TrackSelection};
use crate::core::process::CommandRunner;

/// State for the Manual Selection window
#[derive(Debug, Clone)]
pub struct ManualSelection {
    job: Job,
    // available tracks
    available_ref_tracks: Vec<Track>,
    available_sec_tracks: Vec<Track>,
    available_ter_tracks: Vec<Track>,

    // final selections (ordered)
    pub selected_tracks: Vec<TrackSelection>,

    // lightweight per-item editing state (size multiplier as a string for input)
    size_inputs: Vec<String>,

    status: String,
    loaded: bool,
}

#[derive(Debug, Clone)]
pub enum DialogMessage {
    // async loading
    TracksInfoReady(Result<(Vec<Track>, Vec<Track>, Vec<Track>), String>),

    // adding/removing/reordering
    AddTrack(&'static str, Track),     // source tag "REF"/"SEC"/"TER"
    RemoveTrack(usize),
    MoveTrack(usize, i8),              // index, -1 up / +1 down

    // per-item toggles
    ToggleDefault(usize),
    ToggleForced(usize),
    ToggleApplyTrackName(usize),
    ToggleConvertToAss(usize),
    ToggleRescale(usize),

    // size multiplier editing
    SizeChanged(usize, String),
    SizeInc(usize),
    SizeDec(usize),

    // actions
    OkClicked,
    CancelClicked,
}

pub enum DialogResult {
    Ok(Vec<TrackSelection>),
    Cancel,
}

impl ManualSelection {
    pub fn new(job: Job) -> Self {
        Self {
            job,
            available_ref_tracks: vec![],
            available_sec_tracks: vec![],
            available_ter_tracks: vec![],
            selected_tracks: vec![],
            size_inputs: vec![],
            status: "Loading track info...".to_string(),
            loaded: false,
        }
    }

    /// Kick off async load of track info (iced 0.13 uses Task)
    pub fn on_load(&self) -> Task<crate::Message> {
        Task::perform(
            load_track_info(self.job.clone()),
                      |result| crate::Message::ManualSelectionMessage(DialogMessage::TracksInfoReady(result)),
        )
    }

    pub fn update(&mut self, message: DialogMessage) -> Option<DialogResult> {
        match message {
            DialogMessage::TracksInfoReady(Ok((ref_t, sec_t, ter_t))) => {
                self.available_ref_tracks = ref_t;
                self.available_sec_tracks = sec_t;
                self.available_ter_tracks = ter_t;
                self.status = "Ready".to_string();
                self.loaded = true;
            }
            DialogMessage::TracksInfoReady(Err(e)) => {
                self.status = format!("Error loading tracks: {}", e);
                self.loaded = true;
            }

            DialogMessage::AddTrack(source, track) => {
                // Guardrail: SEC/TER video cannot be added
                if (source == "SEC" || source == "TER") && track.r#type == "video" {
                    return None;
                }
                let selection = TrackSelection {
                    source: source.to_string(),
                    original_track: track,
                    extracted_path: None,
                    is_default: false,
                    is_forced: false,
                    apply_track_name: true,
                    convert_to_ass: false,
                    rescale: false,
                    size_multiplier: 1.0,
                };
                self.selected_tracks.push(selection);
                self.size_inputs.push("1".to_string());
                // keep single default per type if user toggles later
            }

            DialogMessage::RemoveTrack(index) => {
                if index < self.selected_tracks.len() {
                    self.selected_tracks.remove(index);
                    self.size_inputs.remove(index);
                }
            }

            DialogMessage::MoveTrack(index, delta) => {
                if index < self.selected_tracks.len() {
                    let new_index = if delta < 0 {
                        index.saturating_sub(1)
                    } else {
                        (index + 1).min(self.selected_tracks.len() - 1)
                    };
                    if new_index != index {
                        self.selected_tracks.swap(index, new_index);
                        self.size_inputs.swap(index, new_index);
                    }
                }
            }

            DialogMessage::ToggleDefault(index) => {
                if let Some(sel) = self.selected_tracks.get(index).cloned() {
                    let ttype = sel.original_track.r#type.clone();
                    // unset all defaults of same type
                    for s in self.selected_tracks.iter_mut().filter(|s| s.original_track.r#type == ttype) {
                        s.is_default = false;
                    }
                    // set toggled item
                    if let Some(s) = self.selected_tracks.get_mut(index) {
                        s.is_default = true;
                    }
                }
            }

            DialogMessage::ToggleForced(index) => {
                if let Some(s) = self.selected_tracks.get_mut(index) {
                    // Forced only applies to subtitles
                    if s.original_track.r#type == "subtitles" {
                        s.is_forced = !s.is_forced;
                    }
                }
            }

            DialogMessage::ToggleApplyTrackName(index) => {
                if let Some(s) = self.selected_tracks.get_mut(index) {
                    s.apply_track_name = !s.apply_track_name;
                }
            }

            DialogMessage::ToggleConvertToAss(index) => {
                if let Some(s) = self.selected_tracks.get_mut(index) {
                    s.convert_to_ass = !s.convert_to_ass;
                }
            }

            DialogMessage::ToggleRescale(index) => {
                if let Some(s) = self.selected_tracks.get_mut(index) {
                    s.rescale = !s.rescale;
                }
            }

            DialogMessage::SizeChanged(index, val) => {
                if index < self.size_inputs.len() {
                    self.size_inputs[index] = val.clone();
                    // live-parse to valid number if possible (>0)
                    if let Ok(n) = val.trim().parse::<f64>() {
                        if n > 0.0 {
                            if let Some(s) = self.selected_tracks.get_mut(index) {
                                s.size_multiplier = n;
                            }
                        }
                    }
                }
            }

            DialogMessage::SizeInc(index) => {
                if let Some(s) = self.selected_tracks.get_mut(index) {
                    s.size_multiplier = (s.size_multiplier + 0.1).max(0.1);
                    if index < self.size_inputs.len() {
                        self.size_inputs[index] = format!("{:.2}", s.size_multiplier);
                    }
                }
            }

            DialogMessage::SizeDec(index) => {
                if let Some(s) = self.selected_tracks.get_mut(index) {
                    s.size_multiplier = (s.size_multiplier - 0.1).max(0.1);
                    if index < self.size_inputs.len() {
                        self.size_inputs[index] = format!("{:.2}", s.size_multiplier);
                    }
                }
            }

            DialogMessage::OkClicked => {
                // Normalize: ensure at most one default per type.
                enforce_single_default_per_type(&mut self.selected_tracks);
                return Some(DialogResult::Ok(self.selected_tracks.clone()));
            }
            DialogMessage::CancelClicked => return Some(DialogResult::Cancel),
        }
        None
    }
}

fn enforce_single_default_per_type(list: &mut [TrackSelection]) {
    // keep the first default encountered for a type; clear others
    use std::collections::HashSet;
    let mut seen: HashSet<&str> = HashSet::new();
    for i in 0..list.len() {
        if list[i].is_default {
            let key: &str = &list[i].original_track.r#type;
            if seen.contains(key) {
                list[i].is_default = false;
            } else {
                seen.insert(key);
            }
        }
    }
}

pub fn view(state: &ManualSelection) -> Element<DialogMessage> {
    // LEFT: available tracks (REF/SEC/TER)
    let available_tracks_pane = scrollable(
        column![
            track_list_group("Reference Tracks", "REF", &state.available_ref_tracks),
                                           track_list_group("Secondary Tracks", "SEC", &state.available_sec_tracks),
                                           track_list_group("Tertiary Tracks", "TER", &state.available_ter_tracks),
        ]
        .spacing(20),
    );

    // RIGHT: selected tracks (final output)
    let mut final_col = column!().spacing(8);
    for (i, selection) in state.selected_tracks.iter().enumerate() {
        final_col = final_col.push(selected_track_row(i, selection, state.size_inputs.get(i)));
    }
    let final_output_pane = scrollable(final_col);

    let header = text("Manual Track Selection").size(22);

    let main_row = row![
        container(available_tracks_pane).width(Length::FillPortion(2)),
        container(final_output_pane).width(Length::FillPortion(3)),
    ]
    .spacing(16);

    let controls = row![
        text(if state.loaded { &state.status } else { "Loading…" }).width(Length::Fill),
        button("OK").on_press(DialogMessage::OkClicked),
        button("Cancel").on_press(DialogMessage::CancelClicked),
    ]
    .spacing(10)
    .align_items(Alignment::Center);

    container(
        column![
            header,
            main_row,
            controls,
        ]
        .spacing(12)
        .padding(10),
    )
    .width(Length::Fixed(1100.0))
    .height(Length::Fixed(650.0))
    .into()
}

fn track_list_group<'a>(
    title: &'a str,
    source: &'static str,
    tracks: &'a [Track],
) -> Element<'a, DialogMessage> {
    let mut items = column!().spacing(6);

    if tracks.is_empty() {
        items = items.push(text("No tracks found.").size(14));
    } else {
        for tr in tracks {
            let is_disabled_video = tr.r#type == "video" && (source == "SEC" || source == "TER");
            let label = format!(
                "[{}-{:}] ({}) {}{}",
                                tr.r#type.chars().next().unwrap_or('U').to_ascii_uppercase(),
                                tr.id,
                                tr.properties.language.as_deref().unwrap_or("und"),
                                tr.properties
                                .codec_id
                                .as_deref()
                                .unwrap_or("N/A"),
                                if tr
                                    .properties
                                    .track_name
                                    .as_deref()
                                    .map(|s| !s.is_empty())
                                    .unwrap_or(false)
                                    {
                                        format!(
                                            "  '{}'",
                                            tr.properties.track_name.as_deref().unwrap_or("")
                                        )
                                    } else {
                                        "".to_string()
                                    }
            );

            let mut btn = button(label).width(Length::Fill);
            if !is_disabled_video {
                btn = btn.on_press(DialogMessage::AddTrack(source, tr.clone()));
            }
            items = items.push(btn);
        }
    }

    column![text(title).size(16), container(items).padding(6)]
    .spacing(6)
    .into()
}

fn selected_track_row(
    index: usize,
    sel: &TrackSelection,
    size_input_opt: Option<&String>,
) -> Element<DialogMessage> {
    let badges = format!(
        "{}{}",
        if sel.is_default { "⭐ " } else { "" },
            if sel.is_forced && sel.original_track.r#type == "subtitles" { "📌 " } else { "" }
    );

    let info = text(format!(
        "[{}] ID {} ({}) {}  {}",
                            sel.source,
                            sel.original_track.id,
                            sel.original_track
                            .properties
                            .language
                            .as_deref()
                            .unwrap_or("und"),
                            sel.original_track
                            .properties
                            .codec_id
                            .as_deref()
                            .unwrap_or("N/A"),
                            badges
    ));

    // Per-item controls (compact, inline)
    let is_subs = sel.original_track.r#type == "subtitles";

    // size multiplier input (string kept in state)
    let size_str = size_input_opt.cloned().unwrap_or_else(|| "1".to_string());
    let size_row = row![
        text("Size:"),
        button("-").on_press(DialogMessage::SizeDec(index)),
        text_input("x", &size_str).on_input(move |s| DialogMessage::SizeChanged(index, s)).width(Length::Fixed(60.0)),
        button("+").on_press(DialogMessage::SizeInc(index)),
    ]
    .spacing(6)
    .align_items(Alignment::Center);

    let toggles_row = row![
        button(if sel.is_default { "Default ✓" } else { "Default" })
        .on_press(DialogMessage::ToggleDefault(index)),
        button(if sel.apply_track_name { "Name ✓" } else { "Name" })
        .on_press(DialogMessage::ToggleApplyTrackName(index)),
        if is_subs {
            button(if sel.is_forced { "Forced ✓" } else { "Forced" })
            .on_press(DialogMessage::ToggleForced(index))
        } else {
            // placeholder to keep spacing
            button("Forced").on_press(DialogMessage::ToggleForced(index)).style(iced::theme::Button::Secondary)
        },
        if is_subs && extension_is_srt(&sel.original_track) {
            button(if sel.convert_to_ass { "SRT→ASS ✓" } else { "SRT→ASS" })
            .on_press(DialogMessage::ToggleConvertToAss(index))
        } else {
            button("SRT→ASS").on_press(DialogMessage::ToggleConvertToAss(index)).style(iced::theme::Button::Secondary)
        },
        if is_subs {
            button(if sel.rescale { "Rescale ✓" } else { "Rescale" })
            .on_press(DialogMessage::ToggleRescale(index))
        } else {
            button("Rescale").on_press(DialogMessage::ToggleRescale(index)).style(iced::theme::Button::Secondary)
        },
        Space::with_width(Length::Fill),
        button("↑").on_press(DialogMessage::MoveTrack(index, -1)),
        button("↓").on_press(DialogMessage::MoveTrack(index, 1)),
        button("Remove").on_press(DialogMessage::RemoveTrack(index)),
    ]
    .spacing(6)
    .align_items(Alignment::Center);

    container(
        column![
            row![info].align_items(Alignment::Center),
              size_row,
              toggles_row,
        ]
        .spacing(6)
        .padding(6),
    )
    .into()
}

fn extension_is_srt(track: &Track) -> bool {
    // mkvmerge codec id for UTF8 subs is "S_TEXT/UTF8" (SRT)
    track
    .properties
    .codec_id
    .as_deref()
    .map(|c| c.eq_ignore_ascii_case("S_TEXT/UTF8"))
    .unwrap_or(false)
}

async fn load_track_info(job: Job) -> Result<(Vec<Track>, Vec<Track>, Vec<Track>), String> {
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    tokio::spawn(async move {
        // drain logs if any
        while rx.recv().await.is_some() {}
    });
    let runner = CommandRunner::new(Default::default(), tx);

    let ref_tracks = mkv_utils::get_stream_info(&runner, &job.ref_file)
    .await?
    .tracks;

    let sec_tracks = if let Some(sec_file) = &job.sec_file {
        mkv_utils::get_stream_info(&runner, sec_file).await?.tracks
    } else {
        vec![]
    };

    let ter_tracks = if let Some(ter_file) = &job.ter_file {
        mkv_utils::get_stream_info(&runner, ter_file).await?.tracks
    } else {
        vec![]
    };

    Ok((ref_tracks, sec_tracks, ter_tracks))
}
