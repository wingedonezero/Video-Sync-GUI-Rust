// src/ui/manual_selection_dialog.rs

use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Command, Element, Length};
use crate::core::mkv_utils::{self, Track};
use crate::core::pipeline::Job;
use crate::core::process::CommandRunner;
use crate::VsgApp; // Import the main app state

// State for the Manual Selection window
pub struct ManualSelection {
    pub job: Job,
    pub available_ref_tracks: Vec<Track>,
    pub available_sec_tracks: Vec<Track>,
    pub available_ter_tracks: Vec<Track>,
    // This will hold the user's final track layout
    // pub selected_tracks: Vec<Track>,
    pub status: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    // Message sent when the window is first loaded
    Loaded,
    // Message to receive the results of probing the source files
    TracksInfoReady(Result<(Vec<Track>, Vec<Track>, Vec<Track>), String>),
    // Placeholder messages for interactivity
    AddTrack(u64),
    OkClicked,
    CancelClicked,
}

impl ManualSelection {
    pub fn new(job: Job) -> (Self, Command<Message>) {
        let selection = Self {
            job,
            available_ref_tracks: vec![],
            available_sec_tracks: vec![],
            available_ter_tracks: vec![],
            status: "Loading track info...".to_string(),
        };

        // When the window is created, we immediately issue a command
        // to load the track information in the background.
        (selection, Command::perform(load_track_info(selection.job.clone()), Message::TracksInfoReady))
    }

    pub fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Loaded => {
                // This is where we could do additional setup if needed
            }
            Message::TracksInfoReady(Ok((ref_tracks, sec_tracks, ter_tracks))) => {
                self.available_ref_tracks = ref_tracks;
                self.available_sec_tracks = sec_tracks;
                self.available_ter_tracks = ter_tracks;
                self.status = "Ready".to_string();
            }
            Message::TracksInfoReady(Err(e)) => {
                self.status = format!("Error loading tracks: {}", e);
            }
            _ => {
                // Handle other messages later
            }
        }
        Command::none()
    }

    pub fn view(&self) -> Element<Message> {
        let available_tracks_pane = scrollable(column![
            track_list_group("Reference Tracks", &self.available_ref_tracks),
                                               track_list_group("Secondary Tracks", &self.available_sec_tracks),
                                               track_list_group("Tertiary Tracks", &self.available_ter_tracks),
        ].spacing(20));

        let final_output_pane = scrollable(
            text("Final output tracks will go here.")
        );

        let main_content = row![
            container(available_tracks_pane).width(Length::FillPortion(1)),
            container(final_output_pane).width(Length::FillPortion(2)),
        ].spacing(20);

        let controls = row![
            text(&self.status).width(Length::Fill),
            button("OK").on_press(Message::OkClicked),
            button("Cancel").on_press(Message::CancelClicked),
        ].spacing(10).align_items(Alignment::Center);

        container(column![main_content, controls].spacing(10))
        .padding(10)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}

// Helper to display a list of tracks in a groupbox
fn track_list_group(title: &str, tracks: &[Track]) -> Element<Message> {
    let mut col = column!().spacing(5);
    if tracks.is_empty() {
        col = col.push(text("No tracks found.").size(14));
    } else {
        for track in tracks {
            let track_info = format!(
                "[{}] ID {} ({}): {}",
                                     track.r#type.chars().next().unwrap_or('?').to_uppercase(),
                                     track.id,
                                     track.properties.language.as_deref().unwrap_or("und"),
                                     track.properties.codec_id.as_deref().unwrap_or("N/A"),
            );
            col = col.push(button(track_info).width(Length::Fill).on_press(Message::AddTrack(track.id)));
        }
    }

    column![text(title).size(16), container(col).padding(5)]
    .spacing(5)
    .into()
}


// Async function to load all track info for the job
async fn load_track_info(job: Job) -> Result<(Vec<Track>, Vec<Track>, Vec<Track>), String> {
    let (tx, mut rx) = tokio::sync::mpsc::channel(1); // Dummy channel, not used for UI logs
    tokio::spawn(async move { while let Some(_) = rx.recv().await {} });
    let runner = CommandRunner::new(tx);

    let mut ref_tracks = vec![];
    let mut sec_tracks = vec![];
    let mut ter_tracks = vec![];

    ref_tracks = mkv_utils::get_stream_info(&runner, &job.ref_file).await?.tracks;

    if let Some(sec_file) = &job.sec_file {
        sec_tracks = mkv_utils::get_stream_info(&runner, sec_file).await?.tracks;
    }
    if let Some(ter_file) = &job.ter_file {
        ter_tracks = mkv_utils::get_stream_info(&runner, ter_file).await?.tracks;
    }

    Ok((ref_tracks, sec_tracks, ter_tracks))
}
