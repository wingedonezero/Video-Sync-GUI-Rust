use crate::Message;
use iced::widget::{button, column, row, text, Space};
use iced::{Alignment, Element, Length};
use iced_aw::{Card, TabBar, TabLabel};

pub fn view<'a>() -> Card<'a, Message> {
    let header = text("Application Settings");

    // Placeholder content for the tabs
    let tab_content = |title| {
        column![text(format!("Settings for {} will go here.", title))]
        .width(Length::Fill)
        .align_items(Alignment::Center)
    };

    let tabs = TabBar::new(0, Message::NoOp)
    .push(TabLabel::Text("Storage".to_string()), tab_content("Storage"))
    .push(TabLabel::Text("Analysis".to_string()), tab_content("Analysis"))
    .push(TabLabel::Text("Chapters".to_string()), tab_content("Chapters"))
    .push(
        TabLabel::Text("Merge Behavior".to_string()),
          tab_content("Merge Behavior"),
    )
    .push(TabLabel::Text("Logging".to_string()), tab_content("Logging"));

    let body = column![tabs].height(Length::Fixed(500.0));

    let footer = row![
        Space::with_width(Length::Fill),
        button("Cancel").on_press(Message::CloseOptions),
        button("Save")
    ]
    .spacing(10)
    .align_items(Alignment::Center);

    Card::new(header, body)
    .foot(footer)
    .max_width(900.0)
    .style(iced_aw::style::Card::Primary)
}
