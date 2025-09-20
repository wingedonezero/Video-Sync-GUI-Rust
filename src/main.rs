mod config;
mod ui;
mod core;

fn main() -> iced::Result {
    // Initialize the main window app
    ui::main_window::MainWindow::run()
}
