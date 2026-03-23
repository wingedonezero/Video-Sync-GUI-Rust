//! Unified Word List Management System
//!
//! Provides a central system for managing word lists used in OCR validation:
//! - User dictionary, names, romaji, SE dictionaries all unified
//! - Configurable behavior per list (validate, protect, accept fixes)
//! - Reorderable priority
//! - Single ValidationManager used by all OCR components
//!
//! Config stored in .config/ocr/ocr_config.json

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};

/// Source type for word lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WordListSource {
    /// User-created lists (editable).
    User,
    /// From SubtitleEdit files (read-only, overridable).
    #[serde(rename = "se")]
    SubtitleEdit,
    /// Generated/built lists like romaji.
    Built,
    /// System spell checker (Enchant/Hunspell).
    System,
}

impl WordListSource {
    pub fn as_str(&self) -> &str {
        match self {
            Self::User => "user",
            Self::SubtitleEdit => "se",
            Self::Built => "built",
            Self::System => "system",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "user" => Self::User,
            "se" => Self::SubtitleEdit,
            "built" => Self::Built,
            "system" => Self::System,
            _ => Self::User,
        }
    }
}

/// Configuration for a word list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordListConfig {
    /// Display name.
    pub name: String,
    /// WordListSource value.
    pub source: String,
    /// Whether this list is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,

    // Behavior flags
    /// Words won't show as "unknown".
    #[serde(default = "default_true")]
    pub validates_known: bool,
    /// Won't try to "fix" these words.
    #[serde(default = "default_true")]
    pub protects_from_fix: bool,
    /// Accept fixes that produce these words.
    #[serde(default = "default_true")]
    pub accepts_as_fix_result: bool,

    /// For ordering (lower = higher priority).
    #[serde(default = "default_order")]
    pub order: i32,

    /// File reference (for file-backed lists).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_order() -> i32 {
    100
}

impl WordListConfig {
    pub fn new(name: &str, source: &str) -> Self {
        Self {
            name: name.to_string(),
            source: source.to_string(),
            enabled: true,
            validates_known: true,
            protects_from_fix: true,
            accepts_as_fix_result: true,
            order: 100,
            file_path: None,
        }
    }

    /// Builder-style setters.
    pub fn with_order(mut self, order: i32) -> Self {
        self.order = order;
        self
    }

    pub fn with_file_path(mut self, path: &str) -> Self {
        self.file_path = Some(path.to_string());
        self
    }

    pub fn with_validates_known(mut self, v: bool) -> Self {
        self.validates_known = v;
        self
    }

    pub fn with_protects_from_fix(mut self, v: bool) -> Self {
        self.protects_from_fix = v;
        self
    }

    pub fn with_accepts_as_fix_result(mut self, v: bool) -> Self {
        self.accepts_as_fix_result = v;
        self
    }
}

/// A word list with its configuration and loaded words.
#[derive(Debug, Clone)]
pub struct WordList {
    pub config: WordListConfig,
    pub words: HashSet<String>,
}

impl WordList {
    pub fn new(config: WordListConfig, words: HashSet<String>) -> Self {
        Self { config, words }
    }

    pub fn name(&self) -> &str {
        &self.config.name
    }

    pub fn enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn word_count(&self) -> usize {
        self.words.len()
    }

    /// Check if word is in this list (case-insensitive).
    pub fn contains(&self, word: &str) -> bool {
        self.words.contains(&word.to_lowercase()) || self.words.contains(word)
    }
}

/// Default word list configurations.
pub fn default_word_lists() -> Vec<WordListConfig> {
    vec![
        // System dictionary (Enchant/Hunspell) - highest priority
        WordListConfig::new("System Dictionary", "system").with_order(0),
        // User lists
        WordListConfig::new("User Dictionary", "user")
            .with_order(10)
            .with_file_path("user_dictionary.txt"),
        WordListConfig::new("Names", "user")
            .with_order(20)
            .with_file_path("names.txt"),
        // SE lists
        WordListConfig::new("SE Spell Words", "se")
            .with_order(30)
            .with_file_path("subtitleedit/en_US_se.xml"),
        WordListConfig::new("SE Names", "se")
            .with_order(40)
            .with_file_path("subtitleedit/en_names.xml"),
        WordListConfig::new("SE Interjections", "se")
            .with_order(50)
            .with_file_path("subtitleedit/en_interjections_se.xml"),
        // Romaji - validates but doesn't accept as fix result
        WordListConfig::new("Romaji", "built")
            .with_order(60)
            .with_file_path("romaji_dictionary.txt")
            .with_accepts_as_fix_result(false),
        // Word split list - only used for splitting, not validation
        WordListConfig::new("SE Word Split", "se")
            .with_order(70)
            .with_file_path("subtitleedit/eng_WordSplitList.txt")
            .with_validates_known(false)
            .with_protects_from_fix(false)
            .with_accepts_as_fix_result(false),
    ]
}

/// Result of a word validation check.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_known: bool,
    /// Which list it was found in.
    pub source_name: Option<String>,
    /// Should not be "fixed".
    pub is_protected: bool,
}

impl ValidationResult {
    pub fn unknown() -> Self {
        Self {
            is_known: false,
            source_name: None,
            is_protected: false,
        }
    }

    pub fn known(source_name: &str, is_protected: bool) -> Self {
        Self {
            is_known: true,
            source_name: Some(source_name.to_string()),
            is_protected,
        }
    }
}

/// Statistics for logging.
#[derive(Debug, Clone, Default)]
pub struct ValidationStats {
    pub total_validated: usize,
    pub by_source: HashMap<String, usize>,
    pub unknown_words: Vec<String>,
}

impl ValidationStats {
    pub fn add_validated(&mut self, source_name: &str) {
        self.total_validated += 1;
        *self.by_source.entry(source_name.to_string()).or_insert(0) += 1;
    }

    pub fn add_unknown(&mut self, word: &str) {
        if !self.unknown_words.contains(&word.to_string()) {
            self.unknown_words.push(word.to_string());
        }
    }

    /// Get summary string for logging.
    pub fn get_summary(&self) -> String {
        if self.by_source.is_empty() {
            return "0 words validated".to_string();
        }

        let mut parts: Vec<String> = self
            .by_source
            .iter()
            .map(|(source, count)| format!("{} {}", count, source))
            .collect();
        parts.sort();

        let mut summary = format!(
            "{} words validated ({})",
            self.total_validated,
            parts.join(", ")
        );

        if !self.unknown_words.is_empty() {
            let mut unknown_preview: Vec<String> =
                self.unknown_words.iter().take(5).cloned().collect();
            if self.unknown_words.len() > 5 {
                unknown_preview.push(format!("...+{} more", self.unknown_words.len() - 5));
            }
            summary.push_str(&format!(
                ", {} unknown: {}",
                self.unknown_words.len(),
                unknown_preview.join(", ")
            ));
        }

        summary
    }
}

/// Trait for spell checking backends.
///
/// In the Python version, this uses `enchant.Dict`. In Rust, we use the
/// `enchant` crate which wraps the same libenchant C library.
pub trait SpellChecker: Send + Sync {
    /// Check if a word is correctly spelled.
    fn check(&self, word: &str) -> bool;
}

/// Enchant-based spell checker — wraps libenchant (same as Python's pyenchant).
///
/// Supports multiple backends (hunspell, aspell, etc.) via enchant's provider system.
pub struct EnchantSpellChecker {
    dict: enchant::Dict,
}

// SAFETY: enchant::Dict is not Send+Sync by default, but we only use it
// from a single thread in practice (OCR processing is sequential per track).
unsafe impl Send for EnchantSpellChecker {}
unsafe impl Sync for EnchantSpellChecker {}

impl EnchantSpellChecker {
    /// Create a new enchant spell checker for the given language.
    ///
    /// Returns None if the language dictionary is not available.
    pub fn new(lang: &str) -> Option<Self> {
        let mut broker = enchant::Broker::new();
        if !broker.dict_exists(lang) {
            return None;
        }
        match broker.request_dict(lang) {
            Ok(dict) => Some(Self { dict }),
            Err(_) => None,
        }
    }

    /// Check if a dictionary exists for the given language code.
    pub fn dict_exists(lang: &str) -> bool {
        let mut broker = enchant::Broker::new();
        broker.dict_exists(lang)
    }
}

impl SpellChecker for EnchantSpellChecker {
    fn check(&self, word: &str) -> bool {
        self.dict.check(word).unwrap_or(false)
    }
}

/// Central manager for word validation across all OCR components.
///
/// Provides a single source of truth for:
/// - Is a word "known" (shouldn't be flagged as unknown)?
/// - Is a word "protected" (shouldn't be auto-fixed)?
/// - Is a word a valid fix result (acceptable correction target)?
pub struct ValidationManager {
    pub config_dir: PathBuf,
    config_path: PathBuf,
    pub word_lists: Vec<WordList>,
    /// System spell checker (enchant-based, same as Python's pyenchant).
    pub spell_checker: Option<Box<dyn SpellChecker>>,
    /// Additional language dictionaries for protection only (not fix validation).
    pub protection_checkers: HashMap<String, Box<dyn SpellChecker>>,
    stats: ValidationStats,
}

impl ValidationManager {
    /// Create a new ValidationManager.
    pub fn new(config_dir: &Path) -> Self {
        let config_dir = config_dir.to_path_buf();
        let config_path = config_dir.join("ocr_config.json");

        Self {
            config_dir,
            config_path,
            word_lists: Vec::new(),
            spell_checker: None,
            protection_checkers: HashMap::new(),
            stats: ValidationStats::default(),
        }
    }

    /// Set the system spell checker.
    pub fn set_spell_checker(&mut self, spell_checker: Box<dyn SpellChecker>) {
        self.spell_checker = Some(spell_checker);
    }

    /// Load additional language dictionaries for word protection.
    ///
    /// Words found in any of these languages are considered "known" and
    /// protected from auto-fixing, but are NOT valid fix targets.
    /// Uses enchant (libenchant) for dictionary access — same as Python's pyenchant.
    pub fn init_protection_languages(&mut self) {
        // Languages commonly found in anime subtitles
        let languages = [
            ("en_US", "English (US)"),
            ("en_GB", "English (UK)"),
            ("ja", "Japanese"),
            ("fr_FR", "French"),
            ("de_DE", "German"),
            ("es_ES", "Spanish"),
            ("pt_BR", "Portuguese"),
            ("it_IT", "Italian"),
            ("ko_KR", "Korean"),
            ("zh_CN", "Chinese"),
        ];

        let mut loaded = Vec::new();
        for (lang_code, lang_name) in &languages {
            if EnchantSpellChecker::dict_exists(lang_code) {
                if let Some(checker) = EnchantSpellChecker::new(lang_code) {
                    self.protection_checkers.insert(
                        lang_code.to_string(),
                        Box::new(checker),
                    );
                    loaded.push(format!("{lang_name} ({lang_code})"));
                }
            }
        }

        if !loaded.is_empty() {
            info!("[WordLists] Loaded {} protection languages: {}", loaded.len(), loaded.join(", "));
        } else {
            debug!("[WordLists] No enchant dictionaries found for protection languages");
        }
    }

    /// Check if a word exists in any protection language dictionary.
    fn check_protection_languages(&self, word: &str) -> bool {
        for checker in self.protection_checkers.values() {
            if checker.check(word) || checker.check(&word.to_lowercase()) {
                return true;
            }
        }
        false
    }

    /// Load word list configurations from JSON.
    pub fn load_config(&self) -> Vec<WordListConfig> {
        if !self.config_path.exists() {
            info!("[WordLists] No config found, using defaults");
            return default_word_lists();
        }

        match fs::read_to_string(&self.config_path) {
            Ok(content) => {
                match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(data) => {
                        let mut configs = Vec::new();
                        if let Some(lists) = data.get("word_lists").and_then(|v| v.as_array()) {
                            for item in lists {
                                if let Ok(config) =
                                    serde_json::from_value::<WordListConfig>(item.clone())
                                {
                                    configs.push(config);
                                }
                            }
                        }
                        info!("[WordLists] Loaded {} word list configs", configs.len());
                        configs
                    }
                    Err(e) => {
                        error!("[WordLists] Error loading config: {}", e);
                        default_word_lists()
                    }
                }
            }
            Err(e) => {
                error!("[WordLists] Error loading config: {}", e);
                default_word_lists()
            }
        }
    }

    /// Save word list configurations to JSON.
    pub fn save_config(&self) {
        if let Err(e) = fs::create_dir_all(&self.config_dir) {
            error!("[WordLists] Error creating config dir: {}", e);
            return;
        }

        let configs: Vec<serde_json::Value> = self
            .word_lists
            .iter()
            .map(|wl| serde_json::to_value(&wl.config).unwrap_or_default())
            .collect();

        let data = serde_json::json!({ "word_lists": configs });

        match serde_json::to_string_pretty(&data) {
            Ok(json_str) => {
                if let Err(e) = fs::write(&self.config_path, json_str) {
                    error!("[WordLists] Error saving config: {}", e);
                } else {
                    info!("[WordLists] Saved config to {}", self.config_path.display());
                }
            }
            Err(e) => {
                error!("[WordLists] Error serializing config: {}", e);
            }
        }
    }

    /// Get all word lists sorted by order.
    pub fn get_word_lists(&self) -> Vec<&WordList> {
        let mut lists: Vec<&WordList> = self.word_lists.iter().collect();
        lists.sort_by_key(|wl| wl.config.order);
        lists
    }

    /// Add a word list.
    pub fn add_word_list(&mut self, config: WordListConfig, words: HashSet<String>) {
        debug!(
            "[WordLists] Added '{}' with {} words",
            config.name,
            words.len()
        );
        self.word_lists.push(WordList::new(config, words));
    }

    /// Get a word list by name.
    pub fn get_word_list_by_name(&self, name: &str) -> Option<&WordList> {
        self.word_lists.iter().find(|wl| wl.name() == name)
    }

    /// Get a mutable word list by name.
    pub fn get_word_list_by_name_mut(&mut self, name: &str) -> Option<&mut WordList> {
        self.word_lists.iter_mut().find(|wl| wl.name() == name)
    }

    /// Change the order of a word list.
    pub fn reorder_word_list(&mut self, name: &str, new_order: i32) {
        if let Some(wl) = self.get_word_list_by_name_mut(name) {
            wl.config.order = new_order;
            self.save_config();
        }
    }

    /// Update configuration for a word list.
    pub fn update_word_list_config(&mut self, name: &str, updates: &HashMap<String, serde_json::Value>) {
        if let Some(wl) = self.get_word_list_by_name_mut(name) {
            if let Some(v) = updates.get("enabled").and_then(|v| v.as_bool()) {
                wl.config.enabled = v;
            }
            if let Some(v) = updates.get("validates_known").and_then(|v| v.as_bool()) {
                wl.config.validates_known = v;
            }
            if let Some(v) = updates.get("protects_from_fix").and_then(|v| v.as_bool()) {
                wl.config.protects_from_fix = v;
            }
            if let Some(v) = updates.get("accepts_as_fix_result").and_then(|v| v.as_bool()) {
                wl.config.accepts_as_fix_result = v;
            }
            if let Some(v) = updates.get("order").and_then(|v| v.as_i64()) {
                wl.config.order = v as i32;
            }
            self.save_config();
        }
    }

    // =========================================================================
    // Validation Methods - Used by all OCR components
    // =========================================================================

    /// Check if a word is "known" (shouldn't be flagged as unknown).
    ///
    /// Checks all enabled word lists with `validates_known=true`,
    /// in priority order.
    pub fn is_known_word(&self, word: &str) -> ValidationResult {
        self.is_known_word_inner(word, false)
    }

    /// Check if a word is "known", optionally tracking stats.
    pub fn is_known_word_tracked(&mut self, word: &str) -> ValidationResult {
        // We need to do this without borrowing self mutably during iteration
        let result = self.is_known_word_inner(word, false);

        if result.is_known {
            if let Some(ref source) = result.source_name {
                self.stats.add_validated(source);
            }
        } else {
            self.stats.add_unknown(word);
        }

        result
    }

    fn is_known_word_inner(&self, word: &str, _track_stats: bool) -> ValidationResult {
        let word_lower = word.to_lowercase();

        // Check system spell checker first (if available and enabled)
        if let Some(system_list) = self.get_word_list_by_name("System Dictionary") {
            if system_list.enabled()
                && system_list.config.validates_known
                && self.spell_checker.is_some()
            {
                let sc = self.spell_checker.as_ref().unwrap();
                if sc.check(word) || sc.check(&word_lower) {
                    return ValidationResult::known(
                        "System Dictionary",
                        system_list.config.protects_from_fix,
                    );
                }
            }
        }

        // Check word lists in order
        for wl in self.get_word_lists() {
            if !wl.enabled() || !wl.config.validates_known {
                continue;
            }
            if wl.config.source == "system" {
                continue; // Already checked above
            }

            if wl.contains(word) {
                return ValidationResult::known(wl.name(), wl.config.protects_from_fix);
            }
        }

        // Check additional protection languages
        if self.check_protection_languages(word) {
            return ValidationResult::known("Foreign Language", true);
        }

        // Check hyphenated compounds (e.g., "Onee-chan", "Nee-san")
        if word.contains('-') {
            let parts: Vec<&str> = word.split('-').collect();
            if parts.len() >= 2 && parts.iter().all(|p| !p.is_empty()) {
                let results: Vec<ValidationResult> =
                    parts.iter().map(|p| self.is_known_word_inner(p, false)).collect();
                if results.iter().all(|r| r.is_known) {
                    let is_protected = results.iter().any(|r| r.is_protected);
                    return ValidationResult::known("Compound", is_protected);
                }
            }
        }

        // Not found
        ValidationResult::unknown()
    }

    /// Check if a word is "protected" (shouldn't be auto-fixed).
    pub fn is_protected_word(&self, word: &str) -> bool {
        let word_lower = word.to_lowercase();

        // Check system spell checker
        if let Some(system_list) = self.get_word_list_by_name("System Dictionary") {
            if system_list.enabled()
                && system_list.config.protects_from_fix
                && self.spell_checker.is_some()
            {
                let sc = self.spell_checker.as_ref().unwrap();
                if sc.check(word) || sc.check(&word_lower) {
                    return true;
                }
            }
        }

        // Check word lists
        for wl in self.get_word_lists() {
            if !wl.enabled() || !wl.config.protects_from_fix {
                continue;
            }
            if wl.config.source == "system" {
                continue;
            }
            if wl.contains(word) {
                return true;
            }
        }

        // Check additional protection languages
        if self.check_protection_languages(word) {
            return true;
        }

        // Check hyphenated compounds
        if word.contains('-') {
            let parts: Vec<&str> = word.split('-').collect();
            if parts.len() >= 2 && parts.iter().all(|p| !p.is_empty() && self.is_protected_word(p))
            {
                return true;
            }
        }

        false
    }

    /// Check if a word is a valid result for an OCR fix.
    ///
    /// More restrictive than `is_known_word` - e.g., romaji words are "known"
    /// but not valid fix results.
    pub fn is_valid_fix_result(&self, word: &str) -> bool {
        let word_lower = word.to_lowercase();

        // Check system spell checker
        if let Some(system_list) = self.get_word_list_by_name("System Dictionary") {
            if system_list.enabled()
                && system_list.config.accepts_as_fix_result
                && self.spell_checker.is_some()
            {
                let sc = self.spell_checker.as_ref().unwrap();
                if sc.check(word) || sc.check(&word_lower) {
                    return true;
                }
            }
        }

        // Check word lists
        for wl in self.get_word_lists() {
            if !wl.enabled() || !wl.config.accepts_as_fix_result {
                continue;
            }
            if wl.config.source == "system" {
                continue;
            }
            if wl.contains(word) {
                return true;
            }
        }

        false
    }

    // =========================================================================
    // Statistics and Logging
    // =========================================================================

    /// Reset validation statistics.
    pub fn reset_stats(&mut self) {
        self.stats = ValidationStats::default();
    }

    /// Get current validation statistics.
    pub fn get_stats(&self) -> &ValidationStats {
        &self.stats
    }

    /// Log a summary of validation statistics.
    pub fn log_summary(&self) {
        info!("[OCR] Validation: {}", self.stats.get_summary());
    }

    /// Get summary of loaded word lists.
    pub fn get_list_summary(&self) -> String {
        let mut lines = Vec::new();
        for wl in self.get_word_lists() {
            let status = if wl.enabled() { "enabled" } else { "disabled" };
            let mut flags = String::new();
            if wl.config.validates_known {
                flags.push('V');
            }
            if wl.config.protects_from_fix {
                flags.push('P');
            }
            if wl.config.accepts_as_fix_result {
                flags.push('A');
            }
            if flags.is_empty() {
                flags.push('-');
            }
            lines.push(format!(
                "  [{:02}] {}: {} words ({}) [{}]",
                wl.config.order,
                wl.name(),
                wl.word_count(),
                status,
                flags
            ));
        }
        lines.join("\n")
    }
}

/// Global validation manager instance.
static VALIDATION_MANAGER: once_cell::sync::OnceCell<Mutex<ValidationManager>> =
    once_cell::sync::OnceCell::new();

/// Get or create the global ValidationManager instance.
pub fn get_validation_manager(config_dir: Option<&Path>) -> &'static Mutex<ValidationManager> {
    if let Some(dir) = config_dir {
        // Re-initialize if a config_dir is provided
        let manager = ValidationManager::new(dir);
        // Try to set; if already set, we can't replace (OnceLock semantics).
        // In practice, call this once at startup.
        let _ = VALIDATION_MANAGER.set(Mutex::new(manager));
    }
    VALIDATION_MANAGER.get_or_init(|| {
        let default_dir = PathBuf::from(".config/ocr");
        Mutex::new(ValidationManager::new(&default_dir))
    })
}

// =============================================================================
// Word List Loaders
// =============================================================================

/// Load words from a text file (one word per line).
///
/// Handles user_dictionary.txt, names.txt, romaji_dictionary.txt, eng_WordSplitList.txt.
pub fn load_text_wordlist(path: &Path) -> HashSet<String> {
    let mut words = HashSet::new();
    if !path.exists() {
        return words;
    }

    match fs::read_to_string(path) {
        Ok(content) => {
            for line in content.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    words.insert(word.to_lowercase());
                }
            }
            debug!(
                "[WordLists] Loaded {} words from {}",
                words.len(),
                path.file_name().unwrap_or_default().to_string_lossy()
            );
        }
        Err(e) => {
            error!("[WordLists] Error loading {}: {}", path.display(), e);
        }
    }

    words
}

/// Load words from a SubtitleEdit XML file.
///
/// Handles en_US_se.xml, en_interjections_se.xml.
pub fn load_se_xml_wordlist(path: &Path, element_name: &str) -> HashSet<String> {
    let mut words = HashSet::new();
    if !path.exists() {
        return words;
    }

    match fs::read_to_string(path) {
        Ok(content) => {
            use quick_xml::events::Event;
            use quick_xml::Reader;

            let mut reader = Reader::from_str(&content);
            let mut in_target = false;
            let mut current_text = String::new();

            loop {
                match reader.read_event() {
                    Ok(Event::Start(ref e)) => {
                        if e.name().as_ref() == element_name.as_bytes() {
                            in_target = true;
                            current_text.clear();
                        }
                    }
                    Ok(Event::Text(ref e)) => {
                        if in_target {
                            let decoded = e.decode().unwrap_or_default();
                            if let Ok(text) = quick_xml::escape::unescape(&decoded) {
                                current_text.push_str(&text);
                            }
                        }
                    }
                    Ok(Event::End(ref e)) => {
                        if e.name().as_ref() == element_name.as_bytes() && in_target {
                            let word = current_text.trim().to_string();
                            if !word.is_empty() {
                                words.insert(word.to_lowercase());
                            }
                            in_target = false;
                        }
                    }
                    Ok(Event::Eof) => break,
                    Err(e) => {
                        error!("[WordLists] XML error in {}: {}", path.display(), e);
                        break;
                    }
                    _ => {}
                }
            }

            debug!(
                "[WordLists] Loaded {} words from {}",
                words.len(),
                path.file_name().unwrap_or_default().to_string_lossy()
            );
        }
        Err(e) => {
            error!("[WordLists] Error loading {}: {}", path.display(), e);
        }
    }

    words
}

/// Load names from SubtitleEdit names XML file.
///
/// Returns (names_set, blacklist_set).
pub fn load_se_names_xml(path: &Path) -> (HashSet<String>, HashSet<String>) {
    let mut names = HashSet::new();
    let mut blacklist = HashSet::new();

    if !path.exists() {
        return (names, blacklist);
    }

    match fs::read_to_string(path) {
        Ok(content) => {
            use quick_xml::events::Event;
            use quick_xml::Reader;

            let mut reader = Reader::from_str(&content);
            let mut in_blacklist = false;
            let mut in_name = false;
            let mut current_text = String::new();

            loop {
                match reader.read_event() {
                    Ok(Event::Start(ref e)) => {
                        match e.name().as_ref() {
                            b"blacklist" => {
                                in_blacklist = true;
                            }
                            b"name" => {
                                in_name = true;
                                current_text.clear();
                            }
                            _ => {}
                        }
                    }
                    Ok(Event::Text(ref e)) => {
                        if in_name {
                            let decoded = e.decode().unwrap_or_default();
                            if let Ok(text) = quick_xml::escape::unescape(&decoded) {
                                current_text.push_str(&text);
                            }
                        }
                    }
                    Ok(Event::End(ref e)) => {
                        match e.name().as_ref() {
                            b"name" => {
                                if in_name {
                                    let name = current_text.trim().to_string();
                                    if !name.is_empty() {
                                        if in_blacklist {
                                            blacklist.insert(name);
                                        } else {
                                            // Only add if not in blacklist
                                            names.insert(name.to_lowercase());
                                        }
                                    }
                                    in_name = false;
                                }
                            }
                            b"blacklist" => {
                                in_blacklist = false;
                            }
                            _ => {}
                        }
                    }
                    Ok(Event::Eof) => break,
                    Err(e) => {
                        error!("[WordLists] XML error in {}: {}", path.display(), e);
                        break;
                    }
                    _ => {}
                }
            }

            debug!(
                "[WordLists] Loaded {} names ({} blacklisted) from {}",
                names.len(),
                blacklist.len(),
                path.file_name().unwrap_or_default().to_string_lossy()
            );
        }
        Err(e) => {
            error!("[WordLists] Error loading {}: {}", path.display(), e);
        }
    }

    (names, blacklist)
}

/// Initialize and populate the ValidationManager with all word lists.
///
/// This is the main entry point for setting up the validation system.
pub fn initialize_validation_manager(
    config_dir: &Path,
    spell_checker: Option<Box<dyn SpellChecker>>,
) -> ValidationManager {
    let mut manager = ValidationManager::new(config_dir);
    manager.word_lists.clear();

    // Set spell checker
    if let Some(sc) = spell_checker {
        manager.set_spell_checker(sc);
    }

    // Load additional language dictionaries for protection (stubbed)
    manager.init_protection_languages();

    // Load configurations (from JSON or defaults)
    let configs = manager.load_config();

    // Load words for each configured list
    for config in configs {
        let mut words = HashSet::new();

        if config.source == "system" {
            // System dictionary handled via spell_checker, no words to load
        } else if let Some(ref file_path) = config.file_path {
            let full_path = config_dir.join(file_path);

            if file_path.ends_with(".txt") {
                words = load_text_wordlist(&full_path);
            } else if file_path.to_lowercase().contains("names.xml") {
                let (names, _) = load_se_names_xml(&full_path);
                words = names;
            } else if file_path.ends_with(".xml") {
                // Try 'word' element first, common in SE files
                words = load_se_xml_wordlist(&full_path, "word");
            }
        }

        manager.add_word_list(config, words);
    }

    // Log summary
    info!(
        "[WordLists] Initialized {} word lists:",
        manager.word_lists.len()
    );
    info!("{}", manager.get_list_summary());

    manager
}
