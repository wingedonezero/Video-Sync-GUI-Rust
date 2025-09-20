use iced::widget::{button, column, container, row, text, Space};
use iced::{Element, Length};

pub struct JobQueueDialog {
    jobs: Vec<Job>,
    selected_job: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Job {
    pub status: String,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum Message {
    AddJobs,
    RemoveSelected,
    ConfigureJob(usize),
    MoveUp,
    MoveDown,
    StartProcessing,
    Cancel,
}

impl JobQueueDialog {
    pub fn new() -> Self {
        Self {
            jobs: Vec::new(),
            selected_job: None,
        }
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::AddJobs => {
                // TODO: Open add jobs dialog
            }
            Message::RemoveSelected => {
                if let Some(idx) = self.selected_job {
                    self.jobs.remove(idx);
                    self.selected_job = None;
                }
            }
            Message::ConfigureJob(idx) => {
                // TODO: Open manual selection dialog for job
                self.selected_job = Some(idx);
            }
            Message::MoveUp => {
                if let Some(idx) = self.selected_job {
                    if idx > 0 {
                        self.jobs.swap(idx, idx - 1);
                        self.selected_job = Some(idx - 1);
                    }
                }
            }
            Message::MoveDown => {
                if let Some(idx) = self.selected_job {
                    if idx < self.jobs.len() - 1 {
                        self.jobs.swap(idx, idx + 1);
                        self.selected_job = Some(idx + 1);
                    }
                }
            }
            Message::StartProcessing => {
                // TODO: Start processing jobs
            }
            Message::Cancel => {
                // TODO: Close dialog
            }
        }
    }

    pub fn view(&self) -> Element<Message> {
        // TODO: Implement table widget for jobs
        let job_list = container(
            text("Job list will go here")
        )
        .width(Length::Fill)
        .height(Length::FillPortion(5));

        let button_row = row![
            button("Add Job(s)...").on_press(Message::AddJobs),
            Space::with_width(Length::Fill),
            button("Move Up").on_press(Message::MoveUp),
            button("Move Down").on_press(Message::MoveDown),
            button("Remove Selected").on_press(Message::RemoveSelected),
        ]
        .spacing(10)
        .padding(10);

        let dialog_buttons = row![
            Space::with_width(Length::Fill),
            button("Start Processing Queue").on_press(Message::StartProcessing),
            button("Cancel").on_press(Message::Cancel),
        ]
        .spacing(10)
        .padding(10);

        column![
            job_list,
            button_row,
            dialog_buttons,
        ]
        .into()
    }
}
