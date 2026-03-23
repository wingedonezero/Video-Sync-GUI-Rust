//! OCR Post-Processing and Text Correction
//!
//! Applies corrections from user-managed lists and dictionaries:
//!     1. Replacement rules from replacements.json
//!     2. Subtitle Edit OCR corrections
//!     3. Confidence-gated rules
//!     4. Dictionary validation (reports unknown words)
//!
//! Spell checking (enchant) is STUBBED in the Rust port.
//! All other text cleanup logic is fully ported.

use std::collections::HashMap;
use std::path::PathBuf;

use regex::Regex;
use tracing::{debug, info, warn};

use super::dictionaries::{get_dictionaries, ReplacementRule};

/// Configuration for post-processing.
#[derive(Debug, Clone)]
pub struct PostProcessConfig {
    pub cleanup_enabled: bool,
    pub low_confidence_threshold: f64,
    pub garbage_confidence_threshold: f64,
    pub enable_unambiguous_fixes: bool,
    pub enable_confidence_fixes: bool,
    pub enable_dictionary_validation: bool,
    pub enable_garbage_detection: bool,
    pub enable_subtitle_edit: bool,
    pub custom_wordlist_path: Option<PathBuf>,
}

impl Default for PostProcessConfig {
    fn default() -> Self {
        Self {
            cleanup_enabled: true,
            low_confidence_threshold: 60.0,
            garbage_confidence_threshold: 35.0,
            enable_unambiguous_fixes: true,
            enable_confidence_fixes: true,
            enable_dictionary_validation: true,
            enable_garbage_detection: true,
            enable_subtitle_edit: true,
            custom_wordlist_path: None,
        }
    }
}

/// Result of post-processing.
#[derive(Debug, Clone)]
pub struct ProcessResult {
    pub text: String,
    pub original_text: String,
    pub unknown_words: Vec<String>,
    pub fixes_applied: HashMap<String, i32>,
    pub was_modified: bool,
}

impl Default for ProcessResult {
    fn default() -> Self {
        Self {
            text: String::new(),
            original_text: String::new(),
            unknown_words: Vec::new(),
            fixes_applied: HashMap::new(),
            was_modified: false,
        }
    }
}

/// Post-processes OCR text using user-managed correction lists.
pub struct OCRPostProcessor {
    config: PostProcessConfig,
    // Compiled patterns from user-defined replacement rules
    literal_rules: Vec<ReplacementRule>,
    word_boundary_patterns: Vec<(Regex, String)>,
    word_start_patterns: Vec<(Regex, String)>,
    word_end_patterns: Vec<(Regex, String)>,
    regex_patterns: Vec<(Regex, String)>,
    confidence_gated_rules: Vec<ReplacementRule>,
    // Garbage detection patterns
    short_word_sequence: Regex,
}

impl OCRPostProcessor {
    pub fn new(config: PostProcessConfig) -> Self {
        let mut dicts = get_dictionaries(None);
        let replacement_rules = dicts.load_replacements();

        let mut literal_rules = Vec::new();
        let mut word_boundary_patterns = Vec::new();
        let mut word_start_patterns = Vec::new();
        let mut word_end_patterns = Vec::new();
        let mut regex_patterns = Vec::new();
        let mut confidence_gated_rules = Vec::new();

        for rule in &replacement_rules {
            if !rule.enabled {
                continue;
            }

            if rule.confidence_gated {
                confidence_gated_rules.push(rule.clone());
                continue;
            }

            match rule.rule_type.as_str() {
                "literal" => literal_rules.push(rule.clone()),
                "word" => {
                    if let Ok(pattern) = Regex::new(&format!(r"\b{}\b", regex::escape(&rule.pattern))) {
                        word_boundary_patterns.push((pattern, rule.replacement.clone()));
                    }
                }
                "word_start" => {
                    if let Ok(pattern) = Regex::new(&format!(r"\b{}", regex::escape(&rule.pattern))) {
                        word_start_patterns.push((pattern, rule.replacement.clone()));
                    }
                }
                "word_end" => {
                    if let Ok(pattern) = Regex::new(&format!(r"{}\b", regex::escape(&rule.pattern))) {
                        word_end_patterns.push((pattern, rule.replacement.clone()));
                    }
                }
                "regex" => {
                    match Regex::new(&rule.pattern) {
                        Ok(pattern) => regex_patterns.push((pattern, rule.replacement.clone())),
                        Err(e) => warn!("Invalid regex pattern '{}': {}", rule.pattern, e),
                    }
                }
                _ => {}
            }
        }

        let short_word_sequence = Regex::new(r"(?:\b[A-Z]{1,2}\b\s*){3,}").unwrap();

        Self {
            config,
            literal_rules,
            word_boundary_patterns,
            word_start_patterns,
            word_end_patterns,
            regex_patterns,
            confidence_gated_rules,
            short_word_sequence,
        }
    }

    /// Process OCR text and apply corrections.
    pub fn process(&self, text: &str, confidence: f64, _timestamp: &str) -> ProcessResult {
        let mut result = ProcessResult {
            text: text.to_string(),
            original_text: text.to_string(),
            ..Default::default()
        };

        if text.trim().is_empty() {
            return result;
        }

        let mut current_text = text.to_string();

        if !self.config.cleanup_enabled {
            result.text = current_text;
            return result;
        }

        // Step 1: Detect and clean garbage OCR output
        if self.config.enable_garbage_detection {
            current_text = self.clean_garbage_from_text(&current_text, &mut result);
            if current_text.trim().is_empty() {
                result.text = String::new();
                result.was_modified = true;
                return result;
            }
        }

        // Step 2: Apply user replacement rules
        if self.config.enable_unambiguous_fixes {
            current_text = self.apply_replacement_rules(&current_text, &mut result);
        }

        // Step 3: Apply confidence-gated user rules
        if self.config.enable_confidence_fixes {
            current_text = self.apply_confidence_rules(&current_text, confidence, &mut result);
        }

        result.text = current_text.clone();
        result.was_modified = current_text != text;

        result
    }

    /// Apply user-defined replacement rules.
    fn apply_replacement_rules(&self, text: &str, result: &mut ProcessResult) -> String {
        let mut text = text.to_string();

        // Apply literal replacements
        for rule in &self.literal_rules {
            if text.contains(&rule.pattern) {
                let count = text.matches(&rule.pattern).count() as i32;
                text = text.replace(&rule.pattern, &rule.replacement);
                let key = format!("{}>{}", rule.pattern, rule.replacement);
                *result.fixes_applied.entry(key).or_insert(0) += count;
            }
        }

        // Apply word-boundary replacements
        for (pattern, replacement) in &self.word_boundary_patterns {
            let matches: Vec<_> = pattern.find_iter(&text).collect();
            if !matches.is_empty() {
                let count = matches.len() as i32;
                text = pattern.replace_all(&text, replacement.as_str()).to_string();
                let key = format!("word:{}", pattern.as_str());
                *result.fixes_applied.entry(key).or_insert(0) += count;
            }
        }

        // Apply word-start replacements
        for (pattern, replacement) in &self.word_start_patterns {
            let matches: Vec<_> = pattern.find_iter(&text).collect();
            if !matches.is_empty() {
                let count = matches.len() as i32;
                text = pattern.replace_all(&text, replacement.as_str()).to_string();
                let key = format!("word_start:{}", pattern.as_str());
                *result.fixes_applied.entry(key).or_insert(0) += count;
            }
        }

        // Apply word-end replacements
        for (pattern, replacement) in &self.word_end_patterns {
            let matches: Vec<_> = pattern.find_iter(&text).collect();
            if !matches.is_empty() {
                let count = matches.len() as i32;
                text = pattern.replace_all(&text, replacement.as_str()).to_string();
                let key = format!("word_end:{}", pattern.as_str());
                *result.fixes_applied.entry(key).or_insert(0) += count;
            }
        }

        // Apply regex replacements
        for (pattern, replacement) in &self.regex_patterns {
            let matches: Vec<_> = pattern.find_iter(&text).collect();
            if !matches.is_empty() {
                let count = matches.len() as i32;
                text = pattern.replace_all(&text, replacement.as_str()).to_string();
                let key = format!("regex:{}", pattern.as_str());
                *result.fixes_applied.entry(key).or_insert(0) += count;
            }
        }

        text
    }

    /// Apply confidence-gated user rules.
    fn apply_confidence_rules(&self, text: &str, confidence: f64, result: &mut ProcessResult) -> String {
        if confidence >= self.config.low_confidence_threshold {
            return text.to_string();
        }

        let mut text = text.to_string();

        for rule in &self.confidence_gated_rules {
            let new_text = match rule.rule_type.as_str() {
                "literal" => text.replace(&rule.pattern, &rule.replacement),
                "word" => {
                    if let Ok(pattern) = Regex::new(&format!(r"\b{}\b", regex::escape(&rule.pattern))) {
                        pattern.replace_all(&text, rule.replacement.as_str()).to_string()
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };

            if new_text != text {
                let key = format!("{}>{} (low conf)", rule.pattern, rule.replacement);
                *result.fixes_applied.entry(key).or_insert(0) += 1;
                text = new_text;
            }
        }

        text
    }

    /// Detect if a line is likely OCR garbage.
    fn is_garbage_line(&self, text: &str, confidence: f64) -> bool {
        let text = text.trim();
        if text.is_empty() || text.len() < 3 {
            return false;
        }

        // Very low confidence is a strong signal
        if confidence < self.config.garbage_confidence_threshold {
            let words: Vec<&str> = text.split_whitespace().collect();
            if words.len() >= 3 {
                let short_words = words.iter().filter(|w| w.len() <= 2).count();
                if short_words as f64 / words.len() as f64 > 0.6 {
                    return true;
                }
            }
        }

        // Check for sequences of short uppercase "words"
        if self.short_word_sequence.is_match(text) {
            let words: Vec<&str> = text.split_whitespace().collect();
            let uppercase_short = words.iter()
                .filter(|w| w.len() <= 2 && w.chars().all(|c| c.is_uppercase()))
                .count();
            if uppercase_short >= 3 && uppercase_short as f64 / words.len() as f64 > 0.5 {
                return true;
            }
        }

        // Check character composition
        let letters: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();
        if !letters.is_empty() {
            let uppercase_ratio = letters.iter().filter(|c| c.is_uppercase()).count() as f64 / letters.len() as f64;
            if uppercase_ratio > 0.8 {
                let space_ratio = text.chars().filter(|&c| c == ' ').count() as f64 / text.len() as f64;
                if space_ratio > 0.3 {
                    return true;
                }
            }
        }

        false
    }

    /// Remove garbage segments from multi-line text.
    fn clean_garbage_from_text(&self, text: &str, result: &mut ProcessResult) -> String {
        let lines: Vec<&str> = text.split("\\N").collect();

        if lines.len() == 1 {
            if self.is_garbage_line(text, 100.0) {
                *result.fixes_applied.entry("garbage line removed".into()).or_insert(0) += 1;
                return String::new();
            }
            return text.to_string();
        }

        let mut clean_lines = Vec::new();
        for line in &lines {
            if self.is_garbage_line(line, 100.0) {
                *result.fixes_applied.entry("garbage line removed".into()).or_insert(0) += 1;
                continue;
            }

            let cleaned = self.remove_garbage_fragments(line, result);
            if !cleaned.trim().is_empty() {
                clean_lines.push(cleaned);
            }
        }

        clean_lines.join("\\N")
    }

    /// Remove garbage fragments from within a line.
    fn remove_garbage_fragments(&self, text: &str, result: &mut ProcessResult) -> String {
        if let Some(m) = self.short_word_sequence.find(text) {
            if m.start() == 0 {
                let rest = text[m.end()..].trim();
                if !rest.is_empty() && !self.is_garbage_line(rest, 100.0) {
                    *result.fixes_applied.entry("garbage prefix removed".into()).or_insert(0) += 1;
                    return rest.to_string();
                }
            }
        }

        text.to_string()
    }
}

/// Create post-processor from settings dictionary.
pub fn create_postprocessor(settings_dict: &HashMap<String, serde_json::Value>) -> OCRPostProcessor {
    let custom_path = settings_dict.get("ocr_custom_wordlist_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let config = PostProcessConfig {
        cleanup_enabled: settings_dict.get("ocr_cleanup_enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        low_confidence_threshold: settings_dict.get("ocr_low_confidence_threshold").and_then(|v| v.as_f64()).unwrap_or(60.0),
        custom_wordlist_path: if custom_path.is_empty() { None } else { Some(PathBuf::from(custom_path)) },
        ..Default::default()
    };

    OCRPostProcessor::new(config)
}
