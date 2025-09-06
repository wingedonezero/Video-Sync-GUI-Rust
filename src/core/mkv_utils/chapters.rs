// src/core/mkv_utils/chapters.rs
//
// Full Rust port of Python chapter handling:
// - mkvextract chapters -> XML string
// - optional rename to "Chapter NN"
// - apply shift_ms to start/end
// - optional snap to keyframes (via ffprobe packets JSON)
// - normalize end times (<= next start, >= start+1ns)
// - write final XML with declaration

use crate::core::command_runner::CommandRunner;
use regex::Regex;
use serde_json::{Map as JsonMap, Value};
use std::cmp::{max, min};
use std::path::{Path, PathBuf};
use xmltree::{Element, XMLNode};
use xml::writer::EmitterConfig;

// ---------- Public entry ----------

pub fn process_chapters(
    ref_mkv: &str,
    temp_dir: &Path,
    runner: &CommandRunner,
    config: &JsonMap<String, Value>,
    shift_ms: i64,
) -> Option<PathBuf> {
    // Extract chapters to string (stdout). Parity with Python: use "-" sink.
    let xml_content = runner.run(&["mkvextract", ref_mkv, "chapters", "-"])?;
    let xml_trimmed = strip_bom(xml_content.trim());

    if xml_trimmed.is_empty() {
        runner.log("No chapters found in reference file.");
        return None;
    }

    // Parse XML into a DOM we can edit
    let mut root: Element = match Element::parse(xml_trimmed.as_bytes()) {
        Ok(el) => el,
        Err(e) => {
            runner.log(&format!("[ERROR] Chapter processing failed: {}", e));
            return None;
        }
    };

    // Settings
    let rename_chapters = config.get("rename_chapters").and_then(|v| v.as_bool()).unwrap_or(false);
    let snap_chapters = config.get("snap_chapters").and_then(|v| v.as_bool()).unwrap_or(false);
    let snap_mode = config.get("snap_mode").and_then(|v| v.as_str()).unwrap_or("previous").to_string();
    let snap_threshold_ms = config.get("snap_threshold_ms").and_then(|v| v.as_i64()).unwrap_or(250);
    let snap_starts_only = config.get("snap_starts_only").and_then(|v| v.as_bool()).unwrap_or(true);

    // Rename
    if rename_chapters {
        rename_to_chapter_nn(&mut root, runner);
    }

    // Shift
    let shift_ns = shift_ms * 1_000_000;
    if shift_ns != 0 {
        shift_all(&mut root, shift_ns);
        runner.log(&format!("[Chapters] Shifted all timestamps by +{} ms.", shift_ms));
    }

    // Optional snap
    if snap_chapters {
        let kfs = probe_keyframes_ns(ref_mkv, runner);
        if kfs.is_empty() {
            runner.log("[Chapters] Snap skipped: could not load keyframes.");
        } else {
            snap_times_inplace(&mut root, &kfs, &snap_mode, snap_threshold_ms, snap_starts_only, runner);
        }
    }

    // Normalize end times
    normalize_end_times(&mut root, runner);

    // Write out file: "<stem>_chapters_modified.xml"
    let stem = Path::new(ref_mkv)
    .file_stem()
    .unwrap_or_default()
    .to_string_lossy()
    .to_string();
    let out_path = temp_dir.join(format!("{}_chapters_modified.xml", stem));

    let mut buf: Vec<u8> = Vec::new();
    let mut cfg = EmitterConfig::new();
    cfg.perform_indent = false;
    cfg.write_document_declaration = true;
    if let Err(e) = root.write_with_config(&mut buf, cfg) {
        runner.log(&format!("[ERROR] Chapter processing failed: {}", e));
        return None;
    }
    if std::fs::write(&out_path, &buf).is_ok() {
        runner.log(&format!("Chapters XML written to: {}", out_path.display()));
        Some(out_path)
    } else {
        runner.log("[ERROR] Could not write chapters file.");
        None
    }
}

// ---------- Rename ----------

fn rename_to_chapter_nn(root: &mut Element, runner: &CommandRunner) {
    // Find all ChapterAtom nodes and replace their ChapterDisplay with a new one.
    let atoms = collect_chapter_atoms_mut(root);
    for (i, atom) in atoms.into_iter().enumerate() {
        // Remove all ChapterDisplay children
        atom.children.retain(|child| {
            if let XMLNode::Element(el) = child {
                el.name != "ChapterDisplay"
            } else {
                true
            }
        });
        // New ChapterDisplay
        let mut disp = Element::new("ChapterDisplay");
        let mut s = Element::new("ChapterString");
        s.children.push(XMLNode::Text(format!("Chapter {:02}", i + 1)));
        let mut lang = Element::new("ChapterLanguage");
        lang.children.push(XMLNode::Text("und".into()));
        disp.children.push(XMLNode::Element(s));
        disp.children.push(XMLNode::Element(lang));
        atom.children.push(XMLNode::Element(disp));
    }
    runner.log("[Chapters] Renamed chapters to \"Chapter NN\".");
}

// ---------- Shifting ----------

fn shift_all(root: &mut Element, shift_ns: i64) {
    for atom in collect_chapter_atoms_mut(root) {
        for tag in ["ChapterTimeStart", "ChapterTimeEnd"] {
            if let Some(mut el) = child_mut(atom, tag) {
                if let Some(ts) = element_text(&el) {
                    let ns = parse_ns(&ts).unwrap_or(0);
                    let ns2 = ns + shift_ns;
                    set_element_text(&mut el, &fmt_ns(ns2));
                }
            }
        }
    }
}

// ---------- Snapping ----------

fn probe_keyframes_ns(ref_video_path: &str, runner: &CommandRunner) -> Vec<i64> {
    let args = [
        "ffprobe",
        "-v",
        "error",
        "-select_streams",
        "v:0",
        "-show_entries",
        "packet=pts_time,flags",
        "-of",
        "json",
        ref_video_path,
    ];
    let out = match runner.run(&args) {
        Some(s) => s,
        None => {
            runner.log("[WARN] ffprobe for keyframes produced no output.");
            return vec![];
        }
    };
    match serde_json::from_str::<Value>(&out) {
        Ok(v) => {
            let mut kfs = vec![];
            if let Some(pkts) = v.get("packets").and_then(|x| x.as_array()) {
                for p in pkts {
                    let is_k = p
                    .get("flags")
                    .and_then(|x| x.as_str())
                    .map(|f| f.contains('K'))
                    .unwrap_or(false);
                    if !is_k {
                        continue;
                    }
                    if let Some(pts_s) = p.get("pts_time").and_then(|x| x.as_str()) {
                        if let Ok(f) = pts_s.parse::<f64>() {
                            kfs.push((f * 1_000_000_000.0).round() as i64);
                        }
                    } else if let Some(pts_f) = p.get("pts_time").and_then(|x| x.as_f64()) {
                        kfs.push((pts_f * 1_000_000_000.0).round() as i64);
                    }
                }
            }
            kfs.sort();
            runner.log(&format!("[Chapters] Found {} keyframes for snapping.", kfs.len()));
            kfs
        }
        Err(e) => {
            runner.log(&format!("[WARN] Could not parse ffprobe keyframe JSON: {}", e));
            vec![]
        }
    }
}

fn snap_times_inplace(
    root: &mut Element,
    keyframes_ns: &[i64],
    mode: &str,            // "previous" | "nearest"
    threshold_ms: i64,     // default 250
    starts_only: bool,     // default true
    runner: &CommandRunner,
) {
    if keyframes_ns.is_empty() {
        return;
    }
    let thr_ns = threshold_ms * 1_000_000;
    let mut moved = 0;
    let mut on_kf = 0;
    let mut too_far = 0;

    for atom in collect_chapter_atoms_mut(root) {
        let mut tags = vec!["ChapterTimeStart"];
        if !starts_only {
            tags.push("ChapterTimeEnd");
        }
        for tag in tags {
            if let Some(mut el) = child_mut(atom, tag) {
                if let Some(ts) = element_text(&el) {
                    let orig = parse_ns(&ts).unwrap_or(0);
                    let cand = pick_candidate(orig, keyframes_ns, mode);
                    let delta = (orig - cand).abs();
                    if delta == 0 {
                        if tag == "ChapterTimeStart" {
                            on_kf += 1;
                        }
                    } else if delta <= thr_ns {
                        set_element_text(&mut el, &fmt_ns(cand));
                        if tag == "ChapterTimeStart" {
                            moved += 1;
                        }
                    } else {
                        if tag == "ChapterTimeStart" {
                            too_far += 1;
                        }
                    }
                }
            }
        }
    }

    runner.log(&format!(
        "[Chapters] Snap result: moved={}, on_kf={}, too_far={} (kfs={}, mode={}, thr={}ms, starts_only={})",
                        moved, on_kf, too_far, keyframes_ns.len(), mode, threshold_ms, starts_only
    ));
}

fn pick_candidate(ts_ns: i64, keyframes_ns: &[i64], mode: &str) -> i64 {
    if keyframes_ns.is_empty() {
        return ts_ns;
    }
    // bisect_right (upper bound): first index where kf > ts_ns
    let i = keyframes_ns.partition_point(|&x| x <= ts_ns);
    let prev_kf = if i > 0 { keyframes_ns[i - 1] } else { keyframes_ns[0] };
    if mode.eq_ignore_ascii_case("previous") {
        return prev_kf;
    }
    let next_kf = if i < keyframes_ns.len() { keyframes_ns[i] } else { *keyframes_ns.last().unwrap() };
    if (ts_ns - prev_kf).abs() <= (ts_ns - next_kf).abs() {
        prev_kf
    } else {
        next_kf
    }
}

// ---------- Normalization ----------

fn normalize_end_times(root: &mut Element, runner: &CommandRunner) {
    // Collect (start_ns, &mut Element)
    let mut chapters: Vec<(i64, *mut Element)> = vec![];
    for atom in collect_chapter_atoms_mut(root) {
        if let Some(st_el) = child_mut(atom, "ChapterTimeStart") {
            if let Some(ts) = element_text(&st_el) {
                let ns = parse_ns(&ts).unwrap_or(0);
                chapters.push((ns, atom as *mut Element));
            }
        }
    }
    // Sort by start_ns
    chapters.sort_by_key(|(ns, _)| *ns);

    let mut fixed_count = 0usize;

    for idx in 0..chapters.len() {
        let (st_ns, atom_ptr) = chapters[idx];
        let atom = unsafe { &mut *atom_ptr };

        // Current end or default st+1ms
        let mut en_ns = match child_mut(atom, "ChapterTimeEnd").and_then(|el| element_text(&el)) {
            Some(s) => parse_ns(&s).unwrap_or(st_ns + 1_000_000),
            None => st_ns + 1_000_000,
        };

        // Cap to next start if present
        if let Some((next_start, _)) = chapters.get(idx + 1) {
            en_ns = min(en_ns, *next_start);
        }
        // Ensure >= start + 1 ns
        en_ns = max(en_ns, st_ns + 1);

        // Write back (create if missing or different)
        let need_write = match child_mut(atom, "ChapterTimeEnd") {
            Some(mut el) => {
                let new_text = fmt_ns(en_ns);
                let changed = element_text(&el).map(|t| t != new_text).unwrap_or(true);
                if changed {
                    set_element_text(&mut el, &new_text);
                }
                changed
            }
            None => {
                let mut el = Element::new("ChapterTimeEnd");
                el.children.push(XMLNode::Text(fmt_ns(en_ns)));
                atom.children.push(XMLNode::Element(el));
                true
            }
        };

        if need_write {
            fixed_count += 1;
        }
    }

    if fixed_count > 0 {
        runner.log(&format!("[Chapters] Normalized {} chapter end times.", fixed_count));
    }
}

// ---------- XML helpers ----------

fn collect_chapter_atoms_mut<'a>(root: &'a mut Element) -> Vec<&'a mut Element> {
    // Descend: Chapters -> ... -> ChapterAtom (unknown nesting depth)
    let mut out: Vec<&'a mut Element> = vec![];
    collect_named_mut(root, "ChapterAtom", &mut out);
    out
}

fn collect_named_mut<'a>(el: &'a mut Element, target: &str, acc: &mut Vec<&'a mut Element>) {
    if el.name == target {
        acc.push(el);
    }
    for child in el.children.iter_mut() {
        if let XMLNode::Element(ref mut e) = child {
            collect_named_mut(e, target, acc);
        }
    }
}

fn child_mut<'a>(el: &'a mut Element, name: &str) -> Option<&'a mut Element> {
    for child in el.children.iter_mut() {
        if let XMLNode::Element(ref mut e) = child {
            if e.name == name {
                return Some(e);
            }
        }
    }
    None
}

fn element_text(el: &Element) -> Option<String> {
    for c in &el.children {
        if let XMLNode::Text(s) = c {
            return Some(s.clone());
        }
    }
    None
}

fn set_element_text(el: &mut Element, text: &str) {
    // replace existing text node or append one
    for c in el.children.iter_mut() {
        if let XMLNode::Text(s) = c {
            *s = text.to_string();
            return;
        }
    }
    el.children.push(XMLNode::Text(text.to_string()));
}

// ---------- Time helpers ----------

fn strip_bom(s: &str) -> String {
    s.strip_prefix('\u{feff}').unwrap_or(s).to_string()
}

fn parse_ns(t: &str) -> Option<i64> {
    // "HH:MM:SS.frac" -> ns; pad to 9 digits (Python parity)
    // Use regex to split reliably
    // Accept variable fraction length
    // returns non-negative (raw), let fmt clamp later
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"^\s*(\d{2}):(\d{2}):(\d{2})\.(\d{1,9})\s*$").unwrap()
    });
    let caps = RE.captures(t)?;
    let hh: i64 = caps.get(1)?.as_str().parse().ok()?;
    let mm: i64 = caps.get(2)?.as_str().parse().ok()?;
    let ss: i64 = caps.get(3)?.as_str().parse().ok()?;
    let mut frac = caps.get(4)?.as_str().to_string();
    while frac.len() < 9 { frac.push('0'); }
    let frac_ns: i64 = frac.parse().ok()?;
    let total_s = hh * 3600 + mm * 60 + ss;
    Some(total_s * 1_000_000_000 + frac_ns)
}

fn fmt_ns(mut ns: i64) -> String {
    if ns < 0 { ns = 0; }
    let frac = (ns % 1_000_000_000).abs();
    let total_s = ns / 1_000_000_000;
    let hh = total_s / 3600;
    let mm = (total_s % 3600) / 60;
    let ss = total_s % 60;
    format!("{:02}:{:02}:{:02}.{:09}", hh, mm, ss, frac)
}
