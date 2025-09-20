use std::collections::HashMap;
use std::path::PathBuf;

/// Pipeline context that gets passed through all steps
#[derive(Debug, Clone)]
pub struct Context {
    pub settings: crate::config::AppConfig,
    pub sources: HashMap<String, PathBuf>,
    pub and_merge: bool,
    pub temp_dir: PathBuf,
    pub output_dir: PathBuf,

    // Filled by steps
    pub delays: Option<Delays>,
}

#[derive(Debug, Clone)]
pub struct Delays {
    pub source_delays_ms: HashMap<String, i32>,
    pub global_shift_ms: i32,
}
