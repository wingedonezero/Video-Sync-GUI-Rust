
use anyhow::Result;
use crate::types::*;

pub fn probe_tracks(_src: &Sources, _tools: &ToolPaths) -> Result<Vec<TrackMeta>> {
    Err(anyhow::anyhow!("probe not implemented yet"))
}
