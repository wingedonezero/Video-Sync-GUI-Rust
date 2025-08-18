
use tracing::{info, warn, error};

pub fn step<S: AsRef<str>>(s: S) { info!("{}", s.as_ref()); }
pub fn warn_line<S: AsRef<str>>(s: S) { warn!("{}", s.as_ref()); }
pub fn err_line<S: AsRef<str>>(s: S) { error!("{}", s.as_ref()); }
