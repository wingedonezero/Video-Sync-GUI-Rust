
use anyhow::Result;
use std::path::Path;

pub fn shift_chapters_xml(_in_xml: &Path, _out_xml: &Path, _shift_ms: i64) -> Result<()> {
    anyhow::bail!("chapters shift not implemented yet")
}
