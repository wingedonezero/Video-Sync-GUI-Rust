use iced::widget::{button, checkbox, column, container, pick_list, row, text, text_input, Space};
use iced::{Element, Length};
use crate::gui::theme::{UiTheme, card_container_style};

#[derive(Debug, Clone)]
pub enum Msg {
    BrowseRef, BrowseSec, BrowseTer,
    RefChanged(String), SecChanged(String), TerChanged(String),
    AutoApply(bool), AutoApplyStrict(bool),
    ArchiveLogs(bool),
    AnalyzeOnly, AnalyzeMerge,
    ThemeChanged(UiTheme),
    OpenSettings,
}

pub struct Inputs<'a> {
    pub ref_path: &'a str, pub sec_path: &'a str, pub ter_path: &'a str,
    pub auto_apply: bool, pub auto_apply_strict: bool, pub archive_logs: bool,
    pub theme_choice: UiTheme,
}
impl<'a> Inputs<'a> {
    pub fn view(self) -> Element<'static, Msg> {
        let top = row![
            button("Settings…").on_press(Msg::OpenSettings),
            Space::with_width(Length::Fill),
            text("Theme:"),
            pick_list(&UiTheme::ALL[..], Some(self.theme_choice), Msg::ThemeChanged)
        ].spacing(10);

        let inputs = container(column![
            row![ text("Reference:").width(Length::Fixed(110.0)),
                               text_input("File or directory", self.ref_path, Msg::RefChanged).width(Length::Fill),
                               button("Browse…").on_press(Msg::BrowseRef) ].spacing(8),
                               row![ text("Secondary:").width(Length::Fixed(110.0)),
                               text_input("Optional", self.sec_path, Msg::SecChanged).width(Length::Fill),
                               button("Browse…").on_press(Msg::BrowseSec) ].spacing(8),
                               row![ text("Tertiary:").width(Length::Fixed(110.0)),
                               text_input("Optional", self.ter_path, Msg::TerChanged).width(Length::Fill),
                               button("Browse…").on_press(Msg::BrowseTer) ].spacing(8),
        ].spacing(10)).padding(10).style(card_container_style());

        let manual = container(column![
            iced::widget::text("For Analyze & Merge, you’ll select tracks per file. Auto-apply reuses your last layout when the track signature matches.").size(14),
                               checkbox("Auto-apply this layout to all matching files in batch", self.auto_apply, Msg::AutoApply),
                               checkbox("Strict match (type + lang + codec)", self.auto_apply_strict, Msg::AutoApplyStrict),
        ].spacing(8)).padding(10).style(card_container_style());

        let actions = container(column![
            row![ button("Analyze Only").on_press(Msg::AnalyzeOnly),
                                button("Analyze  Merge").on_press(Msg::AnalyzeMerge),
                                Space::with_width(Length::Fill), ].spacing(10),
                                checkbox("Archive logs to a zip file on batch completion", self.archive_logs, Msg::ArchiveLogs),
        ].spacing(10)).padding(10).style(card_container_style());

        column![top, inputs, manual, actions].spacing(12).padding(10).into()
    }
}
