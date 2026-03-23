//! Characters per second — 1:1 port of `vsg_qt/subtitle_editor/utils/cps.py`.
//!
//! Calculates reading speed for subtitle events.

/// Calculate characters per second for a subtitle event.
///
/// Returns `None` if duration is zero or negative.
pub fn calculate_cps(text: &str, duration_ms: i64) -> Option<f64> {
    if duration_ms <= 0 {
        return None;
    }
    // Strip ASS override tags for character count
    let clean_text = strip_ass_tags(text);
    let char_count = clean_text.chars().filter(|c| !c.is_whitespace()).count();
    let duration_seconds = duration_ms as f64 / 1000.0;
    Some(char_count as f64 / duration_seconds)
}

/// Strip ASS override tags from text: `{\tag}` → removed.
fn strip_ass_tags(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut in_tag = false;
    for ch in text.chars() {
        match ch {
            '{' if text.contains('\\') => in_tag = true,
            '}' if in_tag => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
}

/// Get a color indicator for CPS value:
/// - Green: <= 15 CPS (comfortable)
/// - Yellow: 15-25 CPS (fast)
/// - Red: > 25 CPS (too fast)
pub fn cps_color(cps: f64) -> &'static str {
    if cps <= 15.0 {
        "green"
    } else if cps <= 25.0 {
        "yellow"
    } else {
        "red"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_cps() {
        // 10 characters over 2 seconds = 5 CPS
        assert!((calculate_cps("Hello World", 2000).unwrap() - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_zero_duration() {
        assert!(calculate_cps("text", 0).is_none());
    }

    #[test]
    fn test_strip_tags() {
        assert_eq!(strip_ass_tags(r"{\b1}Hello{\b0}"), "Hello");
    }
}
