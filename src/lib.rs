// src/lib.rs
pub mod core {
    pub mod command_runner;
    pub mod config;
    pub mod mkv_utils;
    pub mod subtitle_utils;
    pub mod analysis;
    pub mod job_discovery;
    pub mod pipeline;          // <-- new
}
pub mod types {
    pub mod errors;
    pub mod tracks;
}
