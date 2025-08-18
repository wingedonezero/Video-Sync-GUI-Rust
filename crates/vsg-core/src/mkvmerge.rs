//! mkvmerge command builder

use std::path::PathBuf;

pub struct MkvMergeOpts {
    pub reference: PathBuf,
    pub secondary: Option<PathBuf>,
    pub tertiary: Option<PathBuf>,
    pub out_opts: PathBuf,
    pub output: PathBuf,
    pub sec_delay: Option<i64>,
    pub ter_delay: Option<i64>,
    pub mkvmerge: Option<PathBuf>,
}

impl MkvMergeOpts {
    pub fn validate(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.output.parent() {
            if !parent.exists() {
                anyhow::bail!("Output directory does not exist: {:?}", parent);
            }
        }
        Ok(())
    }
}
