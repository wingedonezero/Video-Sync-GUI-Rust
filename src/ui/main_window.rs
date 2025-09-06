// src/ui/main_window.rs

use iced::widget::{
    button, checkbox, column, container, progress_bar, row, scrollable, text, text_input,
};
use iced::{Alignment, Element, Font, Length};

use crate::{Message, VsgApp};

// Helper: simple group box (kept same)
fn group_box<'a>(
    title: &'a str,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let title_text = text(title).size(16);
    let content_container = container(content).padding(10u16);

    column![title_text, content_container].spacing(5).into()
}

// Helper: file input row (kept same, 0.13 API tweaks)
fn file_input_row<'a>(
    label: &'a str,
    value: &'a str,
    on_change: fn(String) -> Message,
                      on_browse: Message,
) -> Element<'a, Message> {
    row![
        text(label).width(Length::Fixed(80.0)),
        text_input("Select a file or directory...", value).on_input(on_change),
        button(text("Browse...")).on_press(on_browse),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

pub fn view(state: &VsgApp) -> Element<Message> {
    // Top bar with Settings
    let top_bar = row![
        button(text("Settings…")).on_press(Message::OpenSettings),
        container(text("")).width(Length::Fill),
    ]
    .padding(10u16);

    // Inputs group
    let input_group = group_box(
        "Input Files (File or Directory)",
                                column![
                                    file_input_row(
                                        "Reference:",
                                        &state.ref_path,
                                        Message::RefPathChanged,
                                        Message::BrowseRef
                                    ),
                                file_input_row(
                                    "Secondary:",
                                    &state.sec_path,
                                    Message::SecPathChanged,
                                    Message::BrowseSec
                                ),
                                file_input_row(
                                    "Tertiary:",
                                    &state.ter_path,
                                    Message::TerPathChanged,
                                    Message::BrowseTer
                                ),
                                ]
                                .spacing(5),
    );

    // Manual selection behavior
    let manual_group = group_box(
        "Manual Selection Behavior",
        column![
            checkbox("Auto-apply this layout", state.auto_apply_layout)
            .on_toggle(Message::AutoApplyToggled),
                                 checkbox(
                                     "Strict match (type + lang + codec)",
                                          state.auto_apply_strict
                                 )
                                 .on_toggle(Message::AutoApplyStrictToggled),
        ]
        .spacing(5),
    );

    // Action buttons: disabled while running
    let mut analyze_button = button(text("Analyze Only"));
    let mut merge_button = button(text("Analyze & Merge"));

    if !state.is_running {
        analyze_button = analyze_button.on_press(Message::StartJob(false));
        merge_button = merge_button.on_press(Message::StartJob(true));
    }

    let actions_group = group_box(
        "Actions",
        column![
            row![analyze_button, merge_button].spacing(10),
                                  checkbox("Archive logs on batch completion", state.archive_logs)
                                  .on_toggle(Message::ArchiveLogsToggled),
        ]
        .spacing(5),
    );

    // Status / progress
    let status_bar = row![
        text("Status:"),
        text(&state.status_text).width(Length::Fill),
        progress_bar(0.0..=100.0, state.progress),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .padding(5u16);

    // Results
    let results_group = group_box(
        "Latest Job Results",
        row![
            text("Secondary Delay:"),
                                  text(&state.sec_delay_text).width(Length::Fill),
                                  text("Tertiary Delay:"),
                                  text(&state.ter_delay_text).width(Length::Fill),
        ]
        .spacing(10),
    );

    // Log
    let log_group = group_box(
        "Log",
        scrollable(text(state.log_output.join("\n")).font(Font::MONOSPACE))
        .height(Length::Fill),
    );

    // Layout
    let content = column![
        top_bar,
        input_group,
        manual_group,
        actions_group,
        status_bar,
        results_group,
        log_group
    ]
    .padding(10u16)
    .spacing(15);

    container(content)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
