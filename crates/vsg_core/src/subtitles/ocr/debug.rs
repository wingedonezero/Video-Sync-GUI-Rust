//! OCR Debug Output
//!
//! Creates organized debug output for analyzing OCR issues:
//! - Preprocessed images saved by issue type
//! - Simple text files with timecodes and OCR output
//! - Easy to share and analyze specific problems

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use image::RgbaImage;

/// Debug info for a single subtitle.
struct DebugSubtitle {
    index: usize,
    start_time: String,
    end_time: String,
    raw_text: String,
    confidence: f64,
    image: Option<RgbaImage>,
    raw_ocr_text: Option<String>,
    unknown_words: Vec<String>,
    fixes_applied: HashMap<String, String>,
    original_text: Option<String>,
}

/// Collects and outputs debug information for OCR analysis.
pub struct OCRDebugger {
    logs_dir: PathBuf,
    base_name: String,
    timestamp: String,
    enabled: bool,
    low_confidence_threshold: f64,
    subtitles: HashMap<usize, DebugSubtitle>,
    unknown_word_indices: HashSet<usize>,
    fix_indices: HashSet<usize>,
    low_confidence_indices: HashSet<usize>,
}

impl OCRDebugger {
    pub fn new(
        logs_dir: PathBuf,
        base_name: String,
        timestamp: String,
        enabled: bool,
        low_confidence_threshold: f64,
    ) -> Self {
        Self {
            logs_dir,
            base_name,
            timestamp,
            enabled,
            low_confidence_threshold,
            subtitles: HashMap::new(),
            unknown_word_indices: HashSet::new(),
            fix_indices: HashSet::new(),
            low_confidence_indices: HashSet::new(),
        }
    }

    fn debug_dir(&self) -> PathBuf {
        self.logs_dir.join(format!("{}_ocr_debug_{}", self.base_name, self.timestamp))
    }

    /// Add a subtitle for potential debug output.
    pub fn add_subtitle(
        &mut self,
        index: usize,
        start_time: &str,
        end_time: &str,
        text: &str,
        confidence: f64,
        image: Option<&RgbaImage>,
        raw_ocr_text: Option<&str>,
    ) {
        if !self.enabled {
            return;
        }

        self.subtitles.insert(index, DebugSubtitle {
            index,
            start_time: start_time.to_string(),
            end_time: end_time.to_string(),
            raw_text: text.to_string(),
            confidence,
            image: image.cloned(),
            raw_ocr_text: raw_ocr_text.map(|s| s.to_string()),
            unknown_words: Vec::new(),
            fixes_applied: HashMap::new(),
            original_text: None,
        });

        if confidence < self.low_confidence_threshold {
            self.low_confidence_indices.insert(index);
        }
    }

    /// Record an unknown word for a subtitle.
    pub fn add_unknown_word(&mut self, index: usize, word: &str) {
        if !self.enabled {
            return;
        }
        if let Some(sub) = self.subtitles.get_mut(&index) {
            sub.unknown_words.push(word.to_string());
            self.unknown_word_indices.insert(index);
        }
    }

    /// Record a fix applied to a subtitle.
    pub fn add_fix(&mut self, index: usize, fix_name: &str, description: &str, original_text: Option<&str>) {
        if !self.enabled {
            return;
        }
        if let Some(sub) = self.subtitles.get_mut(&index) {
            sub.fixes_applied.insert(fix_name.to_string(), description.to_string());
            if let Some(orig) = original_text {
                sub.original_text = Some(orig.to_string());
            }
            self.fix_indices.insert(index);
        }
    }

    /// Save all debug output to disk.
    pub fn save(&self) {
        if !self.enabled || self.subtitles.is_empty() {
            return;
        }

        let debug_dir = self.debug_dir();
        let _ = std::fs::create_dir_all(&debug_dir);

        self.save_summary(&debug_dir);
        self.save_raw_ocr(&debug_dir);
        self.save_all_subtitles(&debug_dir);

        if !self.unknown_word_indices.is_empty() {
            self.save_unknown_words(&debug_dir);
        }
        if !self.fix_indices.is_empty() {
            self.save_fixes(&debug_dir);
        }
        if !self.low_confidence_indices.is_empty() {
            self.save_low_confidence(&debug_dir);
        }
    }

    fn save_summary(&self, debug_dir: &Path) {
        let summary_path = debug_dir.join("summary.txt");
        let content = format!(
            "OCR Debug Summary\n{}\nBase name: {}\nTimestamp: {}\nTotal subtitles: {}\n\n\
             Issues found:\n  Unknown words: {} subtitles\n  Fixes applied: {} subtitles\n  \
             Low confidence: {} subtitles\n",
            "=".repeat(50),
            self.base_name,
            self.timestamp,
            self.subtitles.len(),
            self.unknown_word_indices.len(),
            self.fix_indices.len(),
            self.low_confidence_indices.len(),
        );
        let _ = std::fs::write(summary_path, content);
    }

    fn save_raw_ocr(&self, debug_dir: &Path) {
        let path = debug_dir.join("raw_ocr.txt");
        let mut lines = vec![
            "Raw OCR Output (Unedited)".to_string(),
            "=".repeat(50),
            String::new(),
            "Format: [index] timecode | confidence% | raw text".to_string(),
            "=".repeat(50),
            String::new(),
        ];

        let mut indices: Vec<usize> = self.subtitles.keys().copied().collect();
        indices.sort();

        for idx in &indices {
            if let Some(sub) = self.subtitles.get(idx) {
                let raw_text = sub.raw_ocr_text.as_deref()
                    .or(sub.original_text.as_deref())
                    .unwrap_or(&sub.raw_text);
                let text_display = raw_text.replace('\n', "\\n");
                lines.push(format!(
                    "[{:04}] {} -> {} | {:5.1}% | {}",
                    sub.index, sub.start_time, sub.end_time, sub.confidence, text_display
                ));
            }
        }

        let _ = std::fs::write(path, lines.join("\n"));
    }

    fn save_all_subtitles(&self, debug_dir: &Path) {
        let folder = debug_dir.join("all_subtitles");
        let _ = std::fs::create_dir_all(&folder);

        let mut lines = vec![
            "All Subtitles - Raw Verification".to_string(),
            "=".repeat(50),
            String::new(),
        ];

        let mut indices: Vec<usize> = self.subtitles.keys().copied().collect();
        indices.sort();

        for idx in &indices {
            if let Some(sub) = self.subtitles.get(idx) {
                let raw_text = sub.raw_ocr_text.as_deref()
                    .or(sub.original_text.as_deref())
                    .unwrap_or(&sub.raw_text);

                lines.push("-".repeat(50));
                lines.push(format!("Index: {}", sub.index));
                lines.push(format!("Time: {} -> {}", sub.start_time, sub.end_time));
                lines.push(format!("Confidence: {:.1}%", sub.confidence));
                lines.push(format!("Image: sub_{:04}.png", sub.index));
                lines.push(String::new());
                lines.push(format!("Raw OCR:\n  {}", raw_text.replace('\n', "\n  ")));
                lines.push(String::new());

                if let Some(ref image) = sub.image {
                    let _ = image.save(folder.join(format!("sub_{:04}.png", sub.index)));
                }
            }
        }

        let _ = std::fs::write(folder.join("all_subtitles.txt"), lines.join("\n"));
    }

    fn save_unknown_words(&self, debug_dir: &Path) {
        let folder = debug_dir.join("unknown_words");
        let _ = std::fs::create_dir_all(&folder);

        let mut lines = vec![
            "Unknown Words Debug".to_string(),
            "=".repeat(50),
            String::new(),
        ];

        let mut indices: Vec<usize> = self.unknown_word_indices.iter().copied().collect();
        indices.sort();

        for idx in &indices {
            if let Some(sub) = self.subtitles.get(idx) {
                lines.push("-".repeat(50));
                lines.push(format!("Index: {}", sub.index));
                lines.push(format!("Time: {} -> {}", sub.start_time, sub.end_time));
                lines.push(format!("Confidence: {:.1}%", sub.confidence));
                lines.push(format!("OCR Text:\n  {}", sub.raw_text.replace('\n', "\n  ")));
                lines.push(format!("Unknown words: {}", sub.unknown_words.join(", ")));
                lines.push(String::new());

                if let Some(ref image) = sub.image {
                    let _ = image.save(folder.join(format!("sub_{:04}.png", sub.index)));
                }
            }
        }

        let _ = std::fs::write(folder.join("unknown_words.txt"), lines.join("\n"));
    }

    fn save_fixes(&self, debug_dir: &Path) {
        let folder = debug_dir.join("fixes_applied");
        let _ = std::fs::create_dir_all(&folder);

        let mut lines = vec![
            "Fixes Applied Debug".to_string(),
            "=".repeat(50),
            String::new(),
        ];

        let mut indices: Vec<usize> = self.fix_indices.iter().copied().collect();
        indices.sort();

        for idx in &indices {
            if let Some(sub) = self.subtitles.get(idx) {
                lines.push("-".repeat(50));
                lines.push(format!("Index: {}", sub.index));
                lines.push(format!("Time: {} -> {}", sub.start_time, sub.end_time));
                lines.push(format!("Confidence: {:.1}%", sub.confidence));

                if let Some(ref orig) = sub.original_text {
                    lines.push(format!("Original OCR:\n  {}", orig.replace('\n', "\n  ")));
                }
                lines.push(format!("After fixes:\n  {}", sub.raw_text.replace('\n', "\n  ")));
                lines.push("Fixes applied:".to_string());
                for (name, desc) in &sub.fixes_applied {
                    lines.push(format!("  - {}: {}", name, desc));
                }
                lines.push(String::new());

                if let Some(ref image) = sub.image {
                    let _ = image.save(folder.join(format!("sub_{:04}.png", sub.index)));
                }
            }
        }

        let _ = std::fs::write(folder.join("fixes_applied.txt"), lines.join("\n"));
    }

    fn save_low_confidence(&self, debug_dir: &Path) {
        let folder = debug_dir.join("low_confidence");
        let _ = std::fs::create_dir_all(&folder);

        let mut lines = vec![
            "Low Confidence Debug".to_string(),
            "=".repeat(50),
            String::new(),
            format!("Subtitles with confidence below {}%.", self.low_confidence_threshold),
            String::new(),
        ];

        let mut indices: Vec<usize> = self.low_confidence_indices.iter().copied().collect();
        indices.sort_by(|a, b| {
            let ca = self.subtitles.get(a).map(|s| s.confidence).unwrap_or(100.0);
            let cb = self.subtitles.get(b).map(|s| s.confidence).unwrap_or(100.0);
            ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
        });

        for idx in &indices {
            if let Some(sub) = self.subtitles.get(idx) {
                lines.push("-".repeat(50));
                lines.push(format!("Index: {}", sub.index));
                lines.push(format!("Time: {} -> {}", sub.start_time, sub.end_time));
                lines.push(format!("Confidence: {:.1}%", sub.confidence));
                lines.push(format!("OCR Text:\n  {}", sub.raw_text.replace('\n', "\n  ")));
                lines.push(String::new());

                if let Some(ref image) = sub.image {
                    let _ = image.save(folder.join(format!("sub_{:04}.png", sub.index)));
                }
            }
        }

        let _ = std::fs::write(folder.join("low_confidence.txt"), lines.join("\n"));
    }
}

/// Create a debugger from settings.
pub fn create_debugger(
    logs_dir: &Path,
    base_name: &str,
    timestamp: &str,
    settings_dict: &std::collections::HashMap<String, serde_json::Value>,
) -> OCRDebugger {
    OCRDebugger::new(
        logs_dir.to_path_buf(),
        base_name.to_string(),
        timestamp.to_string(),
        settings_dict.get("ocr_debug_output").and_then(|v| v.as_bool()).unwrap_or(false),
        settings_dict.get("ocr_low_confidence_threshold").and_then(|v| v.as_f64()).unwrap_or(60.0),
    )
}
