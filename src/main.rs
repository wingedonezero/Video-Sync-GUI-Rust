use iced::widget::{column, container};
use iced::{executor, Application, Command, Element, Length, Settings, Theme};
use iced_advanced::Modal; // UPDATED

mod ui;
use ui::{job_queue_dialog, main_window, options_dialog};

fn main() -> iced::Result {
    VsgApp::run(Settings {
        window: iced::window::Settings {
            size: (1000, 700),
                ..Default::default()
        },
        ..Settings::default()
    })
}

// Struct to hold the entire state of our application
pub struct VsgApp {
    log_output: String,
    status_text: String,
    progress: f32,
    quick_analysis_ref: String,
    quick_analysis_sec: String,
    quick_analysis_ter: String,
    archive_logs: bool,
    show_job_queue: bool,
    show_options: bool,
}

// Enum to represent every possible user interaction
#[derive(Debug, Clone)]
pub enum Message {
    NoOp, // Placeholder for actions that do nothing yet
    OpenJobQueue,
    CloseJobQueue,
    OpenOptions,
    CloseOptions,
    QuickAnalysisInputChanged {
        source: QuickAnalysisSource,
        text: String,
    },
    ArchiveLogsToggled(bool),
    AnalyzeOnlyPressed,
}

#[derive(Debug, Clone)]
pub enum QuickAnalysisSource {
    Reference,
    Secondary,
    Tertiary,
}

impl Application for VsgApp {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        (
            Self {
                log_output: "Welcome to the Rust version of Video Sync & Merge!\nLog output will appear here.".to_string(),
         status_text: "Ready".to_string(),
         progress: 0.0,
         quick_analysis_ref: "".to_string(),
         quick_analysis_sec: "".to_string(),
         quick_analysis_ter: "".to_string(),
         archive_logs: true,
         show_job_queue: false,
         show_options: false,
            },
         Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("Video/Audio Sync & Merge - Rust/Iced Edition")
    }

    // This function is called whenever a Message is received (e.g., a button is clicked)
    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::OpenJobQueue => self.show_job_queue = true,
            Message::CloseJobQueue => self.show_job_queue = false,
            Message::OpenOptions => self.show_options = true,
            Message::CloseOptions => self.show_options = false,
            Message::QuickAnalysisInputChanged { source, text } => match source {
                QuickAnalysisSource::Reference => self.quick_analysis_ref = text,
                QuickAnalysisSource::Secondary => self.quick_analysis_sec = text,
                QuickAnalysisSource::Tertiary => self.quick_analysis_ter = text,
            },
            Message::ArchiveLogsToggled(is_checked) => self.archive_logs = is_checked,
            Message::AnalyzeOnlyPressed => {
                self.status_text = "Analysis started (not really!)...".to_string();
                self.progress = 0.3;
            }
            Message::NoOp => {}
        }
        Command::none()
    }

    // This function draws the UI based on the current state
    fn view(&self) -> Element<Message> {
        let main_content = main_window::view(
            &self.log_output,
            self.status_text.as_str(),
                                             self.progress,
                                             &self.quick_analysis_ref,
                                             &self.quick_analysis_sec,
                                             &self.quick_analysis_ter,
                                             self.archive_logs,
        );

        let base = Modal::new(main_content)
        .on_blur(Some(Message::NoOp));

        let job_queue_overlay = if self.show_job_queue {
            Some(job_queue_dialog::view(Message::CloseJobQueue))
        } else {
            None
        };

        let options_overlay = if self.show_options {
            Some(options_dialog::view(Message::CloseOptions))
        } else {
            None
        };

        let with_job_queue = base.overlay(job_queue_overlay);
        let with_options = Modal::new(with_job_queue).overlay(options_overlay);


        container(with_options)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x()
        .center_y()
        .into()
    }
}
