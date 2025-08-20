
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub struct Logger {
    buf: String,
    file: Option<File>,
}

impl Logger {
    pub fn new() -> Self {
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
        let dir = exe.parent().unwrap_or(&PathBuf::from(".")).to_path_buf();
        let stamp = OffsetDateTime::now_utc().format(&Rfc3339).unwrap_or_else(|_| "now".into()).replace(':','-');
        let log_path = dir.join(format!("vsg_gui_{}.log", stamp));
        let file = OpenOptions::new().create(true).write(true).append(true).open(log_path).ok();
        Logger { buf: String::new(), file }
    }

    pub fn log(&mut self, s: &str) {
        let now = OffsetDateTime::now_utc().format(&Rfc3339).unwrap_or_else(|_| "now".into());
        let line = format!("[{}] {}\n", now, s);
        self.buf.push_str(&line);
        if let Some(f) = self.file.as_mut() {
            let _ = f.write_all(line.as_bytes());
        }
    }

    pub fn contents(&self) -> &str { &self.buf }
}
