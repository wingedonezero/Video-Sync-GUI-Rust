use crate::error::VsgError;
use crate::model::{SelectionManifest, Source};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn ensure_dir(p:&PathBuf) -> Result<(),VsgError> { fs::create_dir_all(p).map_err(|e| VsgError::Io(e)) }

fn mkvextract_tracks_cmd(input:&str, mappings:&[(u32, String)]) -> Vec<String> {
    // mkvextract tracks input.mkv 0:out0 1:out1 ...
    let mut argv = vec!["tracks".to_string(), input.to_string()];
    for (id,out) in mappings {
        argv.push(format!("{}:{}", id, out));
    }
    argv
}

pub struct ExtractSummary {
    pub files: Vec<String>,
}

pub fn run_mkvextract(selection:&SelectionManifest, work_root:&PathBuf) -> Result<ExtractSummary, VsgError> {
    let mut out_files = Vec::new();

    // Prepare per-source directories
    let mut ref_dir = work_root.clone(); ref_dir.push("ref"); ensure_dir(&ref_dir)?;
    let mut sec_dir = work_root.clone(); sec_dir.push("sec"); ensure_dir(&sec_dir)?;
    let mut ter_dir = work_root.clone(); ter_dir.push("ter"); ensure_dir(&ter_dir)?;

    // Helper to run a single mkvextract tracks call
    let mut run_for = |source:Source, file_path:&str, entries:&[(u32, String)]| -> Result<(),VsgError> {
        if entries.is_empty() { return Ok(()); }
        let mut cmd = Command::new("mkvextract");
        cmd.arg("tracks").arg(file_path);
        for (id,out) in entries {
            cmd.arg(format!("{}:{}", id, out));
        }
        let out = cmd.output().map_err(|e| VsgError::Process(format!("spawn mkvextract: {}", e)))?;
        if !out.status.success() {
            return Err(VsgError::Process(format!("mkvextract failed ({}): {}", out.status, String::from_utf8_lossy(&out.stderr))));
        }
        Ok(())
    };

    // REF
    if !selection.ref_tracks.is_empty() {
        let input = &selection.ref_tracks[0].file_path;
        let mut maps:Vec<(u32,String)> = Vec::new();
        for (i, t) in selection.ref_tracks.iter().enumerate() {
            let subdir = match t.r#type.as_str() { "video"=>"v", "audio"=>"a", "subtitles"=>"s", _=>"o" };
            let mut p = ref_dir.clone(); p.push(subdir);
            ensure_dir(&p)?;
            let fname = format!("{:03}_{}_{}.bin", i, t.r#type, t.language.clone().unwrap_or_else(||"und".into()));
            p.push(fname);
            maps.push((t.track_id, p.to_string_lossy().to_string()));
            out_files.push(p.to_string_lossy().to_string());
        }
        run_for(Source::REF, input, &maps)?;
    }

    // SEC
    if !selection.sec_tracks.is_empty() {
        let input = &selection.sec_tracks[0].file_path;
        let mut maps:Vec<(u32,String)> = Vec::new();
        for (i, t) in selection.sec_tracks.iter().enumerate() {
            let mut p = sec_dir.clone(); p.push("a");
            ensure_dir(&p)?;
            let fname = format!("{:03}_{}_{}.bin", i, t.r#type, t.language.clone().unwrap_or_else(||"und".into()));
            p.push(fname);
            maps.push((t.track_id, p.to_string_lossy().to_string()));
            out_files.push(p.to_string_lossy().to_string());
        }
        run_for(Source::SEC, input, &maps)?;
    }

    // TER
    if !selection.ter_tracks.is_empty() {
        let input = &selection.ter_tracks[0].file_path;
        let mut maps:Vec<(u32,String)> = Vec::new();
        for (i, t) in selection.ter_tracks.iter().enumerate() {
            let mut p = ter_dir.clone(); let subdir = if t.r#type=="subtitles"{"s"} else {"o"}; p.push(subdir);
            ensure_dir(&p)?;
            let fname = format!("{:03}_{}_{}.bin", i, t.r#type, t.language.clone().unwrap_or_else(||"und".into()));
            p.push(fname);
            maps.push((t.track_id, p.to_string_lossy().to_string()));
            out_files.push(p.to_string_lossy().to_string());
        }
        run_for(Source::TER, input, &maps)?;
    }

    Ok(ExtractSummary{files: out_files})
}
