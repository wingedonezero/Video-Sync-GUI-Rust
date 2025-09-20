use crate::{Message, QuickAnalysisSource};
use iced::widget::{
    button, checkbox, column, container, progress_bar, row, scrollable, text, text_input, Column,
    Container, Row, Rule, Space,
};
use iced::{Alignment, Element, Length};

fn styled_container<'a>(content: impl Into<Element<'a, Message>>) -> Container<'a, Message> {
    container(content).padding(5)
}

fn group_box<'a>(title: &str, content: Column<'a, Message>) -> Column<'a, Message> {
    column![
        text(title).size(20),
        Rule::horizontal(1),
        container(content).padding(10)
    ]
    .spacing(5)
}

fn file_input_row<'a>(
    label: &'a str,
    value: &'a str,
    on_change: impl Fn(String) -> Message + 'a,
) -> Row<'a, Message> {
    row![
        text(label).width(Length::FillPortion(2)),
        text_input("Select file or directory...", value)
        .on_input(on_change)
        .width(Length::FillPortion(7)),
        button("Browse...").width(Length::FillPortion(1))
    ]
    .spacing(10)
    .align_items(Alignment::Center)
}

pub fn view<'a>(
    log_output: &'a str,
    status_text: &'a str,
    progress: f32,
    ref_path: &'a str,
    sec_path: &'a str,
    ter_path: &'a str,
    archive_logs: bool,
) -> Element<'a, Message> {
    let top_row = row![
        button("Settings...").on_press(Message::OpenOptions),
        Space::with_width(Length::Fill)
    ];

    let actions_group = group_box(
        "Main Workflow",
        column![
            button("Open Job Queue for Merging...")
            .on_press(Message::OpenJobQueue)
            .padding(10),
                                  checkbox("Archive logs to a zip file on batch completion", archive_logs)
                                  .on_toggle(Message::ArchiveLogsToggled)
        ]
        .spacing(10),
    );

    let analysis_group = group_box(
        "Quick Analysis (Analyze Only)",
                                   column![
                                       file_input_row("Source 1 (Reference):", ref_path, |text| {
                                           Message::QuickAnalysisInputChanged {
                                               source: QuickAnalysisSource::Reference,
                                               text,
                                           }
                                       }),
                                   file_input_row("Source 2:", sec_path, |text| {
                                       Message::QuickAnalysisInputChanged {
                                           source: QuickAnalysisSource::Secondary,
                                           text,
                                       }
                                   }),
                                   file_input_row("Source 3:", ter_path, |text| {
                                       Message::QuickAnalysisInputChanged {
                                           source: QuickAnalysisSource::Tertiary,
                                           text,
                                       }
                                   }),
                                   row![
                                       Space::with_width(Length::Fill),
                                   button("Analyze Only").on_press(Message::AnalyzeOnlyPressed)
                                   ]
                                   ]
                                   .spacing(10),
    );

    let status_bar = row![
        text("Status:"),
        Space::with_width(10),
        text(status_text).width(Length::Fill),
        progress_bar(0.0..=1.0, progress)
        .width(Length::Fixed(200.0))
    ]
    .align_items(Alignment::Center)
    .spacing(10);

    let results_group = group_box(
        "Latest Job Results",
        row![
            text("Source 2 Delay:"),
                                  text("—").width(100),
                                  text("Source 3 Delay:"),
                                  text("—").width(100),
                                  text("Source 4 Delay:"),
                                  text("—").width(100),
                                  Space::with_width(Length::Fill)
        ]
        .spacing(10),
    );

    let log_group = group_box(
        "Log",
        column![scrollable(text(log_output))],
    );

    let main_content = column![
        top_row,
        actions_group,
        analysis_group,
        status_bar,
        results_group,
        log_group,
    ]
    .spacing(15);

    styled_container(main_content).into()
}
