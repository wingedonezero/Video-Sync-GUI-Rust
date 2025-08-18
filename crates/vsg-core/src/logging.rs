use tracing::{info};

pub fn log_cmd(cmdline: &str) {
    info!("$ {}", cmdline);
}

pub fn log_progress(pct: u8) {
    info!("Progress: {}%", pct);
}

pub fn log_section(title: &str) {
    info!("=== {} ===", title);
}
