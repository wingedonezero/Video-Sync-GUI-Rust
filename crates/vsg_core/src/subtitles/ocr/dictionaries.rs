//! OCR Dictionary Management
//!
//! Manages databases for OCR text correction:
//!     1. replacements.json — Pattern-based character/word replacements
//!     2. user_dictionary.txt — User's custom valid words
//!     3. names.txt — Proper names (characters, places, etc.)
//!     4. romaji_dictionary.txt — Japanese romanization words

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Serialize, Deserialize};
use tracing::{debug, error, info, warn};

/// Types of replacement rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleType {
    Literal,
    Word,
    WordStart,
    WordEnd,
    WordMiddle,
    Regex,
}

impl RuleType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RuleType::Literal => "literal",
            RuleType::Word => "word",
            RuleType::WordStart => "word_start",
            RuleType::WordEnd => "word_end",
            RuleType::WordMiddle => "word_middle",
            RuleType::Regex => "regex",
        }
    }
}

/// A single replacement rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplacementRule {
    pub pattern: String,
    pub replacement: String,
    #[serde(rename = "type", default = "default_rule_type")]
    pub rule_type: String,
    #[serde(default)]
    pub confidence_gated: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub description: String,
}

fn default_rule_type() -> String { "literal".to_string() }
fn default_true() -> bool { true }

impl ReplacementRule {
    pub fn new(pattern: &str, replacement: &str) -> Self {
        Self {
            pattern: pattern.to_string(),
            replacement: replacement.to_string(),
            rule_type: "literal".to_string(),
            confidence_gated: false,
            enabled: true,
            description: String::new(),
        }
    }
}

/// Replacement rules file structure.
#[derive(Serialize, Deserialize)]
struct ReplacementsFile {
    version: i32,
    rules: Vec<ReplacementRule>,
}

/// Manages OCR correction dictionaries.
pub struct OCRDictionaries {
    pub config_dir: PathBuf,
    replacements_path: PathBuf,
    user_dict_path: PathBuf,
    names_path: PathBuf,
    replacements: Vec<ReplacementRule>,
    user_words: HashSet<String>,
    names: HashSet<String>,
}

impl OCRDictionaries {
    /// Find the OCR config directory.
    fn find_config_dir() -> PathBuf {
        let cwd_config = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".config")
            .join("ocr");

        if cwd_config.exists() {
            if cwd_config.join("replacements.json").exists()
                || cwd_config.join("user_dictionary.txt").exists()
                || cwd_config.join("romaji_dictionary.txt").exists()
            {
                info!("[OCR] Found existing config at: {}", cwd_config.display());
                return cwd_config;
            }
        }

        info!("[OCR] Using config dir at: {}", cwd_config.display());
        cwd_config
    }

    pub fn new(config_dir: Option<&Path>) -> Self {
        let config_dir = config_dir
            .map(PathBuf::from)
            .unwrap_or_else(Self::find_config_dir);

        let _ = std::fs::create_dir_all(&config_dir);

        let replacements_path = config_dir.join("replacements.json");
        let user_dict_path = config_dir.join("user_dictionary.txt");
        let names_path = config_dir.join("names.txt");

        let mut dicts = Self {
            config_dir,
            replacements_path,
            user_dict_path,
            names_path,
            replacements: Vec::new(),
            user_words: HashSet::new(),
            names: HashSet::new(),
        };

        dicts.ensure_defaults();
        dicts
    }

    fn ensure_defaults(&mut self) {
        if !self.replacements_path.exists() {
            let data = ReplacementsFile {
                version: 1,
                rules: Vec::new(),
            };
            if let Ok(json) = serde_json::to_string_pretty(&data) {
                let _ = std::fs::write(&self.replacements_path, json);
            }
        }

        if !self.user_dict_path.exists() {
            let _ = std::fs::write(
                &self.user_dict_path,
                "# User Dictionary - one word per line\n# Words here won't be flagged as unknown\n",
            );
        }

        if !self.names_path.exists() {
            let _ = std::fs::write(
                &self.names_path,
                "# Names Dictionary - one name per line\n# Character names, places, etc.\n",
            );
        }
    }

    /// Load replacement rules from JSON file.
    pub fn load_replacements(&mut self) -> Vec<ReplacementRule> {
        if !self.replacements.is_empty() {
            return self.replacements.clone();
        }

        if self.replacements_path.exists() {
            match std::fs::read_to_string(&self.replacements_path) {
                Ok(content) => {
                    match serde_json::from_str::<ReplacementsFile>(&content) {
                        Ok(data) => {
                            self.replacements = data.rules;
                            debug!("Loaded {} replacement rules", self.replacements.len());
                        }
                        Err(e) => {
                            error!("Error parsing replacements: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("Error loading replacements: {}", e);
                }
            }
        }

        self.replacements.clone()
    }

    /// Save replacement rules to JSON file.
    pub fn save_replacements(&mut self, rules: &[ReplacementRule]) -> bool {
        let data = ReplacementsFile {
            version: 1,
            rules: rules.to_vec(),
        };
        match serde_json::to_string_pretty(&data) {
            Ok(json) => {
                if std::fs::write(&self.replacements_path, json).is_ok() {
                    self.replacements = rules.to_vec();
                    return true;
                }
            }
            Err(e) => error!("Error serializing replacements: {}", e),
        }
        false
    }

    /// Load user dictionary words.
    pub fn load_user_dictionary(&mut self) -> &HashSet<String> {
        if self.user_words.is_empty() {
            self.user_words = self.load_wordlist(&self.user_dict_path);
        }
        &self.user_words
    }

    /// Load names dictionary.
    pub fn load_names(&mut self) -> &HashSet<String> {
        if self.names.is_empty() {
            self.names = self.load_wordlist(&self.names_path);
        }
        &self.names
    }

    /// Add a word to user dictionary.
    pub fn add_user_word(&mut self, word: &str) -> (bool, String) {
        let word = word.trim().to_string();
        if word.is_empty() {
            return (false, "Word cannot be empty".into());
        }

        self.load_user_dictionary();
        let lower_set: HashSet<String> = self.user_words.iter().map(|w| w.to_lowercase()).collect();
        if lower_set.contains(&word.to_lowercase()) {
            return (false, format!("Word '{}' already exists", word));
        }

        self.user_words.insert(word.clone());
        if self.save_wordlist(&self.user_dict_path.clone(), &self.user_words, "User Dictionary") {
            (true, format!("Added '{}'", word))
        } else {
            (false, "Failed to save dictionary".into())
        }
    }

    /// Add a name to names dictionary.
    pub fn add_name(&mut self, name: &str) -> (bool, String) {
        let name = name.trim().to_string();
        if name.is_empty() {
            return (false, "Name cannot be empty".into());
        }

        self.load_names();
        let lower_set: HashSet<String> = self.names.iter().map(|n| n.to_lowercase()).collect();
        if lower_set.contains(&name.to_lowercase()) {
            return (false, format!("Name '{}' already exists", name));
        }

        self.names.insert(name.clone());
        if self.save_wordlist(&self.names_path.clone(), &self.names, "Names Dictionary") {
            (true, format!("Added '{}'", name))
        } else {
            (false, "Failed to save names".into())
        }
    }

    /// Check if a word is known (in any dictionary).
    pub fn is_known_word(&mut self, word: &str) -> bool {
        let word_lower = word.to_lowercase();

        let user_words = self.load_user_dictionary();
        if user_words.iter().any(|w| w.to_lowercase() == word_lower) {
            return true;
        }

        let names = self.load_names();
        if names.iter().any(|n| n.to_lowercase() == word_lower) {
            return true;
        }

        false
    }

    /// Force reload all dictionaries from disk.
    pub fn reload(&mut self) {
        self.replacements.clear();
        self.user_words.clear();
        self.names.clear();
        self.load_replacements();
        self.load_user_dictionary();
        self.load_names();
        info!("Reloaded all dictionaries from {}", self.config_dir.display());
    }

    fn load_wordlist(&self, path: &Path) -> HashSet<String> {
        let mut words = HashSet::new();
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    words.insert(word.to_string());
                }
            }
        }
        words
    }

    fn save_wordlist(&self, path: &Path, words: &HashSet<String>, header: &str) -> bool {
        let mut lines = vec![
            format!("# {} - one word per line", header),
            String::new(),
        ];
        let mut sorted: Vec<&String> = words.iter().collect();
        sorted.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        for word in sorted {
            lines.push(word.clone());
        }
        std::fs::write(path, lines.join("\n")).is_ok()
    }
}

// Global instance
use std::sync::Mutex;
use once_cell::sync::Lazy;

static DICTIONARIES: Lazy<Mutex<Option<OCRDictionaries>>> = Lazy::new(|| Mutex::new(None));

/// Get or create the global dictionaries instance.
pub fn get_dictionaries(config_dir: Option<&Path>) -> OCRDictionaries {
    // For simplicity, always create a new instance.
    // Thread-safe global state would require Arc<Mutex<>> which complicates the API.
    OCRDictionaries::new(config_dir)
}
