// src/ui/main_window.rs

use iced::widget::{button, checkbox, column, container, progress_bar, row, scrollable, text, text_input};
use iced::{Alignment, Element, Font, Length, Theme};
use crate::{Message, VsgApp};

// Helper functions are no longer generic, they use the default Theme
fn group_box<'a>(title: &str, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    let title_text = text(title).size(16);

    let content_container = container(content).padding(10);

    column![title_text, content_container]
    .spacing(5)
    .into()
}

fn file_input_row<'a>(
    label: &'a str,
    value: &'a str,
    on_change: fn(String) -> Message,
                      on_browse: Message,
) -> Element<'a, Message> {
    row![
        text(label).width(Length::Fixed(80.0)),
        text_input("Select a file or directory...", value).on_input(on_change),
        button("Browse...").on_press(on_browse),
    ]
    .spacing(10)
    .align_items(Alignment::Center)
    .into()
}

// The function signature is now simpler
pub fn view(state: &VsgApp) -> Element<Message> {
    let top_bar = row![
        button("Settings…").on_press(Message::SettingsClicked),
        container(text("")).width(Length::Fill),
    ]
    .padding([0, 10, 10, 10]);

    let input_group = group_box("Input Files (File or Directory)", column![
        file_input_row("Reference:", &state.ref_path, Message::RefPathChanged, Message::BrowseRef),
                                file_input_row("Secondary:", &state.sec_path, Message::SecPathChanged, Message::BrowseSec),
                                file_input_row("Tertiary:", &state.ter_path, Message::TerPathChanged, Message::BrowseTer),
    ].spacing(5));

    let manual_group = group_box("Manual Selection Behavior", column![
        checkbox("Auto-apply this layout", state.auto_apply_layout)
        .on_toggle(Message::AutoApplyToggled),
                                 checkbox("Strict match (type + lang + codec)", state.auto_apply_strict)
                                 .on_toggle(Message::AutoApplyStrictToggled),
    ].spacing(5));

    let actions_group = group_box("Actions", column![
        row![
            button("Analyze Only").on_press(Message::AnalyzeOnlyClicked),
                                  button("Analyze & Merge").on_press(Message::AnalyzeAndMergeClicked),
        ].spacing(10),
                                  checkbox("Archive logs on batch completion", state.archive_logs)
                                  .on_toggle(Message::ArchiveLogsToggled),
    ].spacing(5));

    let status_bar = row![
        text("Status:"),
        text(&state.status_text).width(Length::Fill),
        progress_bar(0.0..=100.0, state.progress),
    ]
    .spacing(10)
    .align_items(Alignment::Center)
    .padding(5);

    let results_group = group_box("Latest Job Results", row![
        text("Secondary Delay:"),
                                  text(&state.sec_delay_text).width(Length::Fill),
                                  text("Tertiary Delay:"),
                                  text(&state.ter_delay_text).width(Length::Fill),
    ].spacing(10));

    let log_group = group_box("Log",
                              scrollable(text(state.log_output.join("\n")).font(Font::MONOSPACE))
                              .height(Length::Fill)
    );

    let content = column![
        top_bar,
        input_group,
        manual_group,
        actions_group,
        status_bar,
        results_group,
        log_group
    ]
    .padding(10)
    .spacing(15);

    container(content).width(Length::Fill).height(Length::Fill).into()
}
