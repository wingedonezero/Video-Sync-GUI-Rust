
use anyhow::Result;
use std::path::{Path, PathBuf};
use crate::types::*;

pub fn extract_plan_and_execute(_src: &Sources, _tools: &ToolPaths, temp: &TempLayout, _prefer_lang: &str, _signs_pattern: &str) -> Result<ExtractPlan> {
    // For PR2 we only need to read manifest; extraction was implemented in PR1 and already committed in repo.
    // This stub just points to existing manifest location so plan step can proceed if caller provides it.
    let manifest_path = temp.root.join("manifest.json");
    let plan = ExtractPlan {
        ref_video: None,
        sec_tracks: vec![],
        ter_subs: vec![],
        ter_attachments: vec![],
        chapters_xml: None,
        manifest_path: manifest_path.clone(),
    };
    Ok(plan)
}
