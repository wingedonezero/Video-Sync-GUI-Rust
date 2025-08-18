
use anyhow::Result;
use serde::Serialize;
use crate::types::MergePlan;

#[derive(Serialize)]
struct Opts(Vec<String>);

pub fn build_opts_tokens(plan: &MergePlan) -> Vec<String> {
    let mut t: Vec<String> = Vec::new();
    // Output first
    t.push("--output".into()); t.push(plan.output_file.to_string_lossy().to_string());
    // Chapters if any
    if let Some(ch) = &plan.chapters {
        t.push("--chapters".into()); t.push(ch.to_string_lossy().to_string());
    }
    // Files/tracks
    for track in &plan.final_order {
        t.push("(".into());
        t.push(track.file.to_string_lossy().to_string());
        t.push(")".into());
        t.extend(track.mkvmerge_track_opts.clone());
    }
    // Attachments
    for att in &plan.attachments {
        t.push("--attach-file".into()); t.push(att.to_string_lossy().to_string());
    }
    t
}

pub fn write_opts_json(plan: &MergePlan, path: &std::path::Path) -> Result<()> {
    let tokens = build_opts_tokens(plan);
    let json = serde_json::to_string_pretty(&tokens)?;
    std::fs::write(path, json)?;
    Ok(())
}
