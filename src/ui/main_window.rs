use iced::widget::{button, column, container, row, text, text_input, scrollable, checkbox, Space};
use iced::{Element, Task, Theme, Length, Alignment};
use crate::config::AppConfig;
use crate::ui::components::section;

pub struct MainWindow {
    // Config
    config: AppConfig,

    // UI State - matching Python's MainWindow
    source1_input: String,
    source2_input: String,
    source3_input: String,
    archive_logs: bool,
    log_messages: Vec<String>,

    // Results display
    source2_delay: Option<i32>,
    source3_delay: Option<i32>,
    source4_delay: Option<i32>,

    // Dialog states (placeholders for now)
    show_settings: bool,
    show_job_queue: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    // File inputs
    Source1Changed(String),
    Source2Changed(String),
    Source3Changed(String),
    BrowseSource1,
    BrowseSource2,
    BrowseSource3,

    // Main actions
    OpenJobQueue,
    AnalyzeOnly,
    OpenSettings,

    // Settings
    ArchiveLogsToggled(bool),

    // File dialog results
    FileSelected(Option<String>, SourceField),

    // Log
    AppendLog(String),
}

#[derive(Debug, Clone, Copy)]
pub enum SourceField {
    Source1,
    Source2,
    Source3,
}

impl MainWindow {
    pub fn run() -> iced::Result {
        iced::application("Video/Audio Sync & Merge - Rust Edition", Self::update, Self::view)
        .theme(Self::theme)
        .window_size((1000.0, 600.0))
        .run_with(Self::new)
    }

    fn new() -> (Self, Task<Message>) {
        let config = AppConfig::load();

        let window = Self {
            source1_input: config.last_ref_path.clone(),
            source2_input: config.last_sec_path.clone(),
            source3_input: config.last_ter_path.clone(),
            archive_logs: config.archive_logs,
            log_messages: vec!["Ready".to_string()],
            source2_delay: None,
            source3_delay: None,
            source4_delay: None,
            show_settings: false,
            show_job_queue: false,
            config,
        };

        (window, Task::none())
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }

    fn append_log(&mut self, message: String) {
        let timestamp = chrono::Local::now().format("%H:%M:%S");
        self.log_messages.push(format!("[{}] {}", timestamp, message));
    }

    fn save_config(&mut self) {
        self.config.last_ref_path = self.source1_input.clone();
        self.config.last_sec_path = self.source2_input.clone();
        self.config.last_ter_path = self.source3_input.clone();
        self.config.archive_logs = self.archive_logs;
        self.config.save();
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Source1Changed(value) => {
                self.source1_input = value;
                Task::none()
            }
            Message::Source2Changed(value) => {
                self.source2_input = value;
                Task::none()
            }
            Message::Source3Changed(value) => {
                self.source3_input = value;
                Task::none()
            }
            Message::BrowseSource1 => {
                Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                        .pick_file()
                        .await
                        .map(|f| f.path().display().to_string())
                    },
                    |result| Message::FileSelected(result, SourceField::Source1),
                )
            }
            Message::BrowseSource2 => {
                Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                        .pick_file()
                        .await
                        .map(|f| f.path().display().to_string())
                    },
                    |result| Message::FileSelected(result, SourceField::Source2),
                )
            }
            Message::BrowseSource3 => {
                Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                        .pick_file()
                        .await
                        .map(|f| f.path().display().to_string())
                    },
                    |result| Message::FileSelected(result, SourceField::Source3),
                )
            }
            Message::FileSelected(Some(path), field) => {
                match field {
                    SourceField::Source1 => self.source1_input = path,
                    SourceField::Source2 => self.source2_input = path,
                    SourceField::Source3 => self.source3_input = path,
                }
                Task::none()
            }
            Message::FileSelected(None, _) => Task::none(),

            Message::OpenSettings => {
                self.append_log("Opening settings dialog...".to_string());
                self.show_settings = true;
                // TODO: Launch settings dialog
                Task::none()
            }

            Message::OpenJobQueue => {
                self.append_log("Opening job queue...".to_string());
                self.save_config();
                self.show_job_queue = true;
                // TODO: Launch job queue dialog
                Task::none()
            }

            Message::AnalyzeOnly => {
                self.append_log("Starting analysis...".to_string());
                self.save_config();
                // TODO: Run analysis
                Task::none()
            }

            Message::ArchiveLogsToggled(value) => {
                self.archive_logs = value;
                Task::none()
            }

            Message::AppendLog(msg) => {
                self.append_log(msg);
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let top_row = row![
            button("Settings…").on_press(Message::OpenSettings),
            Space::with_width(Length::Fill),
        ]
        .padding(10)
        .spacing(10);

        // Main Workflow section (matching Python's actions_group)
        let main_workflow = section(
            "Main Workflow",
            column![
                button(text("Open Job Queue for Merging...").size(14))
                .on_press(Message::OpenJobQueue)
                .padding([10, 20])
                .width(Length::Fill),
                                    checkbox("Archive logs to a zip file on batch completion", self.archive_logs)
                                    .on_toggle(Message::ArchiveLogsToggled),
            ]
            .spacing(10),
        );

        // Quick Analysis section (matching Python's analysis_group)
        let quick_analysis = section(
            "Quick Analysis (Analyze Only)",
                                     column![
                                         self.create_file_input_row(
                                             "Source 1 (Reference):",
                                                                    &self.source1_input,
                                                                    Message::Source1Changed,
                                                                    Message::BrowseSource1,
                                         ),
                                     self.create_file_input_row(
                                         "Source 2:",
                                         &self.source2_input,
                                         Message::Source2Changed,
                                         Message::BrowseSource2,
                                     ),
                                     self.create_file_input_row(
                                         "Source 3:",
                                         &self.source3_input,
                                         Message::Source3Changed,
                                         Message::BrowseSource3,
                                     ),
                                     row![
                                         Space::with_width(Length::Fill),
                                     button("Analyze Only").on_press(Message::AnalyzeOnly),
                                     ]
                                     .padding([10, 0])
                                     ]
                                     .spacing(10),
        );

        // Latest Job Results section
        let results = section(
            "Latest Job Results",
            row![
                text("Source 2 Delay:"),
                              text(self.source2_delay.map_or("—".to_string(), |d| format!("{} ms", d))),
                              Space::with_width(Length::Fixed(20.0)),
                              text("Source 3 Delay:"),
                              text(self.source3_delay.map_or("—".to_string(), |d| format!("{} ms", d))),
                              Space::with_width(Length::Fixed(20.0)),
                              text("Source 4 Delay:"),
                              text(self.source4_delay.map_or("—".to_string(), |d| format!("{} ms", d))),
                              Space::with_width(Length::Fill),
            ]
            .spacing(10)
            .padding(10)
            .align_y(Alignment::Center),
        );

        // Log section
        let log_section = section(
            "Log",
            container(
                scrollable(
                    column(
                        self.log_messages
                        .iter()
                        .map(|msg| text(msg).size(12).into())
                        .collect(),
                    )
                    .spacing(2)
                )
                .height(Length::Fill)
            )
            .height(Length::Fill)
            .padding(10),
        );

        let content = column![
            top_row,
            main_workflow,
            quick_analysis,
            results,
            log_section,
        ]
        .spacing(10)
        .padding(10);

        container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn create_file_input_row<'a>(
        &self,
        label: &str,
        value: &str,
        on_input: impl Fn(String) -> Message + 'a,
                                 on_browse: Message,
    ) -> Element<'a, Message> {
        row![
            text(label).width(Length::FillPortion(2)),
            text_input("", value)
            .on_input(on_input)
            .width(Length::FillPortion(6)),
            button("Browse…")
            .on_press(on_browse)
            .width(Length::FillPortion(1)),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
    }
}
