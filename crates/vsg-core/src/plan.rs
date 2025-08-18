
use anyhow::{Result, Context};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use crate::types::*;

pub fn positive_only(raw: &RawDelays) -> PositiveDelays {
    let min_raw = [Some(0), raw.sec_ms, raw.ter_ms].into_iter().flatten().min().unwrap_or(0);
    let global = if min_raw < 0 { -min_raw } else { 0 };
    let sec_res = raw.sec_ms.unwrap_or(0) + global;
    let ter_res = raw.ter_ms.unwrap_or(0) + global;
    PositiveDelays { global_ms: global, sec_residual_ms: sec_res, ter_residual_ms: ter_res }
}

pub fn plan_merge(manifest: &Path, output_file: &Path, prefer_lang: &str, signs_pattern: &str, first_sub_default: bool, default_signs: bool, delays: &PositiveDelays) -> Result<MergePlan> {
    let bytes = fs::read(manifest).with_context(|| format!("read manifest {}", manifest.display()))?;
    let plan: ExtractPlan = serde_json::from_slice(&bytes).with_context(|| "parse manifest ExtractPlan")?;

    let signs_re = Regex::new(signs_pattern).unwrap();
    let prefer_lang_lc = prefer_lang.to_lowercase();

    let mut final_tracks: Vec<PlannedTrack> = Vec::new();

    // 1) REF Video (single track file)
    if let Some(ref_video) = plan.ref_video {
        let mut opts = vec!["--compression".into(), "0:none".into(), "--sync".into(), format!("0:{}", delays.global_ms)];
        if let Some(lang) = ref_video.meta.lang.clone() {
            opts.push("--language".into()); opts.push(format!("0:{}", lang));
        }
        if let Some(name) = ref_video.meta.name.clone() {
            opts.push("--track-name".into()); opts.push(format!("0:{}", name));
        }
        final_tracks.push(PlannedTrack { meta: ref_video.meta, file: ref_video.out_path, mkvmerge_track_opts: opts });
    }

    // Gather SEC audio (lang match first), then SEC subs, then TER subs
    // 2) SEC audio (preferred language first, preserve src order)
    let mut sec_audio: Vec<ExtractItem> = plan.sec_tracks.iter().cloned().filter(|x| matches!(x.meta.kind, TrackKind::Audio))
        .filter(|x| x.meta.lang.as_deref().unwrap_or("").to_lowercase() == prefer_lang_lc).collect();
    sec_audio.sort_by_key(|x| x.meta.order_in_src);

    for (i, item) in sec_audio.into_iter().enumerate() {
        let mut opts = vec!["--compression".into(), "0:none".into(), "--sync".into(), format!("0:{}", delays.sec_residual_ms)];
        if let Some(lang) = item.meta.lang.clone() {
            opts.push("--language".into()); opts.push(format!("0:{}", lang));
        }
        if let Some(name) = item.meta.name.clone() {
            opts.push("--track-name".into()); opts.push(format!("0:{}", name));
        }
        // default flag for first audio only
        let def = if i == 0 { "yes" } else { "no" };
        opts.push("--default-track-flag".into()); opts.push(format!("0:{}", def));
        final_tracks.push(PlannedTrack { meta: item.meta, file: item.out_path, mkvmerge_track_opts: opts });
    }

    // 3) SEC subs
    let mut sec_subs: Vec<ExtractItem> = plan.sec_tracks.iter().cloned().filter(|x| matches!(x.meta.kind, TrackKind::Subtitle)).collect();
    sec_subs.sort_by_key(|x| x.meta.order_in_src);
    for item in sec_subs {
        let mut opts = vec!["--compression".into(), "0:none".into(), "--sync".into(), format!("0:{}", delays.sec_residual_ms)];
        if let Some(lang) = item.meta.lang.clone() {
            opts.push("--language".into()); opts.push(format!("0:{}", lang));
        }
        if let Some(name) = item.meta.name.clone() {
            opts.push("--track-name".into()); opts.push(format!("0:{}", name));
        }
        final_tracks.push(PlannedTrack { meta: item.meta, file: item.out_path, mkvmerge_track_opts: opts });
    }

    // 4) TER subs
    let mut ter_subs: Vec<ExtractItem> = plan.ter_subs.iter().cloned().collect();
    ter_subs.sort_by_key(|x| x.meta.order_in_src);
    for item in ter_subs {
        let mut opts = vec!["--compression".into(), "0:none".into(), "--sync".into(), format!("0:{}", delays.ter_residual_ms)];
        if let Some(lang) = item.meta.lang.clone() {
            opts.push("--language".into()); opts.push(format!("0:{}", lang));
        }
        if let Some(name) = item.meta.name.clone() {
            opts.push("--track-name".into()); opts.push(format!("0:{}", name));
        }
        final_tracks.push(PlannedTrack { meta: item.meta, file: item.out_path, mkvmerge_track_opts: opts });
    }

    // Subtitle default rule
    if first_sub_default || default_signs {
        let mut picked = false;
        for pt in final_tracks.iter_mut().filter(|t| matches!(t.meta.kind, TrackKind::Subtitle)) {
            if picked { break; }
            let mut make_default = false;
            if default_signs {
                let name = pt.meta.name.as_deref().unwrap_or("").to_lowercase();
                if signs_re.is_match(&name) { make_default = true; }
            }
            if !make_default && first_sub_default {
                make_default = true;
            }
            if make_default {
                pt.mkvmerge_track_opts.push("--default-track-flag".into());
                pt.mkvmerge_track_opts.push("0:yes".into());
                picked = true;
            }
        }
    }

    // Attachments
    let attachments: Vec<PathBuf> = plan.ter_attachments.into_iter().map(|(_, p)| p).collect();

    Ok(MergePlan {
        final_order: final_tracks,
        chapters: plan.chapters_xml,
        delays: delays.clone(),
        attachments,
        output_file: output_file.to_path_buf(),
    })
}
