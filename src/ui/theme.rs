// src/ui/theme.rs

use iced::widget::{button, checkbox, container, progress_bar, scrollable, text, text_input};
use iced::{application, color, Border, Vector, Theme};

#[derive(Debug, Clone, Copy, Default)]
pub struct VsgTheme;

// A more refined color palette
const BACKGROUND_DARK: iced::Color = color!(0x1d, 0x20, 0x21);
const BACKGROUND_LIGHT: iced::Color = color!(0x28, 0x28, 0x28);
const FOREGROUND: iced::Color = color!(0xeb, 0xdb, 0xb2);
const PRIMARY_ACCENT: iced::Color = color!(0x83, 0xa5, 0x98);
const BORDER_COLOR: iced::Color = color!(0x50, 0x49, 0x45);

// We can define different styles for the same widget using an enum
#[derive(Debug, Clone, Copy, Default)]
pub enum Container {
    #[default]
    Default,
    GroupBox,
}

impl container::StyleSheet for VsgTheme {
    type Style = Container;

    fn appearance(&self, style: &Self::Style) -> container::Appearance {
        match style {
            Container::Default => container::Appearance {
                background: Some(BACKGROUND_DARK.into()),
                text_color: Some(FOREGROUND),
                ..Default::default()
            },
            Container::GroupBox => container::Appearance {
                background: Some(BACKGROUND_LIGHT.into()),
                border: Border {
                    color: BORDER_COLOR,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            },
        }
    }
}

// The default text appearance is handled by the application stylesheet.
// We will apply special fonts like monospace directly on the widget.
impl text::StyleSheet for VsgTheme {
    type Style = Theme;
    fn appearance(&self, _style: Self::Style) -> text::Appearance {
        text::Appearance { color: Some(FOREGROUND) }
    }
}

impl application::StyleSheet for VsgTheme {
    type Style = Theme;
    fn appearance(&self, _style: &Self::Style) -> application::Appearance {
        application::Appearance {
            background_color: BACKGROUND_DARK,
            text_color: FOREGROUND,
        }
    }
}

// --- The rest of the StyleSheet implementations are correct and unchanged ---

impl button::StyleSheet for VsgTheme {
    type Style = Theme;
    fn active(&self, _style: &Self::Style) -> button::Appearance {
        button::Appearance {
            background: Some(PRIMARY_ACCENT.into()),
            text_color: BACKGROUND_DARK,
            border: Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn hovered(&self, style: &Self::Style) -> button::Appearance {
        let active = self.active(style);
        button::Appearance {
            shadow_offset: active.shadow_offset + Vector::new(0.0, 1.0),
            ..active
        }
    }
}

impl checkbox::StyleSheet for VsgTheme {
    type Style = Theme;
    fn active(&self, _style: &Self::Style, _is_checked: bool) -> checkbox::Appearance {
        checkbox::Appearance {
            background: BACKGROUND_LIGHT.into(),
            icon_color: FOREGROUND,
            border: Border {
                color: BORDER_COLOR,
                width: 1.0,
                radius: 2.0.into(),
            },
            text_color: Some(FOREGROUND),
        }
    }

    fn hovered(&self, style: &Self::Style, is_checked: bool) -> checkbox::Appearance {
        let active = self.active(style, is_checked);
        checkbox::Appearance {
            background: color!(0x4c, 0x48, 0x46).into(),
            ..active
        }
    }
}

impl scrollable::StyleSheet for VsgTheme {
    type Style = Theme;
    fn active(&self, _style: &Self::Style) -> scrollable::Appearance {
        scrollable::Appearance {
            container: container::Appearance {
                background: Some(BACKGROUND_LIGHT.into()),
                ..Default::default()
            },
            scrollbar: scrollable::Scrollbar {
                background: Some(BACKGROUND_DARK.into()),
                border: Border {
                    color: BORDER_COLOR,
                    width: 0.5,
                    radius: 2.0.into(),
                },
                scroller: scrollable::Scroller {
                    color: PRIMARY_ACCENT,
                    border: Border {
                        color: BORDER_COLOR,
                        width: 0.5,
                        radius: 2.0.into(),
                    },
                },
            },
            gap: None
        }
    }

    fn hovered(&self, style: &Self::Style, _is_grabbed: bool) -> scrollable::Appearance {
        self.active(style)
    }
}

impl text_input::StyleSheet for VsgTheme {
    type Style = Theme;
    fn active(&self, _style: &Self::Style) -> text_input::Appearance {
        text_input::Appearance {
            background: BACKGROUND_LIGHT.into(),
            border: Border {
                color: BORDER_COLOR,
                width: 1.0,
                radius: 2.0.into(),
            },
            icon_color: FOREGROUND,
        }
    }

    fn focused(&self, style: &Self::Style) -> text_input::Appearance {
        text_input::Appearance {
            border: Border {
                color: PRIMARY_ACCENT,
                ..self.active(style).border
            },
            ..self.active(style)
        }
    }

    fn placeholder_color(&self, _style: &Self::Style) -> iced::Color {
        color!(0x7c, 0x6f, 0x64)
    }

    fn value_color(&self, _style: &Self::Style) -> iced::Color {
        FOREGROUND
    }

    fn selection_color(&self, _style: &Self::Style) -> iced::Color {
        PRIMARY_ACCENT
    }

    fn disabled(&self, style: &Self::Style) -> text_input::Appearance {
        let active = self.active(style);
        text_input::Appearance {
            background: color!(0x20, 0x20, 0x20).into(),
            ..active
        }
    }

    fn disabled_color(&self, _style: &Self::Style) -> iced::Color {
        color!(0x7c, 0x6f, 0x64)
    }
}

impl progress_bar::StyleSheet for VsgTheme {
    type Style = Theme;
    fn appearance(&self, _style: &Self::Style) -> progress_bar::Appearance {
        progress_bar::Appearance {
            background: BORDER_COLOR.into(),
            bar: PRIMARY_ACCENT.into(),
            border_radius: 2.0.into(),
        }
    }
}
