fn main() -> iced::Result {
    env_logger::init();
    vsg::gui::app::App::run(iced::Settings {
        antialiasing: true,
        ..Default::default()
    })
}
