//! Time formatting — 1:1 port of `vsg_qt/subtitle_editor/utils/time_format.py`.
//!
//! Converts between time representations used in subtitle editing.

/// Format milliseconds to ASS timestamp: "H:MM:SS.cc" (centiseconds).
pub fn ms_to_ass_timestamp(ms: i64) -> String {
    let total_cs = ms / 10;
    let cs = total_cs % 100;
    let total_seconds = ms / 1000;
    let seconds = total_seconds % 60;
    let total_minutes = total_seconds / 60;
    let minutes = total_minutes % 60;
    let hours = total_minutes / 60;
    format!("{hours}:{minutes:02}:{seconds:02}.{cs:02}")
}

/// Format milliseconds to SRT timestamp: "HH:MM:SS,mmm".
pub fn ms_to_srt_timestamp(ms: i64) -> String {
    let millis = ms % 1000;
    let total_seconds = ms / 1000;
    let seconds = total_seconds % 60;
    let total_minutes = total_seconds / 60;
    let minutes = total_minutes % 60;
    let hours = total_minutes / 60;
    format!("{hours:02}:{minutes:02}:{seconds:02},{millis:03}")
}

/// Parse an ASS timestamp "H:MM:SS.cc" to milliseconds.
pub fn ass_timestamp_to_ms(timestamp: &str) -> Option<i64> {
    let parts: Vec<&str> = timestamp.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let hours: i64 = parts[0].parse().ok()?;
    let minutes: i64 = parts[1].parse().ok()?;
    let sec_parts: Vec<&str> = parts[2].split('.').collect();
    if sec_parts.len() != 2 {
        return None;
    }
    let seconds: i64 = sec_parts[0].parse().ok()?;
    let centiseconds: i64 = sec_parts[1].parse().ok()?;
    Some(hours * 3_600_000 + minutes * 60_000 + seconds * 1_000 + centiseconds * 10)
}

/// Format seconds to display string "MM:SS".
pub fn seconds_to_display(seconds: f64) -> String {
    let total_secs = seconds as i64;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{mins:02}:{secs:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ms_to_ass() {
        assert_eq!(ms_to_ass_timestamp(0), "0:00:00.00");
        assert_eq!(ms_to_ass_timestamp(1_234_560), "0:20:34.56");
    }

    #[test]
    fn test_ms_to_srt() {
        assert_eq!(ms_to_srt_timestamp(0), "00:00:00,000");
        assert_eq!(ms_to_srt_timestamp(1_234_567), "00:20:34,567");
    }

    #[test]
    fn test_ass_to_ms() {
        assert_eq!(ass_timestamp_to_ms("0:00:00.00"), Some(0));
        assert_eq!(ass_timestamp_to_ms("0:20:34.56"), Some(1_234_560));
    }
}
