use crate::Message;
use iced::widget::{button, column, row, text, Space};
use iced::{Alignment, Element, Length};
use iced_advanced::Card; // UPDATED

pub fn view<'a>(on_close: Message) -> Element<'a, Message> { // UPDATED
    let header = text("Job Queue");

    let body = column![
        text("Job list will appear here.").height(Length::Fill),
        row![
            button("Add Job(s)..."),
            Space::with_width(Length::Fill),
            button("Move Up"),
            button("Move Down"),
            button("Remove Selected"),
        ].spacing(10)
    ]
    .spacing(10)
    .width(Length::Fill);

    let footer = row![
        Space::with_width(Length::Fill),
        button("Cancel").on_press(on_close.clone()),
        button("Start Processing Queue")
    ]
    .spacing(10)
    .align_items(Alignment::Center);

    Card::new(header, body)
    .foot(footer)
    .max_width(1200.0)
    .on_close(on_close) // UPDATED
    .into()
}
