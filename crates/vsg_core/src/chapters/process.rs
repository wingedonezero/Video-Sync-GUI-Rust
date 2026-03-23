//! Chapter processing — 1:1 port of `vsg_core/chapters/process.py`.
//!
//! Handles chapter extraction, shifting, snapping, normalization,
//! deduplication, and renaming. Uses quick-xml for both parsing and writing.

use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;

use quick_xml::escape::unescape;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;

use crate::io::runner::CommandRunner;
use crate::models::settings::AppSettings;

use super::keyframes::probe_keyframes_ns;

// ─── Time format helpers ─────────────────────────────────────────────────────

/// Parse "HH:MM:SS.nnnnnnnnn" to nanoseconds — `_parse_ns`
pub fn parse_ns(t: &str) -> i64 {
    let t = t.trim();
    let parts: Vec<&str> = t.splitn(3, ':').collect();
    if parts.len() != 3 {
        return 0;
    }
    let hh: i64 = parts[0].parse().unwrap_or(0);
    let mm: i64 = parts[1].parse().unwrap_or(0);

    // Split seconds and fractional part
    let (ss_str, frac_str) = if let Some(dot_pos) = parts[2].find('.') {
        (&parts[2][..dot_pos], &parts[2][dot_pos + 1..])
    } else {
        (parts[2], "0")
    };

    let ss: i64 = ss_str.parse().unwrap_or(0);
    // Pad or truncate fractional part to 9 digits
    let mut frac_padded = frac_str.to_string();
    while frac_padded.len() < 9 {
        frac_padded.push('0');
    }
    frac_padded.truncate(9);
    let frac: i64 = frac_padded.parse().unwrap_or(0);

    (hh * 3600 + mm * 60 + ss) * 1_000_000_000 + frac
}

/// Format nanoseconds as "HH:MM:SS.nnnnnnnnn" — `_fmt_ns`
pub fn fmt_ns(ns: i64) -> String {
    let ns = ns.max(0);
    let frac = ns % 1_000_000_000;
    let total_s = ns / 1_000_000_000;
    let hh = total_s / 3600;
    let mm = (total_s % 3600) / 60;
    let ss = total_s % 60;
    format!("{hh:02}:{mm:02}:{ss:02}.{frac:09}")
}

/// Format nanoseconds for log as HH:MM:SS.mmm.uuu.nnn — `_fmt_ns_for_log`
fn fmt_ns_for_log(ns: i64) -> String {
    let ns = ns.max(0) as u64;
    let total_us = ns / 1_000;
    let remaining_ns = ns % 1_000;
    let total_ms = total_us / 1_000;
    let remaining_us = total_us % 1_000;
    let total_s = total_ms / 1_000;
    let remaining_ms = total_ms % 1_000;
    let hh = total_s / 3600;
    let mm = (total_s % 3600) / 60;
    let ss = total_s % 60;
    format!("{hh:02}:{mm:02}:{ss:02}.{remaining_ms:03}.{remaining_us:03}.{remaining_ns:03}")
}

/// Format a time delta for logging with unit-adaptive display — `_fmt_delta_for_log`
fn fmt_delta_for_log(delta_ns: i64) -> String {
    let abs_delta = delta_ns.unsigned_abs();
    let sign = if delta_ns > 0 { "+" } else { "-" };

    if abs_delta == 0 {
        "0ns".to_string()
    } else if abs_delta < 1_000 {
        format!("{sign}{abs_delta}ns")
    } else if abs_delta < 1_000_000 {
        let us_value = abs_delta as f64 / 1_000.0;
        format!("{sign}{us_value:.3}\u{00b5}s")
    } else {
        let ms_value = abs_delta as f64 / 1_000_000.0;
        format!("{sign}{ms_value:.3}ms")
    }
}

// ─── Language helpers ────────────────────────────────────────────────────────

/// Common ISO 639-2 to IETF language mapping.
fn lang_639_to_ietf(code: &str) -> &str {
    match code {
        "eng" => "en",
        "jpn" => "ja",
        "spa" => "es",
        "fra" => "fr",
        "deu" => "de",
        "ita" => "it",
        "por" => "pt",
        "rus" => "ru",
        "kor" => "ko",
        "zho" => "zh",
        _ => "und",
    }
}

// ─── Internal chapter data structures ────────────────────────────────────────

/// Represents a chapter display entry (name + languages).
#[derive(Debug, Clone)]
struct ChapterDisplay {
    chapter_string: String,
    chapter_language: String,
    chapter_language_ietf: String,
}

/// Represents a single chapter atom.
#[derive(Debug, Clone)]
struct ChapterAtom {
    start_ns: i64,
    end_ns: Option<i64>,
    displays: Vec<ChapterDisplay>,
}

// ─── XML parsing with quick-xml ──────────────────────────────────────────────

/// Parse chapter XML into a list of ChapterAtom structs.
fn parse_chapter_xml(xml_content: &str) -> Result<Vec<ChapterAtom>, String> {
    let mut reader = Reader::from_str(xml_content);
    reader.config_mut().trim_text(false);

    let mut chapters: Vec<ChapterAtom> = Vec::new();
    let mut buf = Vec::new();

    // State tracking for nested elements
    let mut in_chapter_atom = false;
    let mut in_chapter_display = false;
    let mut current_element = String::new();

    // Current chapter being built
    let mut current_start_ns: Option<i64> = None;
    let mut current_end_ns: Option<i64> = None;
    let mut current_displays: Vec<ChapterDisplay> = Vec::new();

    // Current display being built
    let mut current_string = String::new();
    let mut current_language = String::new();
    let mut current_ietf = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                match name.as_str() {
                    "ChapterAtom" => {
                        in_chapter_atom = true;
                        current_start_ns = None;
                        current_end_ns = None;
                        current_displays.clear();
                    }
                    "ChapterDisplay" if in_chapter_atom => {
                        in_chapter_display = true;
                        current_string.clear();
                        current_language.clear();
                        current_ietf.clear();
                    }
                    _ if in_chapter_atom => {
                        current_element = name;
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                match name.as_str() {
                    "ChapterAtom" => {
                        if let Some(start_ns) = current_start_ns {
                            let mut displays = current_displays.clone();
                            if displays.is_empty() {
                                displays.push(ChapterDisplay {
                                    chapter_string: format!(
                                        "Chapter {}",
                                        chapters.len() + 1
                                    ),
                                    chapter_language: "und".to_string(),
                                    chapter_language_ietf: "und".to_string(),
                                });
                            }
                            chapters.push(ChapterAtom {
                                start_ns,
                                end_ns: current_end_ns,
                                displays,
                            });
                        }
                        in_chapter_atom = false;
                    }
                    "ChapterDisplay" => {
                        if in_chapter_display {
                            let lang = if current_language.is_empty() {
                                "und".to_string()
                            } else {
                                current_language.clone()
                            };
                            let ietf = if current_ietf.is_empty() {
                                lang_639_to_ietf(&lang).to_string()
                            } else {
                                current_ietf.clone()
                            };
                            current_displays.push(ChapterDisplay {
                                chapter_string: current_string.clone(),
                                chapter_language: lang,
                                chapter_language_ietf: ietf,
                            });
                            in_chapter_display = false;
                        }
                    }
                    _ => {
                        current_element.clear();
                    }
                }
            }
            Ok(Event::Text(ref e)) => {
                let decoded = e
                    .decode()
                    .map_err(|err| format!("XML text decode error: {err}"))?;
                let text = unescape(&decoded)
                    .map_err(|err| format!("XML unescape error: {err}"))?
                    .to_string();

                if in_chapter_display {
                    match current_element.as_str() {
                        "ChapterString" => current_string.push_str(&text),
                        "ChapterLanguage" => current_language.push_str(text.trim()),
                        "ChapLanguageIETF" => current_ietf.push_str(text.trim()),
                        _ => {}
                    }
                } else if in_chapter_atom {
                    match current_element.as_str() {
                        "ChapterTimeStart" => {
                            current_start_ns = Some(parse_ns(&text));
                        }
                        "ChapterTimeEnd" => {
                            current_end_ns = Some(parse_ns(&text));
                        }
                        _ => {}
                    }
                }
            }
            // Handle XML entity references like &lt; &gt; &amp; &quot;
            Ok(Event::GeneralRef(ref e)) => {
                let entity = e.decode()
                    .map_err(|err| format!("XML entity decode error: {err}"))?;
                let resolved = match entity.as_ref() {
                    "lt" => "<",
                    "gt" => ">",
                    "amp" => "&",
                    "quot" => "\"",
                    "apos" => "'",
                    _ => "",
                };
                if !resolved.is_empty() && in_chapter_display {
                    match current_element.as_str() {
                        "ChapterString" => current_string.push_str(resolved),
                        "ChapterLanguage" => current_language.push_str(resolved),
                        "ChapLanguageIETF" => current_ietf.push_str(resolved),
                        _ => {}
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error at position {}: {e}", reader.error_position())),
            _ => {}
        }
        buf.clear();
    }

    chapters.sort_by_key(|c| c.start_ns);
    Ok(chapters)
}

// ─── XML writing with quick-xml ──────────────────────────────────────────────

/// Write chapters to Matroska chapter XML format using quick-xml writer.
fn write_chapter_xml(chapters: &[ChapterAtom]) -> Result<String, String> {
    let mut writer = Writer::new_with_indent(Cursor::new(Vec::new()), b' ', 2);

    // XML declaration
    writer
        .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
        .map_err(|e| format!("XML write error: {e}"))?;

    // Newline after declaration
    writer
        .write_event(Event::Text(BytesText::new("\n")))
        .map_err(|e| format!("XML write error: {e}"))?;

    // <Chapters>
    writer
        .write_event(Event::Start(BytesStart::new("Chapters")))
        .map_err(|e| format!("XML write error: {e}"))?;

    // <EditionEntry>
    writer
        .write_event(Event::Start(BytesStart::new("EditionEntry")))
        .map_err(|e| format!("XML write error: {e}"))?;

    for chapter in chapters {
        // <ChapterAtom>
        writer
            .write_event(Event::Start(BytesStart::new("ChapterAtom")))
            .map_err(|e| format!("XML write error: {e}"))?;

        // <ChapterTimeStart>
        write_text_element(&mut writer, "ChapterTimeStart", &fmt_ns(chapter.start_ns))?;

        // <ChapterTimeEnd>
        if let Some(end_ns) = chapter.end_ns {
            write_text_element(&mut writer, "ChapterTimeEnd", &fmt_ns(end_ns))?;
        }

        for display in &chapter.displays {
            // <ChapterDisplay>
            writer
                .write_event(Event::Start(BytesStart::new("ChapterDisplay")))
                .map_err(|e| format!("XML write error: {e}"))?;

            write_text_element(&mut writer, "ChapterString", &display.chapter_string)?;
            write_text_element(&mut writer, "ChapterLanguage", &display.chapter_language)?;
            write_text_element(
                &mut writer,
                "ChapLanguageIETF",
                &display.chapter_language_ietf,
            )?;

            // </ChapterDisplay>
            writer
                .write_event(Event::End(BytesEnd::new("ChapterDisplay")))
                .map_err(|e| format!("XML write error: {e}"))?;
        }

        // </ChapterAtom>
        writer
            .write_event(Event::End(BytesEnd::new("ChapterAtom")))
            .map_err(|e| format!("XML write error: {e}"))?;
    }

    // </EditionEntry>
    writer
        .write_event(Event::End(BytesEnd::new("EditionEntry")))
        .map_err(|e| format!("XML write error: {e}"))?;

    // </Chapters>
    writer
        .write_event(Event::End(BytesEnd::new("Chapters")))
        .map_err(|e| format!("XML write error: {e}"))?;

    let result = writer.into_inner().into_inner();
    String::from_utf8(result).map_err(|e| format!("UTF-8 error: {e}"))
}

/// Helper to write a simple text element like `<Tag>text</Tag>`.
fn write_text_element(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    tag: &str,
    text: &str,
) -> Result<(), String> {
    writer
        .write_event(Event::Start(BytesStart::new(tag)))
        .map_err(|e| format!("XML write error: {e}"))?;
    writer
        .write_event(Event::Text(BytesText::new(text)))
        .map_err(|e| format!("XML write error: {e}"))?;
    writer
        .write_event(Event::End(BytesEnd::new(tag)))
        .map_err(|e| format!("XML write error: {e}"))?;
    Ok(())
}

// ─── Processing functions ────────────────────────────────────────────────────

/// Normalize chapter end times and remove duplicates — `_normalize_and_dedupe_chapters`
fn normalize_and_dedupe_chapters(chapters: &mut Vec<ChapterAtom>, runner: &CommandRunner) {
    // Remove duplicates (same start time)
    let mut seen_start_times = std::collections::HashSet::new();
    chapters.retain(|chap| {
        if seen_start_times.contains(&chap.start_ns) {
            let name = chap
                .displays
                .first()
                .map(|d| d.chapter_string.as_str())
                .unwrap_or("Unknown");
            runner.log_message(&format!(
                "  - Removed duplicate chapter '{}' found at timestamp {}",
                name,
                fmt_ns_for_log(chap.start_ns)
            ));
            false
        } else {
            seen_start_times.insert(chap.start_ns);
            true
        }
    });

    // Normalize end times to create seamless chapters
    let len = chapters.len();
    for i in 0..len {
        let next_start_ns = if i + 1 < len {
            Some(chapters[i + 1].start_ns)
        } else {
            None
        };

        let chap = &mut chapters[i];
        let name = chap
            .displays
            .first()
            .map(|d| d.chapter_string.clone())
            .unwrap_or_else(|| format!("Chapter Atom {}", i + 1));

        let original_end_ns = chap.end_ns;

        let desired_end_ns = if let Some(next_start) = next_start_ns {
            next_start
        } else {
            let original = original_end_ns.unwrap_or(chap.start_ns);
            (chap.start_ns + 1_000_000_000).max(original)
        };

        let reason = if next_start_ns.is_some() {
            " (to create seamless chapters)"
        } else {
            ""
        };

        if chap.end_ns != Some(desired_end_ns) {
            let original_display = original_end_ns
                .map(fmt_ns_for_log)
                .unwrap_or_else(|| "None".to_string());
            runner.log_message(&format!(
                "  - Normalized '{}' end time: ({} -> {}){reason}",
                name,
                original_display,
                fmt_ns_for_log(desired_end_ns)
            ));
            chap.end_ns = Some(desired_end_ns);
        }
    }
}

/// Snap chapter times to keyframes — `_snap_chapter_times_inplace`
fn snap_chapter_times(
    chapters: &mut [ChapterAtom],
    keyframes_ns: &[i64],
    settings: &AppSettings,
    runner: &CommandRunner,
) {
    let mode = &settings.snap_mode;
    let threshold_ms = settings.snap_threshold_ms;
    let starts_only = settings.snap_starts_only;
    let threshold_ns = threshold_ms as i64 * 1_000_000;
    let mut moved = 0;
    let mut on_kf = 0;
    let mut too_far = 0;

    runner.log_message(&format!(
        "[Chapters] Snapping with mode={mode}, threshold={threshold_ms}ms..."
    ));

    let pick_candidate = |ts_ns: i64| -> i64 {
        if keyframes_ns.is_empty() {
            return ts_ns;
        }
        let i = keyframes_ns.partition_point(|&kf| kf <= ts_ns);
        let prev_kf = if i > 0 {
            keyframes_ns[i - 1]
        } else {
            keyframes_ns[0]
        };

        if mode.to_string() == "previous" {
            prev_kf
        } else {
            let next_kf = if i < keyframes_ns.len() {
                keyframes_ns[i]
            } else {
                *keyframes_ns.last().unwrap()
            };
            if (ts_ns - prev_kf).abs() <= (ts_ns - next_kf).abs() {
                prev_kf
            } else {
                next_kf
            }
        }
    };

    for (i, chapter) in chapters.iter_mut().enumerate() {
        let chapter_name = chapter
            .displays
            .first()
            .map(|d| d.chapter_string.clone())
            .unwrap_or_else(|| format!("Chapter Atom {}", i + 1));

        // Process start time
        {
            let original_ns = chapter.start_ns;
            let candidate_ns = pick_candidate(original_ns);
            let delta_ns = candidate_ns - original_ns;
            let abs_delta_ns = delta_ns.abs();

            if abs_delta_ns == 0 {
                on_kf += 1;
                runner.log_message(&format!(
                    "  - Kept '{}' ({}) - already on keyframe.",
                    chapter_name,
                    fmt_ns_for_log(original_ns)
                ));
            } else if abs_delta_ns <= threshold_ns {
                chapter.start_ns = candidate_ns;
                moved += 1;
                runner.log_message(&format!(
                    "  - Snapped '{}' ({}) -> {} (moved by {})",
                    chapter_name,
                    fmt_ns_for_log(original_ns),
                    fmt_ns_for_log(candidate_ns),
                    fmt_delta_for_log(delta_ns)
                ));
            } else {
                too_far += 1;
                runner.log_message(&format!(
                    "  - Skipped '{}' ({}) - nearest keyframe is {} away (exceeds threshold).",
                    chapter_name,
                    fmt_ns_for_log(original_ns),
                    fmt_delta_for_log(delta_ns)
                ));
            }
        }

        // Process end time (unless starts_only)
        if !starts_only {
            if let Some(end_ns) = chapter.end_ns {
                let candidate_ns = pick_candidate(end_ns);
                let delta_ns = candidate_ns - end_ns;
                let abs_delta_ns = delta_ns.abs();

                if abs_delta_ns == 0 {
                    runner.log_message(&format!(
                        "  - Kept '{}' ({}) - already on keyframe.",
                        chapter_name,
                        fmt_ns_for_log(end_ns)
                    ));
                } else if abs_delta_ns <= threshold_ns {
                    chapter.end_ns = Some(candidate_ns);
                    runner.log_message(&format!(
                        "  - Snapped '{}' ({}) -> {} (moved by {})",
                        chapter_name,
                        fmt_ns_for_log(end_ns),
                        fmt_ns_for_log(candidate_ns),
                        fmt_delta_for_log(delta_ns)
                    ));
                } else {
                    runner.log_message(&format!(
                        "  - Skipped '{}' ({}) - nearest keyframe is {} away (exceeds threshold).",
                        chapter_name,
                        fmt_ns_for_log(end_ns),
                        fmt_delta_for_log(delta_ns)
                    ));
                }
            }
        }
    }

    runner.log_message(&format!(
        "[Chapters] Snap complete: {moved} moved, {on_kf} on keyframe, {too_far} skipped."
    ));
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Process chapters from reference MKV — `process_chapters`
///
/// Extracts, optionally snaps to keyframes, shifts by delay,
/// normalizes, deduplicates, and optionally renames chapters.
pub fn process_chapters(
    ref_mkv: &str,
    temp_dir: &Path,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
    settings: &AppSettings,
    shift_ms: i32,
) -> Option<String> {
    let xml_content = runner.run(&["mkvextract", ref_mkv, "chapters", "-"], tool_paths)?;

    if xml_content.trim().is_empty() {
        runner.log_message("No chapters found in reference file.");
        return None;
    }

    // Strip BOM if present
    let xml_content = xml_content.strip_prefix('\u{feff}').unwrap_or(&xml_content);

    let mut chapters = match parse_chapter_xml(xml_content) {
        Ok(c) => c,
        Err(e) => {
            runner.log_message(&format!("[ERROR] Chapter processing failed: {e}"));
            return None;
        }
    };

    if chapters.is_empty() {
        runner.log_message("No chapters found in reference file.");
        return None;
    }

    // IMPORTANT: Snap FIRST (in video time), THEN shift to container time
    // This ensures chapters land on actual keyframes in the final muxed file
    if settings.snap_chapters {
        let keyframes_ns = probe_keyframes_ns(ref_mkv, runner, tool_paths);
        if !keyframes_ns.is_empty() {
            snap_chapter_times(&mut chapters, &keyframes_ns, settings, runner);
        } else {
            runner.log_message("[Chapters] Snap skipped: could not load keyframes.");
        }
    }

    // Shift all timestamps to container time
    // Must match video container delay exactly (integer ms) for correct keyframe alignment
    let shift_ns = shift_ms as i64 * 1_000_000;
    if shift_ns != 0 {
        runner.log_message(&format!(
            "[Chapters] Shifting all timestamps by +{shift_ms}ms."
        ));
        for chapter in &mut chapters {
            chapter.start_ns += shift_ns;
            if let Some(ref mut end_ns) = chapter.end_ns {
                *end_ns += shift_ns;
            }
        }
    }

    // Normalize and deduplicate
    runner.log_message("[Chapters] Normalizing chapter data...");
    normalize_and_dedupe_chapters(&mut chapters, runner);

    // Rename chapters if enabled
    if settings.rename_chapters {
        runner.log_message("[Chapters] Renaming chapters to \"Chapter NN\"...");
        for (i, chapter) in chapters.iter_mut().enumerate() {
            let (original_lang, original_ietf) = if let Some(display) = chapter.displays.first() {
                (
                    display.chapter_language.clone(),
                    display.chapter_language_ietf.clone(),
                )
            } else {
                ("und".to_string(), "und".to_string())
            };

            chapter.displays = vec![ChapterDisplay {
                chapter_string: format!("Chapter {:02}", i + 1),
                chapter_language: original_lang.clone(),
                chapter_language_ietf: original_ietf.clone(),
            }];

            runner.log_message(&format!(
                "  - Renamed chapter {} (language: {original_lang}, IETF: {original_ietf})",
                i + 1
            ));
        }
    }

    // Write output XML
    let mkv_stem = Path::new(ref_mkv)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let out_path = temp_dir.join(format!("{mkv_stem}_chapters_modified.xml"));

    let xml_output = match write_chapter_xml(&chapters) {
        Ok(xml) => xml,
        Err(e) => {
            runner.log_message(&format!("[ERROR] Chapter XML writing failed: {e}"));
            return None;
        }
    };

    match std::fs::write(&out_path, &xml_output) {
        Ok(()) => {
            runner.log_message(&format!("Chapters XML written to: {}", out_path.display()));
            Some(out_path.to_string_lossy().to_string())
        }
        Err(e) => {
            runner.log_message(&format!("[ERROR] Chapter processing failed: {e}"));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ns_basic() {
        assert_eq!(parse_ns("00:00:00.000000000"), 0);
        assert_eq!(parse_ns("00:00:01.000000000"), 1_000_000_000);
        assert_eq!(parse_ns("01:00:00.000000000"), 3_600_000_000_000);
        assert_eq!(parse_ns("00:01:31.074316666"), 91_074_316_666);
    }

    #[test]
    fn parse_ns_short_fraction() {
        // Python pads fraction to 9 digits
        assert_eq!(parse_ns("00:00:01.5"), 1_500_000_000);
        assert_eq!(parse_ns("00:00:01.50"), 1_500_000_000);
        assert_eq!(parse_ns("00:00:01.123"), 1_123_000_000);
    }

    #[test]
    fn fmt_ns_basic() {
        assert_eq!(fmt_ns(0), "00:00:00.000000000");
        assert_eq!(fmt_ns(1_000_000_000), "00:00:01.000000000");
        assert_eq!(fmt_ns(91_074_316_666), "00:01:31.074316666");
    }

    #[test]
    fn fmt_ns_negative_clamped() {
        assert_eq!(fmt_ns(-100), "00:00:00.000000000");
    }

    #[test]
    fn parse_fmt_round_trip() {
        let test_values = [0, 1_500_000_000, 91_074_316_666, 3_600_000_000_000];
        for ns in test_values {
            assert_eq!(parse_ns(&fmt_ns(ns)), ns);
        }
    }

    #[test]
    fn fmt_ns_for_log_works() {
        assert_eq!(fmt_ns_for_log(91_074_316_666), "00:01:31.074.316.666");
        assert_eq!(fmt_ns_for_log(0), "00:00:00.000.000.000");
    }

    #[test]
    fn fmt_delta_for_log_works() {
        assert_eq!(fmt_delta_for_log(0), "0ns");
        assert_eq!(fmt_delta_for_log(500), "+500ns");
        assert_eq!(fmt_delta_for_log(-500), "-500ns");
        assert_eq!(fmt_delta_for_log(1_500), "+1.500\u{00b5}s");
        assert_eq!(fmt_delta_for_log(1_500_000), "+1.500ms");
        assert_eq!(fmt_delta_for_log(-2_500_000), "-2.500ms");
    }

    #[test]
    fn parse_chapter_xml_basic() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Chapters>
  <EditionEntry>
    <ChapterAtom>
      <ChapterTimeStart>00:00:00.000000000</ChapterTimeStart>
      <ChapterTimeEnd>00:05:00.000000000</ChapterTimeEnd>
      <ChapterDisplay>
        <ChapterString>Intro</ChapterString>
        <ChapterLanguage>eng</ChapterLanguage>
      </ChapterDisplay>
    </ChapterAtom>
    <ChapterAtom>
      <ChapterTimeStart>00:05:00.000000000</ChapterTimeStart>
      <ChapterDisplay>
        <ChapterString>Main</ChapterString>
        <ChapterLanguage>eng</ChapterLanguage>
      </ChapterDisplay>
    </ChapterAtom>
  </EditionEntry>
</Chapters>"#;

        let chapters = parse_chapter_xml(xml).unwrap();
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].start_ns, 0);
        assert_eq!(chapters[0].end_ns, Some(300_000_000_000));
        assert_eq!(chapters[0].displays[0].chapter_string, "Intro");
        assert_eq!(chapters[0].displays[0].chapter_language, "eng");
        assert_eq!(chapters[0].displays[0].chapter_language_ietf, "en");
        assert_eq!(chapters[1].start_ns, 300_000_000_000);
        assert_eq!(chapters[1].end_ns, None);
    }

    #[test]
    fn parse_chapter_xml_with_namespace() {
        // Some MKVs use namespace in chapter XML
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Chapters xmlns="urn:ebml:matroska">
  <EditionEntry>
    <ChapterAtom>
      <ChapterTimeStart>00:00:00.000000000</ChapterTimeStart>
      <ChapterDisplay>
        <ChapterString>Test</ChapterString>
        <ChapterLanguage>jpn</ChapterLanguage>
      </ChapterDisplay>
    </ChapterAtom>
  </EditionEntry>
</Chapters>"#;

        let chapters = parse_chapter_xml(xml).unwrap();
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].displays[0].chapter_string, "Test");
        assert_eq!(chapters[0].displays[0].chapter_language, "jpn");
        assert_eq!(chapters[0].displays[0].chapter_language_ietf, "ja");
    }

    #[test]
    fn write_chapter_xml_produces_valid_xml() {
        let chapters = vec![
            ChapterAtom {
                start_ns: 0,
                end_ns: Some(300_000_000_000),
                displays: vec![ChapterDisplay {
                    chapter_string: "Chapter 01".to_string(),
                    chapter_language: "eng".to_string(),
                    chapter_language_ietf: "en".to_string(),
                }],
            },
            ChapterAtom {
                start_ns: 300_000_000_000,
                end_ns: Some(600_000_000_000),
                displays: vec![ChapterDisplay {
                    chapter_string: "Chapter 02".to_string(),
                    chapter_language: "eng".to_string(),
                    chapter_language_ietf: "en".to_string(),
                }],
            },
        ];

        let xml = write_chapter_xml(&chapters).unwrap();

        // Verify it's parseable
        let parsed = parse_chapter_xml(&xml).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].start_ns, 0);
        assert_eq!(parsed[1].start_ns, 300_000_000_000);
        assert_eq!(parsed[0].displays[0].chapter_string, "Chapter 01");

        // Verify structure
        assert!(xml.contains("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml.contains("<Chapters>"));
        assert!(xml.contains("<ChapterTimeStart>00:00:00.000000000</ChapterTimeStart>"));
        assert!(xml.contains("<ChapLanguageIETF>en</ChapLanguageIETF>"));
    }

    #[test]
    fn write_chapter_xml_escapes_special_chars() {
        let chapters = vec![ChapterAtom {
            start_ns: 0,
            end_ns: None,
            displays: vec![ChapterDisplay {
                chapter_string: "Part <1> & \"intro\"".to_string(),
                chapter_language: "eng".to_string(),
                chapter_language_ietf: "en".to_string(),
            }],
        }];

        let xml = write_chapter_xml(&chapters).unwrap();
        // quick-xml handles escaping automatically
        assert!(xml.contains("&lt;1&gt;"));
        assert!(xml.contains("&amp;"));

        // Verify round-trip preserves the text
        let parsed = parse_chapter_xml(&xml).unwrap();
        assert_eq!(
            parsed[0].displays[0].chapter_string,
            "Part <1> & \"intro\""
        );
    }
}
