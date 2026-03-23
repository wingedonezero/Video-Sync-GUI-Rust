//! Attachment extraction — 1:1 port of `vsg_core/extraction/attachments.py`.
//!
//! Extracts font attachments from MKV files using mkvextract.

use std::collections::HashMap;
use std::path::Path;

use crate::io::runner::CommandRunner;

use super::tracks::get_stream_info;

/// Font-related MIME type prefixes and exact matches for detection.
const FONT_MIME_PREFIXES: &[&str] = &["font/", "application/font", "application/x-font"];
const FONT_MIME_EXACT: &[&str] = &[
    "application/x-truetype-font",
    "application/truetype",
    "font/ttf",
    "application/vnd.ms-opentype",
    "application/opentype",
    "font/otf",
    "application/font-woff",
    "font/woff",
    "font/woff2",
    "application/postscript",
    "application/x-font-type1",
];
const FONT_MIME_KEYWORDS: &[&str] = &["font", "truetype", "opentype"];
const FONT_EXTENSIONS: &[&str] = &[
    ".ttf", ".otf", ".ttc", ".woff", ".woff2", ".eot", ".fon", ".fnt", ".pfb", ".pfa",
];
/// Extensions for generic binary MIME types that are actually fonts.
const FONT_EXTENSIONS_BINARY: &[&str] = &[".ttf", ".otf", ".ttc", ".woff", ".woff2"];

/// Check if an attachment is a font file — comprehensive detection covering all common cases.
fn is_font_attachment(mime_type: &str, file_name: &str) -> bool {
    let mime_lower = mime_type.to_lowercase();
    let name_lower = file_name.to_lowercase();

    // Standard font MIME type prefixes
    if FONT_MIME_PREFIXES
        .iter()
        .any(|prefix| mime_lower.starts_with(prefix))
    {
        return true;
    }

    // Exact MIME type matches (TrueType, OpenType, WOFF, PostScript)
    if FONT_MIME_EXACT.iter().any(|m| mime_lower == *m) {
        return true;
    }

    // Generic binary MIME with font file extension
    if matches!(
        mime_lower.as_str(),
        "application/octet-stream" | "binary/octet-stream"
    ) && FONT_EXTENSIONS_BINARY
        .iter()
        .any(|ext| name_lower.ends_with(ext))
    {
        return true;
    }

    // Any MIME containing font/truetype/opentype keywords
    if FONT_MIME_KEYWORDS
        .iter()
        .any(|kw| mime_lower.contains(kw))
    {
        return true;
    }

    // File extension fallback (most reliable)
    if FONT_EXTENSIONS
        .iter()
        .any(|ext| name_lower.ends_with(ext))
    {
        return true;
    }

    false
}

/// Extract font attachments from MKV — `extract_attachments`
pub fn extract_attachments(
    mkv: &str,
    temp_dir: &Path,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
    role: &str,
) -> Vec<String> {
    let info = match get_stream_info(mkv, runner, tool_paths) {
        Some(i) => i,
        None => return Vec::new(),
    };

    let empty_attachments = vec![];
    let attachments = info
        .get("attachments")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty_attachments);

    let total_attachments = attachments.len();
    let mut files: Vec<String> = Vec::new();
    let mut specs: Vec<String> = Vec::new();
    let mut font_count = 0;

    for attachment in attachments {
        let mime_type = attachment
            .get("content_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let file_name = attachment
            .get("file_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let att_id = attachment
            .get("id")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        if is_font_attachment(mime_type, file_name) {
            font_count += 1;
            let out_path = temp_dir.join(format!("{role}_att_{att_id}_{file_name}"));
            specs.push(format!("{att_id}:{}", out_path.display()));
            files.push(out_path.to_string_lossy().to_string());
        }
    }

    if !specs.is_empty() {
        runner.log_message(&format!(
            "[Attachments] Found {total_attachments} attachments, extracting {font_count} font file(s)..."
        ));

        let mut mkvextract_args: Vec<&str> = vec!["mkvextract", mkv, "attachments"];
        let spec_refs: Vec<&str> = specs.iter().map(|s| s.as_str()).collect();
        mkvextract_args.extend(&spec_refs);
        runner.run(&mkvextract_args, tool_paths);
    } else {
        runner.log_message(&format!(
            "[Attachments] Found {total_attachments} attachments, but none were identified as fonts."
        ));
    }

    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_detection_mime_types() {
        assert!(is_font_attachment("font/ttf", "test.ttf"));
        assert!(is_font_attachment("application/x-truetype-font", "arial.ttf"));
        assert!(is_font_attachment("application/vnd.ms-opentype", "font.otf"));
        assert!(is_font_attachment("application/font-woff", "font.woff"));
    }

    #[test]
    fn font_detection_binary_mime_with_extension() {
        assert!(is_font_attachment("application/octet-stream", "arial.ttf"));
        assert!(is_font_attachment("binary/octet-stream", "font.otf"));
        // Not a font extension with binary mime
        assert!(!is_font_attachment("application/octet-stream", "data.bin"));
    }

    #[test]
    fn font_detection_extension_fallback() {
        assert!(is_font_attachment("", "myfont.ttf"));
        assert!(is_font_attachment("", "myfont.otf"));
        assert!(is_font_attachment("", "myfont.woff2"));
        assert!(is_font_attachment("", "myfont.pfb"));
        assert!(!is_font_attachment("", "image.png"));
    }

    #[test]
    fn font_detection_mime_keywords() {
        assert!(is_font_attachment("application/x-font-whatever", "test"));
        assert!(is_font_attachment("something-truetype-something", "test"));
    }
}
