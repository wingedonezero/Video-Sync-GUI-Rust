//! Frame alignment audit for subtitle synchronization.
//!
//! Analyzes subtitle timing after sync offset is applied but before save,
//! to detect cases where centisecond rounding would cause frame drift.
//!
//! This is a diagnostic tool - it does not modify any timing.
//!
//! 1:1 port of `vsg_core/subtitles/frame_utils/frame_audit.py`.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};

use crate::subtitles::data::SubtitleData;
use super::surgical_rounding::{surgical_round_batch, surgical_round_event, SurgicalBatchStats};

const EPSILON: f64 = 1e-6;

/// Single frame alignment issue detected.
#[derive(Debug, Clone)]
pub struct FrameAuditIssue {
    pub line_index: usize,
    pub text_preview: String,
    pub timestamp_display: String,

    // Start time analysis
    pub exact_start_ms: f64,
    pub rounded_start_ms: i64,
    pub target_start_frame: i64,
    pub actual_start_frame: i64,
    pub start_drift: i64,
    pub start_fix_needed_ms: i64,

    // End time analysis
    pub exact_end_ms: f64,
    pub rounded_end_ms: i64,
    pub target_end_frame: i64,
    pub actual_end_frame: i64,
    pub end_drift: i64,
    pub end_fix_needed_ms: i64,

    // Duration
    pub original_duration_ms: f64,
    pub rounded_duration_ms: i64,
    pub duration_delta_ms: i64,
}

impl FrameAuditIssue {
    /// Categorize the issue.
    pub fn issue_type(&self) -> &str {
        if self.start_drift != 0 && self.end_drift != 0 {
            "BOTH_DRIFT"
        } else if self.start_drift < 0 {
            "START_EARLY"
        } else if self.start_drift > 0 {
            "START_LATE"
        } else if self.end_drift < 0 {
            "END_EARLY"
        } else if self.end_drift > 0 {
            "END_LATE"
        } else {
            "OK"
        }
    }
}

/// Complete audit result for a sync job.
#[derive(Debug, Clone)]
pub struct FrameAuditResult {
    pub job_name: String,
    pub fps: f64,
    pub frame_duration_ms: f64,
    pub rounding_mode: String,
    pub offset_applied_ms: f64,
    pub total_events: usize,
    pub audit_timestamp: DateTime<Local>,

    // Start time stats
    pub start_ok: i64,
    pub start_early: i64,
    pub start_late: i64,

    // End time stats
    pub end_ok: i64,
    pub end_early: i64,
    pub end_late: i64,

    // Frame span stats
    pub span_ok: i64,
    pub span_changed: i64,

    // Duration stats
    pub duration_unchanged: i64,
    pub duration_delta_10ms: i64,
    pub duration_delta_20ms: i64,
    pub duration_delta_large: i64,

    // Issues list
    pub issues: Vec<FrameAuditIssue>,

    // Rounding mode comparison
    pub floor_issues: i64,
    pub round_issues: i64,
    pub ceil_issues: i64,

    // Surgical rounding predictions
    pub predicted_corrections: i64,
    pub predicted_correction_events: i64,

    // Post-save correction stats
    pub corrected_timing_points: i64,
    pub corrected_events: i64,
    pub coordinated_ends: i64,
    pub correction_applied: bool,
}

impl FrameAuditResult {
    pub fn total_start_issues(&self) -> i64 {
        self.start_early + self.start_late
    }

    pub fn total_end_issues(&self) -> i64 {
        self.end_early + self.end_late
    }

    pub fn has_issues(&self) -> bool {
        !self.issues.is_empty()
    }

    /// Update result with post-save surgical rounding statistics.
    pub fn apply_correction_stats(&mut self, stats: &SurgicalBatchStats) {
        self.corrected_timing_points = stats.points_different_from_floor;
        self.corrected_events = stats.events_with_adjustments;
        self.coordinated_ends = stats.ends_coordinated;
        self.correction_applied = true;
    }
}

fn time_to_frame(time_ms: f64, frame_duration_ms: f64) -> i64 {
    ((time_ms + EPSILON) / frame_duration_ms) as i64
}

fn round_to_centisecond(ms: f64, mode: &str) -> i64 {
    let value = ms / 10.0;
    match mode {
        "ceil" => (value.ceil() as i64) * 10,
        "round" => (value.round() as i64) * 10,
        _ => (value.floor() as i64) * 10, // floor (default)
    }
}

fn find_minimal_fix(
    exact_ms: f64,
    target_frame: i64,
    frame_duration_ms: f64,
    rounding_mode: &str,
) -> i64 {
    let frame_start_ms = target_frame as f64 * frame_duration_ms;
    let frame_end_ms = (target_frame + 1) as f64 * frame_duration_ms;

    let rounded = round_to_centisecond(exact_ms, rounding_mode);
    let actual_frame = time_to_frame(rounded as f64, frame_duration_ms);

    if actual_frame == target_frame {
        return 0;
    }

    // Try rounding up (ceil) to get into frame
    let ceil_cs = (frame_start_ms / 10.0).ceil() as i64 * 10;
    if time_to_frame(ceil_cs as f64, frame_duration_ms) == target_frame {
        return ceil_cs - rounded;
    }

    // Try rounding down from frame end
    let floor_cs = ((frame_end_ms - 0.1) / 10.0).floor() as i64 * 10;
    if time_to_frame(floor_cs as f64, frame_duration_ms) == target_frame {
        return floor_cs - rounded;
    }

    // Fallback
    frame_start_ms as i64 - rounded
}

fn format_timestamp(ms: f64) -> String {
    let total_cs = (ms / 10.0) as i64;
    let cs = total_cs % 100;
    let total_seconds = total_cs / 100;
    let seconds = total_seconds % 60;
    let total_minutes = total_seconds / 60;
    let minutes = total_minutes % 60;
    let hours = total_minutes / 60;
    format!("{:02}:{:02}:{:02}.{:02}", hours, minutes, seconds, cs)
}

/// Audit frame alignment for all subtitle events.
///
/// This analyzes the current timing (after sync offset applied) and checks
/// whether centisecond rounding will cause any events to land on wrong frames.
pub fn run_frame_audit(
    subtitle_data: &SubtitleData,
    fps: f64,
    rounding_mode: &str,
    offset_ms: f64,
    job_name: &str,
    log: Option<&dyn Fn(&str)>,
) -> FrameAuditResult {
    let frame_duration_ms = 1000.0 / fps;

    let mut result = FrameAuditResult {
        job_name: job_name.to_string(),
        fps,
        frame_duration_ms,
        rounding_mode: rounding_mode.to_string(),
        offset_applied_ms: offset_ms,
        total_events: subtitle_data.events.len(),
        audit_timestamp: Local::now(),
        start_ok: 0,
        start_early: 0,
        start_late: 0,
        end_ok: 0,
        end_early: 0,
        end_late: 0,
        span_ok: 0,
        span_changed: 0,
        duration_unchanged: 0,
        duration_delta_10ms: 0,
        duration_delta_20ms: 0,
        duration_delta_large: 0,
        issues: Vec::new(),
        floor_issues: 0,
        round_issues: 0,
        ceil_issues: 0,
        predicted_corrections: 0,
        predicted_correction_events: 0,
        corrected_timing_points: 0,
        corrected_events: 0,
        coordinated_ends: 0,
        correction_applied: false,
    };

    if let Some(log_fn) = log {
        log_fn(&format!(
            "[FrameAudit] Starting audit: {} events",
            subtitle_data.events.len()
        ));
        log_fn(&format!(
            "[FrameAudit] FPS: {:.3}, Frame duration: {:.3}ms",
            fps, frame_duration_ms
        ));
        log_fn(&format!("[FrameAudit] Rounding mode: {}", rounding_mode));
    }

    for (idx, event) in subtitle_data.events.iter().enumerate() {
        if event.is_comment {
            continue;
        }

        let exact_start = event.start_ms;
        let exact_end = event.end_ms;

        // What frames should these land on?
        let target_start_frame = time_to_frame(exact_start, frame_duration_ms);
        let target_end_frame = time_to_frame(exact_end, frame_duration_ms);
        let target_span = target_end_frame - target_start_frame;

        // What will rounding produce?
        let rounded_start = round_to_centisecond(exact_start, rounding_mode);
        let rounded_end = round_to_centisecond(exact_end, rounding_mode);

        // What frames do rounded times land on?
        let actual_start_frame = time_to_frame(rounded_start as f64, frame_duration_ms);
        let actual_end_frame = time_to_frame(rounded_end as f64, frame_duration_ms);
        let actual_span = actual_end_frame - actual_start_frame;

        let start_drift = actual_start_frame - target_start_frame;
        let end_drift = actual_end_frame - target_end_frame;

        // Duration analysis
        let original_duration = exact_end - exact_start;
        let rounded_duration = rounded_end - rounded_start;
        let duration_delta = rounded_duration - original_duration as i64;

        // Count stats
        if start_drift == 0 {
            result.start_ok += 1;
        } else if start_drift < 0 {
            result.start_early += 1;
        } else {
            result.start_late += 1;
        }

        if end_drift == 0 {
            result.end_ok += 1;
        } else if end_drift < 0 {
            result.end_early += 1;
        } else {
            result.end_late += 1;
        }

        if actual_span == target_span {
            result.span_ok += 1;
        } else {
            result.span_changed += 1;
        }

        if duration_delta == 0 {
            result.duration_unchanged += 1;
        } else if duration_delta.abs() <= 10 {
            result.duration_delta_10ms += 1;
        } else if duration_delta.abs() <= 20 {
            result.duration_delta_20ms += 1;
        } else {
            result.duration_delta_large += 1;
        }

        // Check what other rounding modes would do
        for (mode, counter) in [
            ("floor", &mut result.floor_issues),
            ("round", &mut result.round_issues),
            ("ceil", &mut result.ceil_issues),
        ] {
            let alt_rounded_start = round_to_centisecond(exact_start, mode);
            let alt_start_frame = time_to_frame(alt_rounded_start as f64, frame_duration_ms);
            if alt_start_frame != target_start_frame {
                *counter += 1;
            }
        }

        // Record issue if there's drift
        if start_drift != 0 || end_drift != 0 {
            let mut text_preview = event.text.clone();
            if text_preview.len() > 40 {
                text_preview.truncate(40);
                text_preview.push_str("...");
            }
            text_preview = text_preview.replace('\n', " ").replace("\\N", " ");

            let issue = FrameAuditIssue {
                line_index: idx,
                text_preview,
                timestamp_display: format_timestamp(exact_start),
                exact_start_ms: exact_start,
                rounded_start_ms: rounded_start,
                target_start_frame,
                actual_start_frame,
                start_drift,
                start_fix_needed_ms: find_minimal_fix(
                    exact_start,
                    target_start_frame,
                    frame_duration_ms,
                    rounding_mode,
                ),
                exact_end_ms: exact_end,
                rounded_end_ms: rounded_end,
                target_end_frame,
                actual_end_frame,
                end_drift,
                end_fix_needed_ms: find_minimal_fix(
                    exact_end,
                    target_end_frame,
                    frame_duration_ms,
                    rounding_mode,
                ),
                original_duration_ms: original_duration,
                rounded_duration_ms: rounded_duration,
                duration_delta_ms: duration_delta,
            };
            result.issues.push(issue);
        }
    }

    // Predict surgical rounding corrections if issues were found
    if result.has_issues() {
        let (_, surgical_stats) = surgical_round_batch(&subtitle_data.events, frame_duration_ms);
        result.predicted_corrections = surgical_stats.points_different_from_floor;
        result.predicted_correction_events = surgical_stats.events_with_adjustments;
    }

    if let Some(log_fn) = log {
        log_fn(&format!(
            "[FrameAudit] Audit complete: {} issues found",
            result.issues.len()
        ));
    }

    result
}

/// Write the audit report to a file.
pub fn write_audit_report(
    result: &FrameAuditResult,
    output_dir: &Path,
    log: Option<&dyn Fn(&str)>,
) -> PathBuf {
    let _ = std::fs::create_dir_all(output_dir);

    let timestamp_str = result.audit_timestamp.format("%Y%m%d_%H%M%S").to_string();
    let safe_job_name: String = result
        .job_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let filename = format!("{}_{}_frame_audit.txt", safe_job_name, timestamp_str);
    let output_path = output_dir.join(&filename);

    let mut lines = Vec::new();

    // Header
    lines.push("=".repeat(70));
    lines.push("FRAME ALIGNMENT AUDIT REPORT".to_string());
    lines.push("=".repeat(70));
    lines.push(String::new());
    lines.push(format!("Job: {}", result.job_name));
    lines.push(format!(
        "Audit time: {}",
        result.audit_timestamp.format("%Y-%m-%d %H:%M:%S")
    ));
    lines.push(format!(
        "Sync offset applied: {:+.3}ms",
        result.offset_applied_ms
    ));
    lines.push(format!(
        "Target FPS: {:.3} (frame duration: {:.3}ms)",
        result.fps, result.frame_duration_ms
    ));
    lines.push(format!("Rounding mode: {}", result.rounding_mode));
    lines.push(format!("Total events: {}", result.total_events));
    lines.push(String::new());

    // Summary
    lines.push("=".repeat(70));
    lines.push("SUMMARY".to_string());
    lines.push("=".repeat(70));
    lines.push(String::new());

    let total = result.total_events as f64;
    if total > 0.0 {
        lines.push("Start times:".to_string());
        lines.push(format!(
            "  Correct frame:     {:4} ({:.1}%)",
            result.start_ok,
            100.0 * result.start_ok as f64 / total
        ));
        lines.push(format!(
            "  1+ frame early:    {:4} ({:.1}%)",
            result.start_early,
            100.0 * result.start_early as f64 / total
        ));
        lines.push(format!(
            "  1+ frame late:     {:4} ({:.1}%)",
            result.start_late,
            100.0 * result.start_late as f64 / total
        ));
        lines.push(String::new());

        lines.push("End times:".to_string());
        lines.push(format!(
            "  Correct frame:     {:4} ({:.1}%)",
            result.end_ok,
            100.0 * result.end_ok as f64 / total
        ));
        lines.push(format!(
            "  1+ frame early:    {:4} ({:.1}%)",
            result.end_early,
            100.0 * result.end_early as f64 / total
        ));
        lines.push(format!(
            "  1+ frame late:     {:4} ({:.1}%)",
            result.end_late,
            100.0 * result.end_late as f64 / total
        ));
        lines.push(String::new());

        lines.push("Frame span:".to_string());
        lines.push(format!(
            "  Correct span:      {:4} ({:.1}%)",
            result.span_ok,
            100.0 * result.span_ok as f64 / total
        ));
        lines.push(format!(
            "  Span changed:      {:4} ({:.1}%)",
            result.span_changed,
            100.0 * result.span_changed as f64 / total
        ));
        lines.push(String::new());

        lines.push("Duration delta:".to_string());
        lines.push(format!(
            "  Unchanged (0ms):   {:4} ({:.1}%)",
            result.duration_unchanged,
            100.0 * result.duration_unchanged as f64 / total
        ));
        lines.push(format!(
            "  +/-10ms:           {:4} ({:.1}%)",
            result.duration_delta_10ms,
            100.0 * result.duration_delta_10ms as f64 / total
        ));
        lines.push(format!(
            "  +/-20ms:           {:4} ({:.1}%)",
            result.duration_delta_20ms,
            100.0 * result.duration_delta_20ms as f64 / total
        ));
        lines.push(format!(
            "  >+/-20ms:          {:4} ({:.1}%)",
            result.duration_delta_large,
            100.0 * result.duration_delta_large as f64 / total
        ));
        lines.push(String::new());

        // Rounding mode comparison
        lines.push("Rounding mode comparison (start time issues):".to_string());
        lines.push(format!("  Floor:             {:4} issues", result.floor_issues));
        lines.push(format!("  Round:             {:4} issues", result.round_issues));
        lines.push(format!("  Ceil:              {:4} issues", result.ceil_issues));

        let best_mode = [
            ("floor", result.floor_issues),
            ("round", result.round_issues),
            ("ceil", result.ceil_issues),
        ]
        .iter()
        .min_by_key(|&&(_, count)| count)
        .map(|&(name, _)| name)
        .unwrap_or("floor");
        lines.push(format!(
            "  Suggested mode:    {} (fewest issues)",
            best_mode
        ));
        lines.push(String::new());

        // Surgical rounding section
        if result.correction_applied {
            lines.push("Surgical frame-aware rounding (APPLIED at save):".to_string());
            lines.push(format!(
                "  Timing points corrected: {}",
                result.corrected_timing_points
            ));
            lines.push(format!(
                "  Events with corrections: {}",
                result.corrected_events
            ));
            if result.coordinated_ends > 0 {
                lines.push(format!(
                    "  Coordinated end times:   {}",
                    result.coordinated_ends
                ));
            }
            lines.push("  Result: All frame drift issues resolved".to_string());
            lines.push(String::new());
        } else if result.predicted_corrections > 0 {
            lines.push("Surgical frame-aware rounding (PREDICTED):".to_string());
            lines.push(format!(
                "  Would correct: {} timing points",
                result.predicted_corrections
            ));
            lines.push(format!(
                "  Would affect:  {} events",
                result.predicted_correction_events
            ));
            lines.push(String::new());
        }
    }

    // Issues section
    if !result.issues.is_empty() {
        lines.push("=".repeat(70));
        lines.push(format!(
            "ISSUES ({} events with frame drift)",
            result.issues.len()
        ));
        lines.push("=".repeat(70));
        lines.push(String::new());

        let mut sorted_issues = result.issues.clone();
        sorted_issues.sort_by(|a, b| {
            a.issue_type()
                .cmp(b.issue_type())
                .then(a.line_index.cmp(&b.line_index))
        });

        for issue in &sorted_issues {
            let surg = surgical_round_event(
                issue.exact_start_ms,
                issue.exact_end_ms,
                result.frame_duration_ms,
            );

            lines.push(format!(
                "[{}] Line {} @ {}",
                issue.issue_type(),
                issue.line_index,
                issue.timestamp_display
            ));
            lines.push(format!("  Text: \"{}\"", issue.text_preview));

            if issue.start_drift != 0 {
                let direction = if issue.start_drift < 0 {
                    "EARLY"
                } else {
                    "LATE"
                };
                lines.push(format!(
                    "  Start: {:.2}ms -> {}ms (frame {} -> {}) {} FRAME {}",
                    issue.exact_start_ms,
                    issue.rounded_start_ms,
                    issue.target_start_frame,
                    issue.actual_start_frame,
                    issue.start_drift.abs(),
                    direction
                ));
                lines.push(format!(
                    "  Would need: {:+}ms to fix start",
                    issue.start_fix_needed_ms
                ));
                lines.push(format!(
                    "  Fix: floor({}ms) -> surgical({}ms) [{}]",
                    issue.rounded_start_ms, surg.start.centisecond_ms, surg.start.method
                ));
            } else {
                lines.push(format!(
                    "  Start OK: frame {}",
                    issue.target_start_frame
                ));
            }

            if issue.end_drift != 0 {
                let direction = if issue.end_drift < 0 {
                    "EARLY"
                } else {
                    "LATE"
                };
                lines.push(format!(
                    "  End: {:.2}ms -> {}ms (frame {} -> {}) {} FRAME {}",
                    issue.exact_end_ms,
                    issue.rounded_end_ms,
                    issue.target_end_frame,
                    issue.actual_end_frame,
                    issue.end_drift.abs(),
                    direction
                ));
                lines.push(format!(
                    "  Would need: {:+}ms to fix end",
                    issue.end_fix_needed_ms
                ));
                lines.push(format!(
                    "  Fix: floor({}ms) -> surgical({}ms) [{}]",
                    issue.rounded_end_ms, surg.end.centisecond_ms, surg.end.method
                ));
            } else if surg.end.method == "coordinated_ceil" {
                lines.push(format!(
                    "  End OK: frame {} (coordinated: {}ms to preserve duration)",
                    issue.target_end_frame, surg.end.centisecond_ms
                ));
            } else {
                lines.push(format!(
                    "  End OK: frame {}",
                    issue.target_end_frame
                ));
            }

            if issue.duration_delta_ms != 0 {
                lines.push(format!(
                    "  Duration: {:.1}ms -> {}ms ({:+}ms)",
                    issue.original_duration_ms,
                    issue.rounded_duration_ms,
                    issue.duration_delta_ms
                ));
            }

            lines.push(String::new());
        }
    } else {
        lines.push("=".repeat(70));
        lines.push("NO ISSUES DETECTED".to_string());
        lines.push("=".repeat(70));
        lines.push(String::new());
        lines.push(
            "All subtitle events will land on their correct frames after rounding.".to_string(),
        );
        lines.push(String::new());
    }

    // Footer
    lines.push("=".repeat(70));
    lines.push("END OF REPORT".to_string());
    lines.push("=".repeat(70));

    let _ = std::fs::write(&output_path, lines.join("\n"));

    if let Some(log_fn) = log {
        log_fn(&format!(
            "[FrameAudit] Report written to: {}",
            output_path.display()
        ));
    }

    output_path
}
