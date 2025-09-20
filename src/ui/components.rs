use iced::widget::{column, container, text};
use iced::{Element, Border, Theme, Length};

/// Creates a section with title and content, matching Python's QGroupBox style
pub fn section<'a, Message: 'a>(
    title: &'a str,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(
        column![
            text(title).size(14),
              container(content)
              .padding(10)
              .style(|theme: &Theme| {
                  let palette = theme.extended_palette();
                  container::Style {
                      text_color: Some(palette.background.base.text),
                     background: Some(palette.background.weak.color.into()),
                     border: Border {
                         color: palette.background.strong.color,
                         width: 1.0,
                         radius: 4.0.into(),
                     },
                     shadow: Default::default(),
                  }
              })
        ]
        .spacing(5)
    )
    .padding(5)
    .width(Length::Fill)
    .into()
}
