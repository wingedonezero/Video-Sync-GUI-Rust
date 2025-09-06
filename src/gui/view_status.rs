use iced::widget::{column, container, progress_bar, row, scrollable, text, Space};
use iced::{Element, Length};
use crate::gui::theme::card_container_style;

pub struct Status<'a> {
    pub status: &'a str, pub progress: f32,
    pub sec_delay: Option<i64>, pub ter_delay: Option<i64>,
    pub log_text: &'a str,
}
impl<'a> Status<'a> {
    pub fn view<Message: 'static>(self) -> Element<'static, Message> {
        let status_row = row![
            text("Status:"), text(self.status).width(Length::Fill),
            progress_bar(0.0..=1.0, self.progress).width(Length::Fixed(220.0))
        ].spacing(10);

        let results = container(
            row![
                text("Secondary Delay:"), text(self.sec_delay.map(|v| format!("{v} ms")).unwrap_or_else(|| "—".into())),
                                Space::with_width(Length::Fixed(20.0)),
                                text("Tertiary Delay:"), text(self.ter_delay.map(|v| format!("{v} ms")).unwrap_or_else(|| "—".into())),
                                Space::with_width(Length::Fill)
            ].spacing(10)
        ).padding(10).style(card_container_style());

        let log = container(scrollable(text(self.log_text).width(Length::Fill)))
        .height(Length::Fill).style(card_container_style());

        column![status_row, results, log].spacing(12).padding([0,10,10,10]).into()
    }
}
