//! Subtitle Edit Dictionary Support
//!
//! Parses and uses Subtitle Edit's dictionary files for OCR correction:
//!     - OCRFixReplaceList.xml - Pattern-based replacements
//!     - names.xml - Valid names
//!     - NoBreakAfterList.xml - Words to keep with following word
//!     - *_se.xml - Extra valid words for spell check
//!     - WordSplitList.txt - Dictionary for splitting merged words
//!
//! These files can be downloaded from:
//! <https://github.com/SubtitleEdit/subtitleedit/tree/main/Dictionaries>

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use super::word_lists::{SpellChecker, ValidationManager};

/// A replacement rule from Subtitle Edit OCRFixReplaceList.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct SEReplacementRule {
    pub from_text: String,
    pub to_text: String,
    /// Rule type: whole_line, partial_line_always, partial_line, begin_line, end_line,
    /// whole_word, partial_word_always, partial_word, regex
    pub rule_type: String,
}

/// Configuration for which SE dictionaries are enabled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SEDictionaryConfig {
    #[serde(default = "default_true")]
    pub ocr_fix_enabled: bool,
    #[serde(default = "default_true")]
    pub names_enabled: bool,
    #[serde(default = "default_true")]
    pub no_break_enabled: bool,
    #[serde(default = "default_true")]
    pub spell_words_enabled: bool,
    #[serde(default = "default_true")]
    pub word_split_enabled: bool,
    #[serde(default = "default_true")]
    pub interjections_enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Default for SEDictionaryConfig {
    fn default() -> Self {
        Self {
            ocr_fix_enabled: true,
            names_enabled: true,
            no_break_enabled: true,
            spell_words_enabled: true,
            word_split_enabled: true,
            interjections_enabled: true,
        }
    }
}

/// Loaded Subtitle Edit dictionary data.
#[derive(Debug, Clone, Default)]
pub struct SEDictionaries {
    // OCR Fix replacements by type
    pub whole_lines: Vec<SEReplacementRule>,
    pub partial_lines_always: Vec<SEReplacementRule>,
    pub partial_lines: Vec<SEReplacementRule>,
    pub begin_lines: Vec<SEReplacementRule>,
    pub end_lines: Vec<SEReplacementRule>,
    pub whole_words: Vec<SEReplacementRule>,
    pub partial_words_always: Vec<SEReplacementRule>,
    pub partial_words: Vec<SEReplacementRule>,
    pub regex_rules: Vec<SEReplacementRule>,

    // Other dictionaries
    pub names: HashSet<String>,
    pub names_blacklist: HashSet<String>,
    pub no_break_after: HashSet<String>,
    pub spell_words: HashSet<String>,
    pub interjections: HashSet<String>,
    pub word_split_list: HashSet<String>,
}

impl SEDictionaries {
    /// Get all words that should be considered valid.
    pub fn get_all_valid_words(&self) -> HashSet<String> {
        let mut words = HashSet::new();
        // names minus blacklist
        for name in &self.names {
            if !self.names_blacklist.contains(name) {
                words.insert(name.clone());
            }
        }
        words.extend(self.spell_words.iter().cloned());
        words.extend(self.interjections.iter().cloned());
        words.extend(self.word_split_list.iter().cloned());
        words
    }

    /// Get total number of replacement rules.
    pub fn get_replacement_count(&self) -> usize {
        self.whole_lines.len()
            + self.partial_lines_always.len()
            + self.partial_lines.len()
            + self.begin_lines.len()
            + self.end_lines.len()
            + self.whole_words.len()
            + self.partial_words_always.len()
            + self.partial_words.len()
            + self.regex_rules.len()
    }
}

/// Parser for Subtitle Edit dictionary files.
pub struct SubtitleEditParser {
    se_dir: PathBuf,
}

impl SubtitleEditParser {
    /// Initialize parser with Subtitle Edit dictionaries directory.
    pub fn new(se_dir: &Path) -> Self {
        Self {
            se_dir: se_dir.to_path_buf(),
        }
    }

    /// Get available SE dictionary files by category.
    pub fn get_available_files(&self) -> std::collections::HashMap<String, Vec<PathBuf>> {
        use std::collections::HashMap;

        let mut files: HashMap<String, Vec<PathBuf>> = HashMap::new();
        files.insert("ocr_fix".into(), Vec::new());
        files.insert("names".into(), Vec::new());
        files.insert("no_break".into(), Vec::new());
        files.insert("spell_words".into(), Vec::new());
        files.insert("interjections".into(), Vec::new());
        files.insert("word_split".into(), Vec::new());

        if !self.se_dir.exists() {
            return files;
        }

        if let Ok(entries) = fs::read_dir(&self.se_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();

                if name.ends_with("_ocrfixreplacelist.xml") {
                    files.get_mut("ocr_fix").unwrap().push(path);
                } else if name.ends_with("_names.xml") || name == "names.xml" {
                    files.get_mut("names").unwrap().push(path);
                } else if name.ends_with("_nobreakafterlist.xml") {
                    files.get_mut("no_break").unwrap().push(path);
                } else if name.ends_with("_se.xml") && !name.contains("interjection") {
                    files.get_mut("spell_words").unwrap().push(path);
                } else if name.contains("interjection") && name.ends_with(".xml") {
                    files.get_mut("interjections").unwrap().push(path);
                } else if name.ends_with("_wordsplitlist.txt") {
                    files.get_mut("word_split").unwrap().push(path);
                }
            }
        }

        files
    }

    /// Parse an OCRFixReplaceList.xml file.
    pub fn parse_ocr_fix_list(&self, path: &Path) -> SEDictionaries {
        let mut result = SEDictionaries::default();

        if !path.exists() {
            warn!("OCR fix list not found: {}", path.display());
            return result;
        }

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                error!("Error loading OCR fix list {}: {}", path.display(), e);
                return result;
            }
        };

        use quick_xml::events::Event;
        use quick_xml::Reader;

        let mut reader = Reader::from_str(&content);
        let mut current_section = String::new();

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match tag.as_str() {
                        "WholeLines" | "PartialLinesAlways" | "PartialLines" | "BeginLines"
                        | "EndLines" | "WholeWords" | "PartialWordsAlways" | "PartialWords"
                        | "RegularExpressions" => {
                            current_section = tag;
                        }
                        "Line" | "LinePart" | "WordPart" | "Beginning" | "Ending" | "Word" => {
                            let mut from_text = String::new();
                            let mut to_text = String::new();

                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "from" => from_text = val,
                                    "to" => to_text = val,
                                    _ => {}
                                }
                            }

                            if !from_text.is_empty() {
                                let rule_type = match current_section.as_str() {
                                    "WholeLines" => "whole_line",
                                    "PartialLinesAlways" => "partial_line_always",
                                    "PartialLines" => "partial_line",
                                    "BeginLines" => "begin_line",
                                    "EndLines" => "end_line",
                                    "WholeWords" => "whole_word",
                                    "PartialWordsAlways" => "partial_word_always",
                                    "PartialWords" => "partial_word",
                                    _ => continue,
                                };

                                let rule = SEReplacementRule {
                                    from_text,
                                    to_text,
                                    rule_type: rule_type.to_string(),
                                };

                                match rule_type {
                                    "whole_line" => result.whole_lines.push(rule),
                                    "partial_line_always" => {
                                        result.partial_lines_always.push(rule)
                                    }
                                    "partial_line" => result.partial_lines.push(rule),
                                    "begin_line" => result.begin_lines.push(rule),
                                    "end_line" => result.end_lines.push(rule),
                                    "whole_word" => result.whole_words.push(rule),
                                    "partial_word_always" => {
                                        result.partial_words_always.push(rule)
                                    }
                                    "partial_word" => result.partial_words.push(rule),
                                    _ => {}
                                }
                            }
                        }
                        "Regex" | "RegEx" if current_section == "RegularExpressions" => {
                            let mut find_pattern = String::new();
                            let mut replace_with = String::new();

                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "find" => find_pattern = val,
                                    "replaceWith" => replace_with = val,
                                    _ => {}
                                }
                            }

                            if !find_pattern.is_empty() {
                                result.regex_rules.push(SEReplacementRule {
                                    from_text: find_pattern,
                                    to_text: replace_with,
                                    rule_type: "regex".to_string(),
                                });
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    // Handle self-closing tags like <Line from="" to="" />
                    let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match tag.as_str() {
                        "Line" | "LinePart" | "WordPart" | "Beginning" | "Ending" | "Word" => {
                            let mut from_text = String::new();
                            let mut to_text = String::new();

                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "from" => from_text = val,
                                    "to" => to_text = val,
                                    _ => {}
                                }
                            }

                            if !from_text.is_empty() {
                                let rule_type = match current_section.as_str() {
                                    "WholeLines" => "whole_line",
                                    "PartialLinesAlways" => "partial_line_always",
                                    "PartialLines" => "partial_line",
                                    "BeginLines" => "begin_line",
                                    "EndLines" => "end_line",
                                    "WholeWords" => "whole_word",
                                    "PartialWordsAlways" => "partial_word_always",
                                    "PartialWords" => "partial_word",
                                    _ => continue,
                                };

                                let rule = SEReplacementRule {
                                    from_text,
                                    to_text,
                                    rule_type: rule_type.to_string(),
                                };

                                match rule_type {
                                    "whole_line" => result.whole_lines.push(rule),
                                    "partial_line_always" => {
                                        result.partial_lines_always.push(rule)
                                    }
                                    "partial_line" => result.partial_lines.push(rule),
                                    "begin_line" => result.begin_lines.push(rule),
                                    "end_line" => result.end_lines.push(rule),
                                    "whole_word" => result.whole_words.push(rule),
                                    "partial_word_always" => {
                                        result.partial_words_always.push(rule)
                                    }
                                    "partial_word" => result.partial_words.push(rule),
                                    _ => {}
                                }
                            }
                        }
                        "Regex" | "RegEx" if current_section == "RegularExpressions" => {
                            let mut find_pattern = String::new();
                            let mut replace_with = String::new();

                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "find" => find_pattern = val,
                                    "replaceWith" => replace_with = val,
                                    _ => {}
                                }
                            }

                            if !find_pattern.is_empty() {
                                result.regex_rules.push(SEReplacementRule {
                                    from_text: find_pattern,
                                    to_text: replace_with,
                                    rule_type: "regex".to_string(),
                                });
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::End(ref e)) => {
                    let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match tag.as_str() {
                        "WholeLines" | "PartialLinesAlways" | "PartialLines" | "BeginLines"
                        | "EndLines" | "WholeWords" | "PartialWordsAlways" | "PartialWords"
                        | "RegularExpressions" => {
                            current_section.clear();
                        }
                        _ => {}
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    error!("XML parse error in {}: {}", path.display(), e);
                    break;
                }
                _ => {}
            }
        }

        info!(
            "Loaded OCR fix list: {} rules from {}",
            result.get_replacement_count(),
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        debug!(
            "  Breakdown: whole_lines={}, partial_lines_always={}, \
             partial_lines={}, begin_lines={}, end_lines={}, \
             whole_words={}, partial_words_always={}, partial_words={}, regex={}",
            result.whole_lines.len(),
            result.partial_lines_always.len(),
            result.partial_lines.len(),
            result.begin_lines.len(),
            result.end_lines.len(),
            result.whole_words.len(),
            result.partial_words_always.len(),
            result.partial_words.len(),
            result.regex_rules.len(),
        );

        result
    }

    /// Parse a names XML file.
    ///
    /// Returns (names_set, blacklist_set).
    pub fn parse_names_xml(&self, path: &Path) -> (HashSet<String>, HashSet<String>) {
        let mut names = HashSet::new();
        let mut blacklist = HashSet::new();

        if !path.exists() {
            warn!("Names file not found: {}", path.display());
            return (names, blacklist);
        }

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                error!("Error loading names file {}: {}", path.display(), e);
                return (names, blacklist);
            }
        };

        use quick_xml::events::Event;
        use quick_xml::Reader;

        let mut reader = Reader::from_str(&content);
        let mut in_blacklist = false;
        let mut in_name = false;
        let mut current_text = String::new();

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => match e.name().as_ref() {
                    b"blacklist" => in_blacklist = true,
                    b"name" => {
                        in_name = true;
                        current_text.clear();
                    }
                    _ => {}
                },
                Ok(Event::Text(ref e)) => {
                    if in_name {
                        let decoded = e.decode().unwrap_or_default();
                        if let Ok(text) = quick_xml::escape::unescape(&decoded) {
                            current_text.push_str(&text);
                        }
                    }
                }
                Ok(Event::End(ref e)) => match e.name().as_ref() {
                    b"blacklist" => in_blacklist = false,
                    b"name" => {
                        if in_name {
                            let name = current_text.trim().to_string();
                            if !name.is_empty() {
                                if in_blacklist {
                                    blacklist.insert(name);
                                } else {
                                    names.insert(name);
                                }
                            }
                            in_name = false;
                        }
                    }
                    _ => {}
                },
                Ok(Event::Eof) => break,
                Err(e) => {
                    error!("XML parse error in {}: {}", path.display(), e);
                    break;
                }
                _ => {}
            }
        }

        info!(
            "Loaded {} names, {} blacklisted from {}",
            names.len(),
            blacklist.len(),
            path.file_name().unwrap_or_default().to_string_lossy()
        );

        (names, blacklist)
    }

    /// Parse a NoBreakAfterList.xml file.
    pub fn parse_no_break_list(&self, path: &Path) -> HashSet<String> {
        let mut items = HashSet::new();

        if !path.exists() {
            warn!("No break list not found: {}", path.display());
            return items;
        }

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                error!("Error loading no break list {}: {}", path.display(), e);
                return items;
            }
        };

        use quick_xml::events::Event;
        use quick_xml::Reader;

        let mut reader = Reader::from_str(&content);
        let mut in_item = false;
        let mut current_text = String::new();

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    if e.name().as_ref() == b"Item" {
                        in_item = true;
                        current_text.clear();
                    }
                }
                Ok(Event::Text(ref e)) => {
                    if in_item {
                        let decoded = e.decode().unwrap_or_default();
                        if let Ok(text) = quick_xml::escape::unescape(&decoded) {
                            current_text.push_str(&text);
                        }
                    }
                }
                Ok(Event::End(ref e)) => {
                    if e.name().as_ref() == b"Item" && in_item {
                        let item = current_text.trim().to_string();
                        if !item.is_empty() {
                            items.insert(item);
                        }
                        in_item = false;
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    error!("XML parse error in {}: {}", path.display(), e);
                    break;
                }
                _ => {}
            }
        }

        info!(
            "Loaded {} no-break items from {}",
            items.len(),
            path.file_name().unwrap_or_default().to_string_lossy()
        );

        items
    }

    /// Parse a spell check words XML file (*_se.xml).
    pub fn parse_spell_words_xml(&self, path: &Path) -> HashSet<String> {
        let mut words = HashSet::new();

        if !path.exists() {
            warn!("Spell words file not found: {}", path.display());
            return words;
        }

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                error!("Error loading spell words file {}: {}", path.display(), e);
                return words;
            }
        };

        use quick_xml::events::Event;
        use quick_xml::Reader;

        let mut reader = Reader::from_str(&content);
        let mut in_word = false;
        let mut current_text = String::new();

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    if e.name().as_ref() == b"word" {
                        in_word = true;
                        current_text.clear();
                    }
                }
                Ok(Event::Text(ref e)) => {
                    if in_word {
                        let decoded = e.decode().unwrap_or_default();
                        if let Ok(text) = quick_xml::escape::unescape(&decoded) {
                            current_text.push_str(&text);
                        }
                    }
                }
                Ok(Event::End(ref e)) => {
                    if e.name().as_ref() == b"word" && in_word {
                        let word = current_text.trim().to_string();
                        if !word.is_empty() {
                            words.insert(word);
                        }
                        in_word = false;
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    error!("XML parse error in {}: {}", path.display(), e);
                    break;
                }
                _ => {}
            }
        }

        info!(
            "Loaded {} spell check words from {}",
            words.len(),
            path.file_name().unwrap_or_default().to_string_lossy()
        );

        words
    }

    /// Parse a WordSplitList.txt file.
    pub fn parse_word_split_list(&self, path: &Path) -> HashSet<String> {
        let mut words = HashSet::new();

        if !path.exists() {
            warn!("Word split list not found: {}", path.display());
            return words;
        }

        match fs::read_to_string(path) {
            Ok(content) => {
                for line in content.lines() {
                    let word = line.trim();
                    if !word.is_empty() && !word.starts_with('#') {
                        words.insert(word.to_string());
                    }
                }
                info!(
                    "Loaded {} words for word splitting from {}",
                    words.len(),
                    path.file_name().unwrap_or_default().to_string_lossy()
                );
            }
            Err(e) => {
                error!("Error loading word split list {}: {}", path.display(), e);
            }
        }

        words
    }

    /// Load all available SE dictionary files.
    pub fn load_all(&self, config: Option<&SEDictionaryConfig>) -> SEDictionaries {
        let config = config.cloned().unwrap_or_default();
        let mut result = SEDictionaries::default();
        let available = self.get_available_files();

        // Load OCR fix lists
        if config.ocr_fix_enabled {
            for path in available.get("ocr_fix").unwrap_or(&Vec::new()) {
                let ocr_data = self.parse_ocr_fix_list(path);
                result.whole_lines.extend(ocr_data.whole_lines);
                result.partial_lines_always.extend(ocr_data.partial_lines_always);
                result.partial_lines.extend(ocr_data.partial_lines);
                result.begin_lines.extend(ocr_data.begin_lines);
                result.end_lines.extend(ocr_data.end_lines);
                result.whole_words.extend(ocr_data.whole_words);
                result.partial_words_always.extend(ocr_data.partial_words_always);
                result.partial_words.extend(ocr_data.partial_words);
                result.regex_rules.extend(ocr_data.regex_rules);
            }
        }

        // Load names
        if config.names_enabled {
            for path in available.get("names").unwrap_or(&Vec::new()) {
                let (names, blacklist) = self.parse_names_xml(path);
                result.names.extend(names);
                result.names_blacklist.extend(blacklist);
            }
        }

        // Load no break after list
        if config.no_break_enabled {
            for path in available.get("no_break").unwrap_or(&Vec::new()) {
                let no_break = self.parse_no_break_list(path);
                result.no_break_after.extend(no_break);
            }
        }

        // Load spell check words
        if config.spell_words_enabled {
            for path in available.get("spell_words").unwrap_or(&Vec::new()) {
                let words = self.parse_spell_words_xml(path);
                result.spell_words.extend(words);
            }
        }

        // Load interjections
        if config.interjections_enabled {
            for path in available.get("interjections").unwrap_or(&Vec::new()) {
                let words = self.parse_spell_words_xml(path); // Same format
                result.interjections.extend(words);
            }
        }

        // Load word split list
        if config.word_split_enabled {
            for path in available.get("word_split").unwrap_or(&Vec::new()) {
                let words = self.parse_word_split_list(path);
                result.word_split_list.extend(words);
            }
        }

        result
    }
}

/// Splits merged words using a dictionary of valid words.
///
/// When OCR produces "thisis" or "Idon't", this class can split
/// them into "this is" or "I don't".
pub struct WordSplitter {
    valid_words: HashSet<String>,
    max_word_len: usize,
}

impl WordSplitter {
    /// Create a new word splitter with the given valid words.
    pub fn new(valid_words: HashSet<String>) -> Self {
        let lower_words: HashSet<String> = valid_words.iter().map(|w| w.to_lowercase()).collect();
        let max_word_len = lower_words.iter().map(|w| w.len()).max().unwrap_or(20);
        Self {
            valid_words: lower_words,
            max_word_len,
        }
    }

    /// Check if word is in the valid words set.
    pub fn is_valid_word(&self, word: &str) -> bool {
        self.valid_words.contains(&word.to_lowercase())
    }

    /// Try to split a merged word into two valid words.
    pub fn try_split(&self, text: &str) -> Option<String> {
        if text.is_empty() || text.len() < 3 {
            return None;
        }

        let text_lower = text.to_lowercase();

        // Try splitting at each position
        for i in 1..text.len() {
            // Ensure we're at a char boundary
            if !text_lower.is_char_boundary(i) {
                continue;
            }
            let left = &text_lower[..i];
            let right = &text_lower[i..];

            if self.is_valid_word(left) && self.is_valid_word(right) {
                // Preserve original casing for left part
                let left_result = &text[..i];
                let right_result = &text[i..];
                return Some(format!("{} {}", left_result, right_result));
            }
        }

        None
    }

    /// Split merged words in text.
    pub fn split_merged_words(
        &self,
        text: &str,
        dictionary: Option<&dyn SpellChecker>,
        validator: Option<&ValidationManager>,
    ) -> String {
        // Process line by line to preserve newlines
        let lines: Vec<&str> = text.split('\n').collect();
        let mut result_lines = Vec::new();

        for line in lines {
            let words: Vec<&str> = line.split_whitespace().collect();
            let mut result_words = Vec::new();

            for word in words {
                // Skip if word is already valid in spell checker
                if let Some(dict) = dictionary {
                    if dict.check(word) {
                        result_words.push(word.to_string());
                        continue;
                    }
                }

                // Skip if word is known in validator
                if let Some(val) = validator {
                    if val.is_known_word(word).is_known {
                        result_words.push(word.to_string());
                        continue;
                    }
                }

                // Skip if word is in our valid words (split list)
                if self.is_valid_word(word) {
                    result_words.push(word.to_string());
                    continue;
                }

                // Try to split
                if let Some(split) = self.try_split(word) {
                    // Validate that split parts are actually valid words
                    let parts: Vec<&str> = split.split_whitespace().collect();
                    let mut all_valid = true;
                    for part in &parts {
                        if let Some(dict) = dictionary {
                            if dict.check(part) {
                                continue;
                            }
                        }
                        if let Some(val) = validator {
                            if val.is_known_word(part).is_known {
                                continue;
                            }
                        }
                        all_valid = false;
                        break;
                    }

                    if all_valid {
                        result_words.push(split);
                    } else {
                        result_words.push(word.to_string());
                    }
                } else {
                    result_words.push(word.to_string());
                }
            }

            result_lines.push(result_words.join(" "));
        }

        result_lines.join("\n")
    }
}

/// Applies Subtitle Edit OCR corrections to text.
///
/// Uses the rules loaded from SE dictionary files to fix OCR errors.
/// Follows Subtitle Edit's logic: only fix words that are NOT in the dictionary,
/// and only accept fixes that produce valid dictionary words.
pub struct SubtitleEditCorrector {
    dicts: SEDictionaries,
    /// Spell checker (stubbed - enchant not available).
    spell_checker: Option<Box<dyn SpellChecker>>,
    /// Optional ValidationManager for unified validation.
    validation_manager: Option<Box<ValidationManager>>,
    word_splitter: Option<WordSplitter>,
    /// Compiled regex patterns from SE rules.
    compiled_regex: Vec<(Regex, String)>,
    /// Lookup map for whole word replacements.
    whole_word_map: std::collections::HashMap<String, String>,
}

impl SubtitleEditCorrector {
    /// Create a new corrector with loaded dictionaries.
    pub fn new(
        se_dicts: SEDictionaries,
        spell_checker: Option<Box<dyn SpellChecker>>,
        validation_manager: Option<Box<ValidationManager>>,
    ) -> Self {
        let word_splitter = if !se_dicts.word_split_list.is_empty() {
            Some(WordSplitter::new(se_dicts.word_split_list.clone()))
        } else {
            None
        };

        // Compile regex patterns
        let mut compiled_regex = Vec::new();
        let mut unicode_pattern_count = 0;
        for rule in &se_dicts.regex_rules {
            match Regex::new(&rule.from_text) {
                Ok(pattern) => {
                    compiled_regex.push((pattern, rule.to_text.clone()));
                }
                Err(e) => {
                    if rule.from_text.contains("\\p{") || rule.from_text.contains(r"\p{") {
                        unicode_pattern_count += 1;
                    } else {
                        warn!("Invalid SE regex pattern '{}': {}", rule.from_text, e);
                    }
                }
            }
        }

        if unicode_pattern_count > 0 {
            info!(
                "Skipped {} SE regex patterns with Unicode properties (not supported by regex crate without unicode-perl feature)",
                unicode_pattern_count
            );
        }

        // Build lookup for whole word replacements
        let whole_word_map: std::collections::HashMap<String, String> = se_dicts
            .whole_words
            .iter()
            .map(|r| (r.from_text.clone(), r.to_text.clone()))
            .collect();

        Self {
            dicts: se_dicts,
            spell_checker,
            validation_manager,
            word_splitter,
            compiled_regex,
            whole_word_map,
        }
    }

    /// Check if a word is protected from being fixed.
    fn is_word_protected(&self, word: &str) -> bool {
        if word.is_empty() || word.len() <= 1 {
            return true;
        }

        let clean_word = word.trim_matches(|c: char| ".,!?;:\"'()-".contains(c));
        if clean_word.is_empty() {
            return true;
        }

        if let Some(ref vm) = self.validation_manager {
            return vm.is_protected_word(clean_word);
        }

        self.is_word_valid_fallback(clean_word)
    }

    /// Check if a word is a valid result for a fix.
    fn is_valid_fix_result(&self, word: &str) -> bool {
        if word.is_empty() {
            return false;
        }

        let clean_word = word.trim_matches(|c: char| ".,!?;:\"'()-".contains(c));
        if clean_word.is_empty() {
            return false;
        }

        if let Some(ref vm) = self.validation_manager {
            return vm.is_valid_fix_result(clean_word);
        }

        self.is_word_valid_fallback(clean_word)
    }

    /// Check if a word is valid (in spell checker or SE dictionaries).
    fn is_word_valid(&self, word: &str) -> bool {
        if word.is_empty() || word.len() <= 1 {
            return true;
        }

        let clean_word = word.trim_matches(|c: char| ".,!?;:\"'()-".contains(c));
        if clean_word.is_empty() {
            return true;
        }

        if let Some(ref vm) = self.validation_manager {
            return vm.is_known_word(clean_word).is_known;
        }

        self.is_word_valid_fallback(clean_word)
    }

    /// Fallback validation using spell checker + SE dictionaries only.
    fn is_word_valid_fallback(&self, clean_word: &str) -> bool {
        // Check spell checker (stubbed)
        if let Some(ref sc) = self.spell_checker {
            if sc.check(clean_word) {
                return true;
            }
            if sc.check(&clean_word.to_lowercase()) {
                return true;
            }
            // Capitalize first letter
            let mut chars = clean_word.chars();
            if let Some(first) = chars.next() {
                let capitalized = format!("{}{}", first.to_uppercase(), chars.as_str());
                if sc.check(&capitalized) {
                    return true;
                }
            }
        }

        // Check SE valid words (names, spell_words, etc.)
        let all_valid = self.dicts.get_all_valid_words();
        let lower_valid: HashSet<String> = all_valid.iter().map(|w| w.to_lowercase()).collect();
        all_valid.contains(clean_word) || lower_valid.contains(&clean_word.to_lowercase())
    }

    /// Try to fix a single word using SE rules.
    fn try_fix_word(&self, word: &str) -> (String, Option<String>) {
        // Try whole word replacement on the ORIGINAL word (including punctuation)
        if let Some(replacement) = self.whole_word_map.get(word) {
            if self.is_valid_fix_result(replacement) {
                return (
                    replacement.clone(),
                    Some(format!("whole_word: {} -> {}", word, replacement)),
                );
            }
        }

        // Strip punctuation but remember it
        let mut prefix = String::new();
        let mut suffix = String::new();
        let mut clean_word = word.to_string();

        // Extract leading punctuation
        while !clean_word.is_empty()
            && clean_word.starts_with(|c: char| !c.is_alphanumeric())
        {
            if let Some(c) = clean_word.chars().next() {
                prefix.push(c);
                clean_word = clean_word[c.len_utf8()..].to_string();
            }
        }

        // Extract trailing punctuation
        while !clean_word.is_empty()
            && clean_word.ends_with(|c: char| !c.is_alphanumeric())
        {
            if let Some(c) = clean_word.chars().last() {
                suffix.insert(0, c);
                clean_word = clean_word[..clean_word.len() - c.len_utf8()].to_string();
            }
        }

        if clean_word.is_empty() {
            return (word.to_string(), None);
        }

        // If word is protected, don't fix it
        if self.is_word_protected(&clean_word) {
            return (word.to_string(), None);
        }

        // Try whole word replacement on cleaned word
        if let Some(replacement) = self.whole_word_map.get(&clean_word) {
            if self.is_valid_fix_result(replacement) {
                return (
                    format!("{}{}{}", prefix, replacement, suffix),
                    Some(format!("whole_word: {} -> {}", clean_word, replacement)),
                );
            }
        }

        // Try partial word fixes
        for rule in &self.dicts.partial_words {
            if clean_word.contains(&rule.from_text) {
                let new_word = clean_word.replace(&rule.from_text, &rule.to_text);
                if self.is_valid_fix_result(&new_word) {
                    return (
                        format!("{}{}{}", prefix, new_word, suffix),
                        Some(format!(
                            "partial_word: {} -> {}",
                            rule.from_text, rule.to_text
                        )),
                    );
                }
            }
        }

        (word.to_string(), None)
    }

    /// Apply all SE corrections to text.
    ///
    /// Returns (corrected_text, list_of_applied_fixes, list_of_unknown_words).
    pub fn apply_corrections(&self, text: &str) -> (String, Vec<String>, Vec<String>) {
        let mut text = text.to_string();
        let mut fixes_applied = Vec::new();
        let mut unknown_words = Vec::new();

        // === LINE-LEVEL FIXES (safe patterns, always apply) ===

        // 1. Whole line replacements
        for rule in &self.dicts.whole_lines {
            if text.trim() == rule.from_text {
                text = rule.to_text.clone();
                fixes_applied.push(format!("whole_line: {} -> {}", rule.from_text, rule.to_text));
                break;
            }
        }

        // 2. Begin line replacements
        for rule in &self.dicts.begin_lines {
            if text.starts_with(&rule.from_text) {
                text = format!("{}{}", rule.to_text, &text[rule.from_text.len()..]);
                fixes_applied.push(format!(
                    "begin_line: {} -> {}",
                    rule.from_text, rule.to_text
                ));
            }
        }

        // 3. End line replacements
        for rule in &self.dicts.end_lines {
            if text.ends_with(&rule.from_text) {
                text = format!(
                    "{}{}",
                    &text[..text.len() - rule.from_text.len()],
                    rule.to_text
                );
                fixes_applied.push(format!("end_line: {} -> {}", rule.from_text, rule.to_text));
            }
        }

        // 4. Partial lines always
        for rule in &self.dicts.partial_lines_always {
            if text.contains(&rule.from_text) {
                text = text.replace(&rule.from_text, &rule.to_text);
                fixes_applied.push(format!(
                    "partial_always: {} -> {}",
                    rule.from_text, rule.to_text
                ));
            }
        }

        // 5. Partial words always
        for rule in &self.dicts.partial_words_always {
            if text.contains(&rule.from_text) {
                text = text.replace(&rule.from_text, &rule.to_text);
                fixes_applied.push(format!(
                    "partial_word_always: {} -> {}",
                    rule.from_text, rule.to_text
                ));
            }
        }

        // 6. Regex replacements
        for (pattern, replacement) in &self.compiled_regex {
            if pattern.is_match(&text) {
                text = pattern.replace_all(&text, replacement.as_str()).to_string();
                fixes_applied.push(format!("regex: {}", pattern.as_str()));
            }
        }

        // === WORD-LEVEL FIXES (require spell check validation) ===

        if self.spell_checker.is_some() {
            // Split into words, preserving spacing
            let word_re = Regex::new(r"\S+|\s+").unwrap();
            let words: Vec<String> = word_re
                .find_iter(&text)
                .map(|m| m.as_str().to_string())
                .collect();
            let mut result_words = Vec::new();

            for word in &words {
                if word.chars().all(|c| c.is_whitespace()) {
                    result_words.push(word.clone());
                    continue;
                }

                let (fixed_word, fix_desc) = self.try_fix_word(word);

                if let Some(desc) = fix_desc {
                    fixes_applied.push(desc);
                    result_words.push(fixed_word);
                } else {
                    result_words.push(word.clone());
                    // Track unknown words
                    let clean = word.trim_matches(|c: char| ".,!?;:\"'()-".contains(c));
                    if !clean.is_empty()
                        && clean.len() > 1
                        && !self.is_word_valid(clean)
                        && !unknown_words.contains(&clean.to_string())
                    {
                        unknown_words.push(clean.to_string());
                    }
                }
            }

            text = result_words.join("");

            // Word splitting
            if let Some(ref splitter) = self.word_splitter {
                let new_text = splitter.split_merged_words(
                    &text,
                    self.spell_checker.as_deref(),
                    self.validation_manager.as_deref(),
                );
                if new_text != text {
                    fixes_applied.push("word_split".to_string());
                    text = new_text;
                }
            }
        }

        (text, fixes_applied, unknown_words)
    }

    /// Check if text seems to contain valid words.
    pub fn seems_valid(&self, text: &str) -> bool {
        let sc = match &self.spell_checker {
            Some(sc) => sc,
            None => return true,
        };

        let word_re = Regex::new(r"\b[a-zA-Z]+\b").unwrap();
        let words: Vec<&str> = word_re.find_iter(text).map(|m| m.as_str()).collect();
        if words.is_empty() {
            return true;
        }

        let valid_count = words.iter().filter(|w| sc.check(w)).count();
        valid_count >= words.len() / 2
    }

    /// Check if word is in the names list (and not blacklisted).
    pub fn is_valid_name(&self, word: &str) -> bool {
        if self.dicts.names_blacklist.contains(word) {
            return false;
        }
        self.dicts.names.contains(word)
    }

    /// Check if word should not have a line break after it.
    pub fn is_no_break_word(&self, word: &str) -> bool {
        self.dicts.no_break_after.contains(word)
    }

    /// Check if word is in any of the valid word lists.
    pub fn is_valid_word_in_lists(&self, word: &str) -> bool {
        let all_valid = self.dicts.get_all_valid_words();
        let lower_valid: HashSet<String> = all_valid.iter().map(|w| w.to_lowercase()).collect();
        all_valid.contains(word) || lower_valid.contains(&word.to_lowercase())
    }
}

/// Configuration storage filename.
pub const SE_CONFIG_FILE: &str = "subtitle_edit_config.json";

/// Load Subtitle Edit configuration from file.
pub fn load_se_config(config_dir: &Path) -> SEDictionaryConfig {
    let config_path = config_dir.join(SE_CONFIG_FILE);

    if config_path.exists() {
        match fs::read_to_string(&config_path) {
            Ok(content) => match serde_json::from_str::<SEDictionaryConfig>(&content) {
                Ok(config) => return config,
                Err(e) => {
                    error!("Error loading SE config: {}", e);
                }
            },
            Err(e) => {
                error!("Error reading SE config file: {}", e);
            }
        }
    }

    SEDictionaryConfig::default()
}

/// Save Subtitle Edit configuration to file.
pub fn save_se_config(config_dir: &Path, config: &SEDictionaryConfig) -> bool {
    let config_path = config_dir.join(SE_CONFIG_FILE);

    match serde_json::to_string_pretty(config) {
        Ok(json_str) => match fs::write(&config_path, json_str) {
            Ok(()) => true,
            Err(e) => {
                error!("Error saving SE config: {}", e);
                false
            }
        },
        Err(e) => {
            error!("Error serializing SE config: {}", e);
            false
        }
    }
}
