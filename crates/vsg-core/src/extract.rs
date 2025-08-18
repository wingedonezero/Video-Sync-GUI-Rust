
use anyhow::Result;
use crate::types::*;

pub fn build_extract_plan(_tracks: &[TrackMeta], _rules: &OrderRules) -> Result<ExtractPlan> {
    Err(anyhow::anyhow!("extract plan not implemented yet"))
}

pub fn execute_extracts(_plan: &ExtractPlan, _tools: &ToolPaths, _temp: &TempLayout) -> Result<()> {
    Err(anyhow::anyhow!("extract execution not implemented yet"))
}
