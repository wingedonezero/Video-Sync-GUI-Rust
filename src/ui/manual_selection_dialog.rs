// src/ui/manual_selection_dialog.rs

use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Command, Element, Length, Theme};
use crate::core::mkv_utils::{self, Track};
use crate::core::pipeline::{Job, TrackSelection};
use crate::core::process::CommandRunner;

// State for the Manual Selection window
#[derive(Debug, Clone)]
pub struct ManualSelection {
    job: Job,
    available_ref_tracks: Vec<Track>,
    available_sec_tracks: Vec<Track>,
    available_ter_tracks: Vec<Track>,
    pub selected_tracks: Vec<TrackSelection>,
    status: String,
}

#[derive(Debug, Clone)]
pub enum DialogMessage {
    TracksInfoReady(Result<(Vec<Track>, Vec<Track>, Vec<Track>), String>),
    AddTrack(String, Track),
    RemoveTrack(usize),
    MoveTrack(usize, i8), // index, direction (-1 up, 1 down)
    ToggleDefault(usize),
    // ... other settings messages
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
            status: "Loading track info...".to_string(),
        }
    }

    pub fn on_load(&self) -> Command<crate::Message> {
        Command::perform(
            load_track_info(self.job.clone()),
                         |result| crate::Message::ManualSelectionMessage(DialogMessage::TracksInfoReady(result))
        )
    }

    pub fn update(&mut self, message: DialogMessage) -> Option<DialogResult> {
        match message {
            DialogMessage::TracksInfoReady(Ok((ref_t, sec_t, ter_t))) => {
                self.available_ref_tracks = ref_t;
                self.available_sec_tracks = sec_t;
                self.available_ter_tracks = ter_t;
                self.status = "Ready".to_string();
            }
            DialogMessage::TracksInfoReady(Err(e)) => {
                self.status = format!("Error loading tracks: {}", e);
            }
            DialogMessage::AddTrack(source, track) => {
                let selection = TrackSelection {
                    source,
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
            }
            DialogMessage::RemoveTrack(index) => {
                if index < self.selected_tracks.len() {
                    self.selected_tracks.remove(index);
                }
            }
            DialogMessage::ToggleDefault(index) => {
                if let Some(track_selection) = self.selected_tracks.get_mut(index) {
                    let new_state = !track_selection.is_default;
                    let track_type = track_selection.original_track.r#type.clone();

                    // Unset all others of the same type
                    for other_track in self.selected_tracks.iter_mut().filter(|t| t.original_track.r#type == track_type) {
                        other_track.is_default = false;
                    }

                    // Set the toggled one
                    track_selection.is_default = new_state;
                }
            }
            DialogMessage::OkClicked => {
                // TODO: Add normalization logic here
                return Some(DialogResult::Ok(self.selected_tracks.clone()));
            },
            DialogMessage::CancelClicked => return Some(DialogResult::Cancel),
            _ => {}
        }
        None
    }
}

pub fn view(state: &ManualSelection) -> Element<DialogMessage> {
    let available_tracks_pane = scrollable(column![
        track_list_group("Reference Tracks", "REF", &state.available_ref_tracks),
                                           track_list_group("Secondary Tracks", "SEC", &state.available_sec_tracks),
                                           track_list_group("Tertiary Tracks", "TER", &state.available_ter_tracks),
    ].spacing(20));

    let mut final_tracks_col = column!().spacing(5);
    for (i, selection) in state.selected_tracks.iter().enumerate() {
        final_tracks_col = final_tracks_col.push(selected_track_view(i, selection));
    }
    let final_output_pane = scrollable(final_tracks_col);

    let main_content = row![
        container(available_tracks_pane).width(Length::FillPortion(2)),
        container(final_output_pane).width(Length::FillPortion(3)),
    ].spacing(20);

    let controls = row![
        text(&state.status).width(Length::Fill),
        button("OK").on_press(DialogMessage::OkClicked),
        button("Cancel").on_press(DialogMessage::CancelClicked),
    ].spacing(10).align_items(Alignment::Center);

    let dialog_content = container(column![
        text("Manual Track Selection").size(24),
                                   main_content,
                                   controls
    ].spacing(15).padding(10))
    .style(iced::theme::Container::Box);

    container(dialog_content)
    .width(Length::Fixed(1100.0))
    .height(Length::Fixed(650.0))
    .into()
}

fn track_list_group<'a>(title: &'a str, source: &'a str, tracks: &'a [Track]) -> Element<'a, DialogMessage> {
    let mut col = column!().spacing(5);
    if tracks.is_empty() {
        col = col.push(text("No tracks found.").size(14));
    } else {
        for track in tracks {
            let is_disabled_video = track.r#type == "video" && (source == "SEC" || source == "TER");

            let track_info = format!(
                "[{}] ID {} ({}): {}",
                                     track.r#type.chars().next().unwrap_or('?').to_uppercase(),
                                     track.id,
                                     track.properties.language.as_deref().unwrap_or("und"),
                                     track.properties.codec_id.as_deref().unwrap_or("N/A"),
            );

            let mut track_button = button(track_info).width(Length::Fill);
            if !is_disabled_video {
                track_button = track_button.on_press(DialogMessage::AddTrack(source.to_string(), track.clone()));
            }

            col = col.push(track_button);
        }
    }
    column![text(title).size(16), container(col).padding(5)].spacing(5).into()
}

fn selected_track_view(index: usize, selection: &TrackSelection) -> Element<DialogMessage> {
    let badges = format!("{}{}",
                         if selection.is_default { "⭐" } else { "" },
                             if selection.is_forced { "📌" } else { "" }
    );

    let track_info = text(format!(
        "[{}] ID {} ({}) {} {}",
                                  selection.source,
                                  selection.original_track.id,
                                  selection.original_track.properties.language.as_deref().unwrap_or("und"),
                                  selection.original_track.properties.codec_id.as_deref().unwrap_or("N/A"),
                                  badges
    ));

    container(row![
        track_info.width(Length::Fill),
              button("Default").on_press(DialogMessage::ToggleDefault(index)),
              button("Settings..."), // Placeholder for pop-over
              button("Remove").on_press(DialogMessage::RemoveTrack(index)),
    ].spacing(10).align_items(Alignment::Center)).padding(5).style(iced::theme::Container::Box).into()
}

async fn load_track_info(job: Job) -> Result<(Vec<Track>, Vec<Track>, Vec<Track>), String> {
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let runner = CommandRunner::new(Default::default(), tx);

    let ref_tracks = mkv_utils::get_stream_info(&runner, &job.ref_file).await?.tracks;

    let sec_tracks = if let Some(sec_file) = &job.sec_file {
        mkv_utils::get_stream_info(&runner, sec_file).await?.tracks
    } else { vec![] };

    let ter_tracks = if let Some(ter_file) = &job.ter_file {
        mkv_utils::get_stream_info(&runner, ter_file).await?.tracks
    } else { vec![] };

    Ok((ref_tracks, sec_tracks, ter_tracks))
}
