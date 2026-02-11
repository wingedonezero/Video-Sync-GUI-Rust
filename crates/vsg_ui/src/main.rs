//! Video Sync GUI - Main Entry Point
//!
//! GTK4/Relm4 based application for video/audio synchronization and merging.

mod app;
mod settings;

use relm4::RelmApp;

fn main() {
    let app = RelmApp::new("com.videosyncgui.app");
    app.run::<app::App>(());
}
