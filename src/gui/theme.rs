use iced::{theme, Theme, Color};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiTheme { Oxocarbon, Light, Dark }

impl UiTheme {
    pub const ALL: [UiTheme; 3] = [UiTheme::Oxocarbon, UiTheme::Light, UiTheme::Dark];
}
impl std::fmt::Display for UiTheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UiTheme::Oxocarbon => write!(f,"Oxocarbon"),
            UiTheme::Light => write!(f,"Light"),
            UiTheme::Dark  => write!(f,"Dark"),
        }
    }
}

pub fn theme_of(t: UiTheme) -> Theme {
    match t {
        UiTheme::Light => Theme::Light,
        UiTheme::Dark  => Theme::Dark,
        UiTheme::Oxocarbon => {
            use iced::theme::Palette;
            Theme::custom(theme::Custom::new(
                "Oxocarbon".into(),
                                             Palette {
                                                 background: Color::from_rgb8(0x16,0x16,0x1B),
                                             text:       Color::from_rgb8(0xE0,0xE0,0xE0),
                                             primary:    Color::from_rgb8(0x78,0xA9,0xFF),
                                             success:    Color::from_rgb8(0x42,0xBE,0x65),
                                             danger:     Color::from_rgb8(0xFA,0x4D,0x56),
                                             }
            ))
        }
    }
}

pub fn card_container_style(
) -> impl Fn(&iced::theme::Container, &iced::Theme) -> iced::renderer::Style + Clone {
    move |_style, theme| {
        let p = theme.extended_palette();
        iced::renderer::Style {
            background: Some(iced::Background::Color(p.background.strong.color)),
            text_color: None,
            border: iced::Border { color: p.background.base.color, width: 1.0, radius: 8.0.into() },
            shadow: Default::default(),
        }
    }
}
