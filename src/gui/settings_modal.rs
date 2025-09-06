use iced::widget::{button, column, container, row, text};
use iced::Element;
use iced_aw::{Card, TabLabel, Tabs};

use crate::gui::app::Msg;
use vsg::core::config::AppConfig;

pub fn modal() -> Element<'static, Msg> {
    Card::new(text("Application Settings"), tabs())
    .foot(row![button("Save").on_press(Msg::CloseSettings(true)), button("Cancel").on_press(Msg::CloseSettings(false))].spacing(10))
    .max_width(900.0)
    .into()
}

fn tabs() -> Element<'static, Msg> {
    let tabs = Tabs::new(0, Msg::SettingsChanged, vec![
        (TabLabel::Text("Storage".into()), storage_tab()),
                         (TabLabel::Text("Analysis".into()), analysis_tab()),
                         (TabLabel::Text("Chapters".into()), chapters_tab()),
                         (TabLabel::Text("Merge Behavior".into()), merge_tab()),
                         (TabLabel::Text("Logging".into()), logging_tab()),
    ]);
    container(tabs).padding(10).into()
}

fn storage_tab() -> Element<'static, Msg> {
    let c = AppConfig::new("settings.json");
    container(column![
        row![text("Output Directory:"), text(c.get_string("output_folder"))],
              row![text("Temporary Directory:"), text(c.get_string("temp_root"))],
              row![text("VideoDiff Path (optional):"), text(c.get_string("videodiff_path"))],
              text("Settings are persisted via the main window; this tab mirrors values so you can confirm."),
    ].spacing(8)).into()
}
fn analysis_tab() -> Element<'static, Msg> {
    let c=AppConfig::new("settings.json");
    container(column![
        row![text("Analysis Mode:"), text(c.get_string("analysis_mode"))],
              row![text("Audio: Scan Chunks:"), text(c.get_i64("scan_chunk_count").to_string())],
              row![text("Audio: Chunk Duration (s):"), text(c.get_i64("scan_chunk_duration").to_string())],
              row![text("Audio: Minimum Match %:"), text(format!("{:.1}", c.get_f64("min_match_pct")))],
              row![text("VideoDiff: Min Allowed Error:"), text(format!("{:.2}", c.get_f64("videodiff_error_min")))],
              row![text("VideoDiff: Max Allowed Error:"), text(format!("{:.2}", c.get_f64("videodiff_error_max")))],
              text("Analysis Audio Track Selection"),
              row![text("REF Language:"), text(c.get_string("analysis_lang_ref"))],
              row![text("SEC Language:"), text(c.get_string("analysis_lang_sec"))],
              row![text("TER Language:"), text(c.get_string("analysis_lang_ter"))],
    ].spacing(8)).into()
}
fn chapters_tab() -> Element<'static, Msg> {
    let c=AppConfig::new("settings.json");
    container(column![
        row![text("Rename to \"Chapter NN\":"), text(c.get_bool("rename_chapters").to_string())],
              row![text("Snap to keyframes:"), text(c.get_bool("snap_chapters").to_string())],
              row![text("Snap Mode:"), text(c.get_string("snap_mode"))],
              row![text("Snap Threshold (ms):"), text(c.get_i64("snap_threshold_ms").to_string())],
              row![text("Only snap starts:"), text(c.get_bool("snap_starts_only").to_string())],
    ].spacing(8)).into()
}
fn merge_tab() -> Element<'static, Msg> {
    let c=AppConfig::new("settings.json");
    container(column![
        row![text("Remove dialog normalization gain (AC3/E-AC3):"), text(c.get_bool("apply_dialog_norm_gain").to_string())],
              row![text("Disable track statistics tags:"), text(c.get_bool("disable_track_statistics_tags").to_string())],
    ].spacing(8)).into()
}
fn logging_tab() -> Element<'static, Msg> {
    let c=AppConfig::new("settings.json");
    container(column![
        row![text("Use compact logging:"), text(c.get_bool("log_compact").to_string())],
              row![text("Auto-scroll log view:"), text(c.get_bool("log_autoscroll").to_string())],
              row![text("Progress Step:"), text(format!("{}%", c.get_i64("log_progress_step")))],
              row![text("Error Tail:"), text(format!("{} lines", c.get_i64("log_error_tail")))],
              row![text("Show mkvmerge options (pretty):"), text(c.get_bool("log_show_options_pretty").to_string())],
              row![text("Show mkvmerge options (raw JSON):"), text(c.get_bool("log_show_options_json").to_string())],
    ].spacing(8)).into()
}
