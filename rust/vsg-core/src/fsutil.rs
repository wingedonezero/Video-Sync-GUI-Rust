use std::env;
use std::path::{PathBuf};
use time::OffsetDateTime;

pub fn binary_dir() -> PathBuf {
    env::current_exe().ok().and_then(|p| p.parent().map(|q| q.to_path_buf())).unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

pub fn default_work_dir() -> PathBuf {
    let ts = OffsetDateTime::now_utc().unix_timestamp();
    let mut p = binary_dir();
    p.push("_work");
    p.push(format!("job_{}", ts));
    p
}

pub fn default_output_dir() -> PathBuf {
    let mut p = binary_dir();
    p.push("_out");
    p
}
