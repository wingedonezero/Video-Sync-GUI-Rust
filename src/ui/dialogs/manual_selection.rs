use iced::widget::{button, column, container, row, text, Space};
use iced::{Element, Length};

pub struct ManualSelectionDialog {
    // Track info from sources
    available_tracks: Vec<TrackInfo>,
    // Selected tracks for output
    selected_tracks: Vec<TrackInfo>,
    // Attachment sources
    attachment_sources: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TrackInfo {
    pub source: String,
    pub id: u32,
    pub track_type: String,
    pub codec: String,
    pub lang: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    AddTrack(usize),
    RemoveTrack(usize),
    MoveUp(usize),
    MoveDown(usize),
    SetDefault(usize),
    Ok,
    Cancel,
}

impl ManualSelectionDialog {
    pub fn new() -> Self {
        Self {
            available_tracks: Vec::new(),
            selected_tracks: Vec::new(),
            attachment_sources: Vec::new(),
        }
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::AddTrack(idx) => {
                if idx < self.available_tracks.len() {
                    self.selected_tracks.push(self.available_tracks[idx].clone());
                }
            }
            Message::RemoveTrack(idx) => {
                if idx < self.selected_tracks.len() {
                    self.selected_tracks.remove(idx);
                }
            }
            Message::MoveUp(idx) => {
                if idx > 0 && idx < self.selected_tracks.len() {
                    self.selected_tracks.swap(idx, idx - 1);
                }
            }
            Message::MoveDown(idx) => {
                if idx < self.selected_tracks.len() - 1 {
                    self.selected_tracks.swap(idx, idx + 1);
                }
            }
            Message::SetDefault(_idx) => {
                // TODO: Set default flag
            }
            Message::Ok => {
                // TODO: Save selection and close
            }
            Message::Cancel => {
                // TODO: Close without saving
            }
        }
    }

    pub fn view(&self) -> Element<Message> {
        let left_pane = container(
            column![
                text("Available Tracks"),
                                  container(text("Track list will go here"))
                                  .width(Length::Fill)
                                  .height(Length::Fill),
            ]
        )
        .width(Length::FillPortion(1))
        .height(Length::Fill);

        let right_pane = container(
            column![
                text("Final Output (Drag to reorder)"),
                                   container(text("Selected tracks will go here"))
                                   .width(Length::Fill)
                                   .height(Length::Fill),
            ]
        )
        .width(Length::FillPortion(2))
        .height(Length::Fill);

        let dialog_buttons = row![
            Space::with_width(Length::Fill),
            button("OK").on_press(Message::Ok),
            button("Cancel").on_press(Message::Cancel),
        ]
        .spacing(10)
        .padding(10);

        column![
            row![left_pane, right_pane]
            .spacing(10)
            .height(Length::FillPortion(5)),
            dialog_buttons,
        ]
        .into()
    }
}
