use cxx_qt_build::CxxQtBuilder;

fn main() {
    unsafe {
        CxxQtBuilder::new()
            .qt_module("Widgets")
            .files(["src/bridge/app_controller.rs"])
            .cc_builder(|cc| {
                cc.include("src/cpp")
                    .file("src/cpp/main_window.cpp");
            })
            .build();
    }
}
