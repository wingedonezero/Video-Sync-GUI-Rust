use crate::error::VsgError;
use crate::model::SelectionManifest;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn ensure_dir(p:&PathBuf) -> Result<(),VsgError> { fs::create_dir_all(p).map_err(|e| VsgError::Io(e)) }

fn ext_for(track_type:&str, codec_opt:Option<&str>) -> &'static str {
    let c = codec_opt.unwrap_or("");
    let cl = c.to_lowercase();
    // Accept both normalized names and mkvmerge codec_id strings.
    match track_type {
        "audio" => {
            match cl.as_str() {
                "aac" | "a_aac" | "mp4a" | "a_aac_mpeg2lc" | "a_aac_mpeg4lc" => "aac",
                "ac3" | "a_ac3" => "ac3",
                "eac3" | "e-ac-3" | "a_eac3" | "a_e-ac-3" => "eac3",
                "dts" | "a_dts" | "a_dts_hd" | "a_dts-x" => "dts",
                "truehd" | "a_truehd" => "thd",
                "flac" | "a_flac" => "flac",
                "opus" | "a_opus" => "opus",
                "vorbis" | "a_vorbis" => "ogg",
                "pcm" | "lpcm" | "a_pcm" | "a_ms/acm" => "wav",
                // Uppercase mkvmerge IDs
                "a_aac" | "a_ac3" | "a_eac3" | "a_dts" | "a_truehd" | "a_flac" | "a_opus" | "a_vorbis" => {
                    // already matched by lowercase, keep for clarity
                    "audio"
                }
                _ => {
                    // Check raw mkvmerge codec_id forms
                    if c.starts_with("A_") {
                        if c.contains("AAC") { "aac" }
                        else if c.contains("AC3") && !c.contains("EAC3") { "ac3" }
                        else if c.contains("EAC3") { "eac3" }
                        else if c.contains("DTS") { "dts" }
                        else if c.contains("TRUEHD") { "thd" }
                        else if c.contains("FLAC") { "flac" }
                        else if c.contains("OPUS") { "opus" }
                        else if c.contains("VORBIS") { "ogg" }
                        else if c.contains("PCM") { "wav" }
                        else { "audio" }
                    } else { "audio" }
                }
            }
        },
        "subtitles" | "subtitle" => {
            match cl.as_str() {
                "ass" | "s_text/ass" | "s_ass" => "ass",
                "srt" | "subrip" | "s_text/utf8" | "s_text/utf-8" => "srt",
                "pgs" | "hdmv_pgs_subtitle" | "s_hdmv/pgs" => "sup",
                _ => {
                    if c.starts_with("S_") {
                        if c.contains("ASS") { "ass" }
                        else if c.contains("UTF8") || c.contains("SUBRIP") { "srt" }
                        else if c.contains("PGS") { "sup" }
                        else { "sub" }
                    } else { "sub" }
                }
            }
        },
        "video" => {
            match cl.as_str() {
                "h264" | "avc" | "v_mpeg4/iso/avc" => "h264",
                "h265" | "hevc" | "v_mpegh/iso/hevc" => "hevc",
                "vc1" | "v_ms/vfw/fourcc" => "vc1",
                "mpeg2" | "mpeg-2" | "v_mpeg2" => "m2v",
                _ => {
                    if c.starts_with("V_") {
                        if c.contains("AVC") { "h264" }
                        else if c.contains("HEVC") { "hevc" }
                        else if c.contains("VC1") { "vc1" }
                        else if c.contains("MPEG2") { "m2v" }
                        else { "video" }
                    } else { "video" }
                }
            }
        },
        _ => "bin"
    }
}

pub struct ExtractSummary { pub files: Vec<String> }

pub fn run_mkvextract(selection:&SelectionManifest, work_root:&PathBuf) -> Result<ExtractSummary, VsgError> {
    let mut out_files = Vec::new();

    // Prepare per-source directories (flat under ref|sec|ter with no type subfolders)
    let mut ref_dir = work_root.clone(); ref_dir.push("ref"); ensure_dir(&ref_dir)?;
    let mut sec_dir = work_root.clone(); sec_dir.push("sec"); ensure_dir(&sec_dir)?;
    let mut ter_dir = work_root.clone(); ter_dir.push("ter"); ensure_dir(&ter_dir)?;

    let run_for = |file_path:&str, entries:&[(u32, String)]| -> Result<(),VsgError> {
        if entries.is_empty() { return Ok(()); }
        let mut cmd = Command::new("mkvextract");
        cmd.arg("tracks").arg(file_path);
        for (id,out) in entries { cmd.arg(format!("{}:{}", id, out)); }
        let out = cmd.output().map_err(|e| VsgError::Process(format!("spawn mkvextract: {}", e)))?;
        if !out.status.success() {
            return Err(VsgError::Process(format!("mkvextract failed ({}): {}", out.status, String::from_utf8_lossy(&out.stderr))));
        }
        Ok(())
    };

    let mkname = |idx:usize, track_type:&str, lang_opt:&Option<String>, codec_opt:&Option<String>| -> String {
        let lang = lang_opt.clone().unwrap_or_else(|| "und".into());
        let ext = ext_for(track_type, codec_opt.as_deref());
        format!("{:03}_{}.{}.{}", idx, track_type, lang, ext)
    };

    // REF
    if !selection.ref_tracks.is_empty() {
        let input = &selection.ref_tracks[0].file_path;
        let mut maps:Vec<(u32,String)> = Vec::new();
        for (i, t) in selection.ref_tracks.iter().enumerate() {
            let mut p = ref_dir.clone();
            let fname = mkname(i, &t.r#type, &t.language, &t.codec);
            p.push(fname);
            maps.push((t.track_id, p.to_string_lossy().to_string()));
            out_files.push(p.to_string_lossy().to_string());
        }
        run_for(input, &maps)?;
    }

    // SEC
    if !selection.sec_tracks.is_empty() {
        let input = &selection.sec_tracks[0].file_path;
        let mut maps:Vec<(u32,String)> = Vec::new();
        for (i, t) in selection.sec_tracks.iter().enumerate() {
            let mut p = sec_dir.clone();
            let fname = mkname(i, &t.r#type, &t.language, &t.codec);
            p.push(fname);
            maps.push((t.track_id, p.to_string_lossy().to_string()));
            out_files.push(p.to_string_lossy().to_string());
        }
        run_for(input, &maps)?;
    }

    // TER
    if !selection.ter_tracks.is_empty() {
        let input = &selection.ter_tracks[0].file_path;
        let mut maps:Vec<(u32,String)> = Vec::new();
        for (i, t) in selection.ter_tracks.iter().enumerate() {
            let mut p = ter_dir.clone();
            let fname = mkname(i, &t.r#type, &t.language, &t.codec);
            p.push(fname);
            maps.push((t.track_id, p.to_string_lossy().to_string()));
            out_files.push(p.to_string_lossy().to_string());
        }
        run_for(input, &maps)?;
    }

    Ok(ExtractSummary{files: out_files})
}
