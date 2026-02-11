//! Video Sync GUI - Main Entry Point
//!
//! GTK4/Relm4 based application for video/audio synchronization and merging.

mod add_job_dialog;
mod app;
mod job_queue;
mod manual_selection;
mod settings;

use relm4::RelmApp;

fn main() {
    let app = RelmApp::new("com.videosyncgui.app");
    app.run::<app::App>(());
}
