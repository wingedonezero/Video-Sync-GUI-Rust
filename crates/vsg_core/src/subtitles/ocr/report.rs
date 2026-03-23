//! OCR Report Generation
//!
//! Generates detailed reports about OCR results:
//!     - Unknown words with context and suggestions
//!     - Low confidence lines flagged for review
//!     - Applied fixes summary
//!     - Overall accuracy metrics

use std::collections::HashMap;
use std::path::Path;

use serde::{Serialize, Deserialize};

/// Information about an unknown word found during OCR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnknownWord {
    pub word: String,
    pub context: String,
    pub timestamp: String,
    pub confidence: f64,
    pub occurrences: i32,
    pub suggestions: Vec<String>,
}

/// A subtitle line with low OCR confidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LowConfidenceLine {
    pub text: String,
    pub timestamp: String,
    pub confidence: f64,
    pub subtitle_index: usize,
    pub potential_issues: Vec<String>,
}

/// OCR result for a single subtitle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleOCRResult {
    pub index: usize,
    pub timestamp_start: String,
    pub timestamp_end: String,
    pub text: String,
    pub confidence: f64,
    pub was_modified: bool,
    pub fixes_applied: HashMap<String, i32>,
    pub unknown_words: Vec<String>,
    pub position_x: u32,
    pub position_y: u32,
    pub is_positioned: bool,
    pub line_regions: Vec<String>,
}

impl Default for SubtitleOCRResult {
    fn default() -> Self {
        Self {
            index: 0,
            timestamp_start: String::new(),
            timestamp_end: String::new(),
            text: String::new(),
            confidence: 0.0,
            was_modified: false,
            fixes_applied: HashMap::new(),
            unknown_words: Vec::new(),
            position_x: 0,
            position_y: 0,
            is_positioned: false,
            line_regions: Vec::new(),
        }
    }
}

/// Complete OCR report for a subtitle file.
pub struct OCRReport {
    pub source_file: String,
    pub output_file: String,
    pub timestamp: String,
    pub language: String,
    pub duration_seconds: f64,

    pub total_subtitles: usize,
    pub successful_subtitles: usize,
    pub failed_subtitles: usize,
    pub average_confidence: f64,
    pub min_confidence: f64,
    pub max_confidence: f64,

    pub subtitles: Vec<SubtitleOCRResult>,
    pub unknown_words: Vec<UnknownWord>,
    pub low_confidence_lines: Vec<LowConfidenceLine>,

    pub total_fixes_applied: i32,
    pub fixes_by_type: HashMap<String, i32>,

    pub positioned_subtitles: usize,
    pub top_positioned: usize,
    pub split_subtitles: usize,
}

impl Default for OCRReport {
    fn default() -> Self {
        Self {
            source_file: String::new(),
            output_file: String::new(),
            timestamp: String::new(),
            language: "eng".to_string(),
            duration_seconds: 0.0,
            total_subtitles: 0,
            successful_subtitles: 0,
            failed_subtitles: 0,
            average_confidence: 0.0,
            min_confidence: 100.0,
            max_confidence: 0.0,
            subtitles: Vec::new(),
            unknown_words: Vec::new(),
            low_confidence_lines: Vec::new(),
            total_fixes_applied: 0,
            fixes_by_type: HashMap::new(),
            positioned_subtitles: 0,
            top_positioned: 0,
            split_subtitles: 0,
        }
    }
}

impl OCRReport {
    /// Add a subtitle result and update statistics.
    pub fn add_subtitle_result(&mut self, result: SubtitleOCRResult) {
        self.total_subtitles += 1;

        if !result.text.trim().is_empty() {
            self.successful_subtitles += 1;
        } else {
            self.failed_subtitles += 1;
        }

        if result.confidence > 0.0 {
            self.min_confidence = self.min_confidence.min(result.confidence);
            self.max_confidence = self.max_confidence.max(result.confidence);
        }

        for (fix_type, count) in &result.fixes_applied {
            *self.fixes_by_type.entry(fix_type.clone()).or_insert(0) += count;
            self.total_fixes_applied += count;
        }

        if result.is_positioned {
            self.positioned_subtitles += 1;
        }
        if !result.line_regions.is_empty() {
            let has_top = result.line_regions.iter().any(|r| r == "top");
            if has_top {
                self.top_positioned += 1;
            }
            let regions: std::collections::HashSet<_> = result.line_regions.iter().collect();
            if regions.len() > 1 {
                self.split_subtitles += 1;
            }
        }

        self.subtitles.push(result);
    }

    /// Add or update an unknown word entry.
    pub fn add_unknown_word(&mut self, word: &str, context: &str, timestamp: &str, confidence: f64) {
        for existing in &mut self.unknown_words {
            if existing.word == word {
                existing.occurrences += 1;
                return;
            }
        }

        self.unknown_words.push(UnknownWord {
            word: word.to_string(),
            context: context.to_string(),
            timestamp: timestamp.to_string(),
            confidence,
            occurrences: 1,
            suggestions: Vec::new(), // Spell checking stubbed
        });
    }

    /// Flag a line for manual review.
    pub fn add_low_confidence_line(
        &mut self,
        text: &str,
        timestamp: &str,
        confidence: f64,
        subtitle_index: usize,
        potential_issues: Vec<String>,
    ) {
        self.low_confidence_lines.push(LowConfidenceLine {
            text: text.to_string(),
            timestamp: timestamp.to_string(),
            confidence,
            subtitle_index,
            potential_issues,
        });
    }

    /// Calculate final statistics.
    pub fn finalize(&mut self) {
        if self.successful_subtitles > 0 {
            let total_conf: f64 = self.subtitles.iter()
                .filter(|s| s.confidence > 0.0)
                .map(|s| s.confidence)
                .sum();
            let conf_count = self.subtitles.iter()
                .filter(|s| s.confidence > 0.0)
                .count();
            if conf_count > 0 {
                self.average_confidence = total_conf / conf_count as f64;
            }
        }

        self.unknown_words.sort_by(|a, b| b.occurrences.cmp(&a.occurrences));
        self.low_confidence_lines.sort_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap_or(std::cmp::Ordering::Equal));
    }

    /// Convert to summary dict for inclusion in main job report.
    pub fn to_summary(&self) -> HashMap<String, serde_json::Value> {
        let mut m = HashMap::new();
        m.insert("total_subtitles".into(), serde_json::json!(self.total_subtitles));
        m.insert("successful".into(), serde_json::json!(self.successful_subtitles));
        m.insert("failed".into(), serde_json::json!(self.failed_subtitles));
        m.insert("average_confidence".into(), serde_json::json!((self.average_confidence * 100.0).round() / 100.0));
        m.insert("total_fixes".into(), serde_json::json!(self.total_fixes_applied));
        m.insert("unknown_word_count".into(), serde_json::json!(self.unknown_words.len()));
        m.insert("low_confidence_count".into(), serde_json::json!(self.low_confidence_lines.len()));
        m.insert("positioned_subtitles".into(), serde_json::json!(self.positioned_subtitles));
        m
    }

    /// Save report to JSON file.
    pub fn save(&self, output_path: &Path) {
        let data = serde_json::json!({
            "metadata": {
                "source_file": self.source_file,
                "output_file": self.output_file,
                "timestamp": self.timestamp,
                "language": self.language,
                "duration_seconds": self.duration_seconds,
            },
            "statistics": {
                "total_subtitles": self.total_subtitles,
                "successful_subtitles": self.successful_subtitles,
                "failed_subtitles": self.failed_subtitles,
                "average_confidence": (self.average_confidence * 100.0).round() / 100.0,
                "min_confidence": (self.min_confidence * 100.0).round() / 100.0,
                "max_confidence": (self.max_confidence * 100.0).round() / 100.0,
                "total_fixes_applied": self.total_fixes_applied,
                "positioned_subtitles": self.positioned_subtitles,
            },
            "fixes_by_type": self.fixes_by_type,
        });

        if let Ok(json_str) = serde_json::to_string_pretty(&data) {
            let _ = std::fs::write(output_path, json_str);
        }
    }
}

/// Create a new OCR report.
pub fn create_report(source_file: &str, output_file: &str, language: &str) -> OCRReport {
    OCRReport {
        source_file: source_file.to_string(),
        output_file: output_file.to_string(),
        timestamp: chrono::Local::now().to_rfc3339(),
        language: language.to_string(),
        ..Default::default()
    }
}
