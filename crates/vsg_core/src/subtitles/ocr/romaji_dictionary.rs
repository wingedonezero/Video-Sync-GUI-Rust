//! Romaji Dictionary Support
//!
//! Provides Japanese romanization (romaji) word validation for OCR.
//! Uses JMdict as the source for Japanese vocabulary.
//!
//! This prevents valid Japanese words written in romaji from being
//! flagged as unknown words by the OCR spell checker.
//!
//! Features:
//!     - Kana to romaji conversion (Hepburn romanization)
//!     - JMdict parsing for vocabulary extraction
//!     - Romaji wordlist generation and caching

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use parking_lot::Mutex;
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

/// JMdict download URL (English-only version is smaller).
pub const JMDICT_URL: &str = "http://ftp.edrdg.org/pub/Nihongo/JMdict_e.gz";
pub const JMDICT_FULL_URL: &str = "http://ftp.edrdg.org/pub/Nihongo/JMdict.gz";

/// Alternative mirrors if primary fails.
pub const JMDICT_MIRRORS: &[&str] = &[
    "http://ftp.edrdg.org/pub/Nihongo/JMdict_e.gz",
    "https://www.edrdg.org/pub/Nihongo/JMdict_e.gz",
];

/// Converts Japanese kana (hiragana/katakana) to romaji using Hepburn romanization.
///
/// Supports:
///     - All basic hiragana and katakana
///     - Small kana (っ, ゃ, ゅ, ょ, etc.)
///     - Long vowels (ー)
///     - Common digraphs and trigraphs
pub struct KanaToRomaji {
    /// Combined kana map (digraphs + hiragana + katakana).
    kana_map: HashMap<&'static str, &'static str>,
    /// Keys sorted by length descending for longest-match-first.
    sorted_kana: Vec<&'static str>,
}

impl Default for KanaToRomaji {
    fn default() -> Self {
        Self::new()
    }
}

impl KanaToRomaji {
    /// Hiragana to romaji mapping.
    const HIRAGANA: &'static [(&'static str, &'static str)] = &[
        // Basic vowels
        ("あ", "a"), ("い", "i"), ("う", "u"), ("え", "e"), ("お", "o"),
        // K-row
        ("か", "ka"), ("き", "ki"), ("く", "ku"), ("け", "ke"), ("こ", "ko"),
        // S-row
        ("さ", "sa"), ("し", "shi"), ("す", "su"), ("せ", "se"), ("そ", "so"),
        // T-row
        ("た", "ta"), ("ち", "chi"), ("つ", "tsu"), ("て", "te"), ("と", "to"),
        // N-row
        ("な", "na"), ("に", "ni"), ("ぬ", "nu"), ("ね", "ne"), ("の", "no"),
        // H-row
        ("は", "ha"), ("ひ", "hi"), ("ふ", "fu"), ("へ", "he"), ("ほ", "ho"),
        // M-row
        ("ま", "ma"), ("み", "mi"), ("む", "mu"), ("め", "me"), ("も", "mo"),
        // Y-row
        ("や", "ya"), ("ゆ", "yu"), ("よ", "yo"),
        // R-row
        ("ら", "ra"), ("り", "ri"), ("る", "ru"), ("れ", "re"), ("ろ", "ro"),
        // W-row
        ("わ", "wa"), ("ゐ", "wi"), ("ゑ", "we"), ("を", "wo"),
        // N
        ("ん", "n"),
        // Voiced (dakuten)
        ("が", "ga"), ("ぎ", "gi"), ("ぐ", "gu"), ("げ", "ge"), ("ご", "go"),
        ("ざ", "za"), ("じ", "ji"), ("ず", "zu"), ("ぜ", "ze"), ("ぞ", "zo"),
        ("だ", "da"), ("ぢ", "ji"), ("づ", "zu"), ("で", "de"), ("ど", "do"),
        ("ば", "ba"), ("び", "bi"), ("ぶ", "bu"), ("べ", "be"), ("ぼ", "bo"),
        // Half-voiced (handakuten)
        ("ぱ", "pa"), ("ぴ", "pi"), ("ぷ", "pu"), ("ぺ", "pe"), ("ぽ", "po"),
        // Small kana
        ("ぁ", "a"), ("ぃ", "i"), ("ぅ", "u"), ("ぇ", "e"), ("ぉ", "o"),
        ("ゃ", "ya"), ("ゅ", "yu"), ("ょ", "yo"), ("ゎ", "wa"),
        // Sokuon (small tsu) - handled specially
        ("っ", ""),
    ];

    /// Katakana to romaji mapping.
    const KATAKANA: &'static [(&'static str, &'static str)] = &[
        // Basic vowels
        ("ア", "a"), ("イ", "i"), ("ウ", "u"), ("エ", "e"), ("オ", "o"),
        // K-row
        ("カ", "ka"), ("キ", "ki"), ("ク", "ku"), ("ケ", "ke"), ("コ", "ko"),
        // S-row
        ("サ", "sa"), ("シ", "shi"), ("ス", "su"), ("セ", "se"), ("ソ", "so"),
        // T-row
        ("タ", "ta"), ("チ", "chi"), ("ツ", "tsu"), ("テ", "te"), ("ト", "to"),
        // N-row
        ("ナ", "na"), ("ニ", "ni"), ("ヌ", "nu"), ("ネ", "ne"), ("ノ", "no"),
        // H-row
        ("ハ", "ha"), ("ヒ", "hi"), ("フ", "fu"), ("ヘ", "he"), ("ホ", "ho"),
        // M-row
        ("マ", "ma"), ("ミ", "mi"), ("ム", "mu"), ("メ", "me"), ("モ", "mo"),
        // Y-row
        ("ヤ", "ya"), ("ユ", "yu"), ("ヨ", "yo"),
        // R-row
        ("ラ", "ra"), ("リ", "ri"), ("ル", "ru"), ("レ", "re"), ("ロ", "ro"),
        // W-row
        ("ワ", "wa"), ("ヰ", "wi"), ("ヱ", "we"), ("ヲ", "wo"),
        // N
        ("ン", "n"),
        // Voiced (dakuten)
        ("ガ", "ga"), ("ギ", "gi"), ("グ", "gu"), ("ゲ", "ge"), ("ゴ", "go"),
        ("ザ", "za"), ("ジ", "ji"), ("ズ", "zu"), ("ゼ", "ze"), ("ゾ", "zo"),
        ("ダ", "da"), ("ヂ", "ji"), ("ヅ", "zu"), ("デ", "de"), ("ド", "do"),
        ("バ", "ba"), ("ビ", "bi"), ("ブ", "bu"), ("ベ", "be"), ("ボ", "bo"),
        // Half-voiced (handakuten)
        ("パ", "pa"), ("ピ", "pi"), ("プ", "pu"), ("ペ", "pe"), ("ポ", "po"),
        // Small kana
        ("ァ", "a"), ("ィ", "i"), ("ゥ", "u"), ("ェ", "e"), ("ォ", "o"),
        ("ャ", "ya"), ("ュ", "yu"), ("ョ", "yo"), ("ヮ", "wa"),
        // Sokuon (small tsu) - handled specially
        ("ッ", ""),
        // Long vowel mark
        ("ー", ""),
        // Additional katakana for foreign words
        ("ヴ", "vu"), ("ヷ", "va"), ("ヸ", "vi"), ("ヹ", "ve"), ("ヺ", "vo"),
    ];

    /// Digraphs (two kana combinations).
    const DIGRAPHS: &'static [(&'static str, &'static str)] = &[
        // Hiragana y-compounds
        ("きゃ", "kya"), ("きゅ", "kyu"), ("きょ", "kyo"),
        ("しゃ", "sha"), ("しゅ", "shu"), ("しょ", "sho"),
        ("ちゃ", "cha"), ("ちゅ", "chu"), ("ちょ", "cho"),
        ("にゃ", "nya"), ("にゅ", "nyu"), ("にょ", "nyo"),
        ("ひゃ", "hya"), ("ひゅ", "hyu"), ("ひょ", "hyo"),
        ("みゃ", "mya"), ("みゅ", "myu"), ("みょ", "myo"),
        ("りゃ", "rya"), ("りゅ", "ryu"), ("りょ", "ryo"),
        ("ぎゃ", "gya"), ("ぎゅ", "gyu"), ("ぎょ", "gyo"),
        ("じゃ", "ja"), ("じゅ", "ju"), ("じょ", "jo"),
        ("ぢゃ", "ja"), ("ぢゅ", "ju"), ("ぢょ", "jo"),
        ("びゃ", "bya"), ("びゅ", "byu"), ("びょ", "byo"),
        ("ぴゃ", "pya"), ("ぴゅ", "pyu"), ("ぴょ", "pyo"),
        // Katakana y-compounds
        ("キャ", "kya"), ("キュ", "kyu"), ("キョ", "kyo"),
        ("シャ", "sha"), ("シュ", "shu"), ("ショ", "sho"),
        ("チャ", "cha"), ("チュ", "chu"), ("チョ", "cho"),
        ("ニャ", "nya"), ("ニュ", "nyu"), ("ニョ", "nyo"),
        ("ヒャ", "hya"), ("ヒュ", "hyu"), ("ヒョ", "hyo"),
        ("ミャ", "mya"), ("ミュ", "myu"), ("ミョ", "myo"),
        ("リャ", "rya"), ("リュ", "ryu"), ("リョ", "ryo"),
        ("ギャ", "gya"), ("ギュ", "gyu"), ("ギョ", "gyo"),
        ("ジャ", "ja"), ("ジュ", "ju"), ("ジョ", "jo"),
        ("ヂャ", "ja"), ("ヂュ", "ju"), ("ヂョ", "jo"),
        ("ビャ", "bya"), ("ビュ", "byu"), ("ビョ", "byo"),
        ("ピャ", "pya"), ("ピュ", "pyu"), ("ピョ", "pyo"),
        // Additional katakana combinations for foreign words
        ("ファ", "fa"), ("フィ", "fi"), ("フェ", "fe"), ("フォ", "fo"),
        ("ティ", "ti"), ("ディ", "di"),
        ("トゥ", "tu"), ("ドゥ", "du"),
        ("ウィ", "wi"), ("ウェ", "we"), ("ウォ", "wo"),
        ("ヴァ", "va"), ("ヴィ", "vi"), ("ヴェ", "ve"), ("ヴォ", "vo"),
        ("シェ", "she"), ("ジェ", "je"), ("チェ", "che"),
        ("ツァ", "tsa"), ("ツィ", "tsi"), ("ツェ", "tse"), ("ツォ", "tso"),
    ];

    /// Create a new converter with combined mappings.
    pub fn new() -> Self {
        let mut kana_map = HashMap::new();

        // Add digraphs first (longer matches take priority)
        for &(k, v) in Self::DIGRAPHS {
            kana_map.insert(k, v);
        }
        for &(k, v) in Self::HIRAGANA {
            kana_map.insert(k, v);
        }
        for &(k, v) in Self::KATAKANA {
            kana_map.insert(k, v);
        }

        // Sort keys by byte length descending for proper matching
        let mut sorted_kana: Vec<&str> = kana_map.keys().copied().collect();
        sorted_kana.sort_by(|a, b| b.len().cmp(&a.len()));

        Self {
            kana_map,
            sorted_kana,
        }
    }

    /// Convert kana text to romaji.
    pub fn convert(&self, text: &str) -> String {
        let mut result = Vec::new();
        let mut i = 0;
        let bytes = text.as_bytes();

        while i < text.len() {
            let remaining = &text[i..];
            let mut matched = false;

            // Try matching longest patterns first
            for &kana in &self.sorted_kana {
                if remaining.starts_with(kana) {
                    let romaji = self.kana_map[kana];

                    // Handle sokuon (small tsu) - doubles the next consonant
                    if (kana == "っ" || kana == "ッ") && i + kana.len() < text.len() {
                        let after = &text[i + kana.len()..];
                        // Look ahead for the next kana's romaji
                        for &next_kana in &self.sorted_kana {
                            if after.starts_with(next_kana) {
                                let next_romaji = self.kana_map[next_kana];
                                if !next_romaji.is_empty() {
                                    // Double the first consonant
                                    if let Some(c) = next_romaji.chars().next() {
                                        result.push(c.to_string());
                                    }
                                }
                                break;
                            }
                        }
                    }
                    // Handle long vowel mark (ー) - extends previous vowel
                    else if kana == "ー" && !result.is_empty() {
                        let prev = result.last().unwrap_or(&String::new()).clone();
                        if let Some(last_char) = prev.chars().last() {
                            if "aeiou".contains(last_char) {
                                result.push(last_char.to_string());
                            }
                        }
                    } else {
                        result.push(romaji.to_string());
                    }

                    i += kana.len();
                    matched = true;
                    break;
                }
            }

            if !matched {
                // Keep non-kana characters as-is
                // We need to advance by one full UTF-8 character
                if let Some(ch) = text[i..].chars().next() {
                    result.push(ch.to_string());
                    i += ch.len_utf8();
                } else {
                    i += 1;
                }
            }
        }

        result.join("")
    }

    /// Check if text is entirely kana (hiragana/katakana).
    pub fn is_kana(&self, text: &str) -> bool {
        for ch in text.chars() {
            let s = ch.to_string();
            if !self.kana_map.contains_key(s.as_str()) {
                // Allow some punctuation
                if !"・ーっッ".contains(ch) {
                    return false;
                }
            }
        }
        true
    }
}

/// Parses JMdict XML files to extract readings (kana) and convert to romaji.
///
/// JMdict structure:
/// ```xml
/// <entry>
///     <k_ele><keb>漢字</keb></k_ele>
///     <r_ele><reb>かんじ</reb></r_ele>
///     <sense>...</sense>
/// </entry>
/// ```
pub struct JMdictParser {
    converter: KanaToRomaji,
}

impl JMdictParser {
    /// Create a new parser with the given converter.
    pub fn new(converter: Option<KanaToRomaji>) -> Self {
        Self {
            converter: converter.unwrap_or_default(),
        }
    }

    /// Parse a JMdict XML file and extract romaji readings.
    ///
    /// Supports plain XML files. For gzip-compressed files (.gz), decompress
    /// them first before passing to this function.
    pub fn parse_file(
        &self,
        file_path: &Path,
        progress_callback: Option<&dyn Fn(usize, usize)>,
    ) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
        let mut romaji_words = HashSet::new();

        let is_gzip = file_path
            .extension()
            .map_or(false, |ext| ext == "gz");

        if is_gzip {
            return Err(format!(
                "Gzip files not supported directly. Please decompress {} first.",
                file_path.display()
            ).into());
        }

        let content = fs::read_to_string(file_path)?;

        let mut entry_count: usize = 0;

        // Use quick-xml to parse
        use quick_xml::events::Event;
        use quick_xml::Reader;

        let mut reader = Reader::from_str(&content);
        // buf is unused; quick-xml 0.39 uses read_event() without a buf argument
        let mut in_entry = false;
        let mut in_r_ele = false;
        let mut in_reb = false;
        let mut current_reb = String::new();

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => match e.name().as_ref() {
                    b"entry" => {
                        in_entry = true;
                    }
                    b"r_ele" if in_entry => {
                        in_r_ele = true;
                    }
                    b"reb" if in_r_ele => {
                        in_reb = true;
                        current_reb.clear();
                    }
                    _ => {}
                },
                Ok(Event::Text(ref e)) => {
                    if in_reb {
                        let decoded = e.decode().unwrap_or_default();
                        if let Ok(text) = quick_xml::escape::unescape(&decoded) {
                            current_reb.push_str(&text);
                        }
                    }
                }
                Ok(Event::End(ref e)) => match e.name().as_ref() {
                    b"reb" => {
                        if in_reb {
                            let kana = current_reb.trim().to_string();
                            let romaji = self.converter.convert(&kana);
                            // Filter: must be alphabetic and at least 3 chars
                            if !romaji.is_empty()
                                && romaji.chars().all(|c| c.is_ascii_alphabetic())
                                && romaji.len() >= 3
                            {
                                romaji_words.insert(romaji.to_lowercase());
                            }
                            in_reb = false;
                        }
                    }
                    b"r_ele" => {
                        in_r_ele = false;
                    }
                    b"entry" => {
                        in_entry = false;
                        entry_count += 1;
                        if let Some(cb) = progress_callback {
                            if entry_count % 10000 == 0 {
                                cb(entry_count, 0);
                            }
                        }
                    }
                    _ => {}
                },
                Ok(Event::Eof) => break,
                Err(e) => {
                    error!("XML parse error in {}: {}", file_path.display(), e);
                    return Err(Box::new(e));
                }
                _ => {}
            }
        }

        info!(
            "Extracted {} romaji words from {} entries",
            romaji_words.len(),
            entry_count
        );
        Ok(romaji_words)
    }

    /// Parse JMdict from a string.
    pub fn parse_string(
        &self,
        content: &str,
        progress_callback: Option<&dyn Fn(usize, usize)>,
    ) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
        let mut romaji_words = HashSet::new();
        let mut entry_count: usize = 0;

        use quick_xml::events::Event;
        use quick_xml::Reader;

        let mut reader = Reader::from_str(content);
        let mut in_entry = false;
        let mut in_r_ele = false;
        let mut in_reb = false;
        let mut current_reb = String::new();

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => match e.name().as_ref() {
                    b"entry" => in_entry = true,
                    b"r_ele" if in_entry => in_r_ele = true,
                    b"reb" if in_r_ele => {
                        in_reb = true;
                        current_reb.clear();
                    }
                    _ => {}
                },
                Ok(Event::Text(ref e)) => {
                    if in_reb {
                        let decoded = e.decode().unwrap_or_default();
                        if let Ok(text) = quick_xml::escape::unescape(&decoded) {
                            current_reb.push_str(&text);
                        }
                    }
                }
                Ok(Event::End(ref e)) => match e.name().as_ref() {
                    b"reb" => {
                        if in_reb {
                            let kana = current_reb.trim().to_string();
                            let romaji = self.converter.convert(&kana);
                            if !romaji.is_empty() && romaji.chars().all(|c| c.is_ascii_alphabetic())
                            {
                                romaji_words.insert(romaji.to_lowercase());
                            }
                            in_reb = false;
                        }
                    }
                    b"r_ele" => in_r_ele = false,
                    b"entry" => {
                        in_entry = false;
                        entry_count += 1;
                        if let Some(cb) = progress_callback {
                            if entry_count % 10000 == 0 {
                                cb(entry_count, 0);
                            }
                        }
                    }
                    _ => {}
                },
                Ok(Event::Eof) => break,
                Err(e) => {
                    error!("XML parse error: {}", e);
                    return Err(Box::new(e));
                }
                _ => {}
            }
        }

        info!(
            "Extracted {} romaji words from {} entries",
            romaji_words.len(),
            entry_count
        );
        Ok(romaji_words)
    }
}

/// Manages romaji wordlist for OCR validation.
///
/// Handles:
///     - Loading cached romaji dictionary
///     - Downloading and parsing JMdict if needed
///     - Word validation
pub struct RomajiDictionary {
    config_dir: PathBuf,
    dict_path: PathBuf,
    jmdict_path: PathBuf,
    words: Mutex<Option<HashSet<String>>>,
    words_mtime: Mutex<f64>,
    converter: KanaToRomaji,
}

impl RomajiDictionary {
    const DICT_FILENAME: &'static str = "romaji_dictionary.txt";
    const JMDICT_CACHE: &'static str = "JMdict_e.gz";

    /// Create a new romaji dictionary with the given config directory.
    pub fn new(config_dir: &Path) -> Self {
        let config_dir = config_dir.to_path_buf();
        let _ = fs::create_dir_all(&config_dir);

        let dict_path = config_dir.join(Self::DICT_FILENAME);
        let jmdict_path = config_dir.join(Self::JMDICT_CACHE);

        Self {
            config_dir,
            dict_path,
            jmdict_path,
            words: Mutex::new(None),
            words_mtime: Mutex::new(0.0),
            converter: KanaToRomaji::new(),
        }
    }

    /// Load romaji dictionary from file.
    ///
    /// Automatically reloads if the file was modified since last load.
    pub fn load(&self) -> HashSet<String> {
        if !self.dict_path.exists() {
            let mut words = self.words.lock();
            if words.is_none() || words.as_ref().map_or(false, |w| !w.is_empty()) {
                info!("[Romaji] Dictionary not found at: {}", self.dict_path.display());
            }
            *words = Some(HashSet::new());
            *self.words_mtime.lock() = 0.0;
            return HashSet::new();
        }

        // Check if we need to reload
        let current_mtime = fs::metadata(&self.dict_path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);

        {
            let words = self.words.lock();
            let mtime = self.words_mtime.lock();
            if words.is_some() && current_mtime == *mtime {
                return words.as_ref().unwrap().clone();
            }
        }

        // Load the file
        match fs::File::open(&self.dict_path) {
            Ok(file) => {
                let reader = BufReader::new(file);
                let mut new_words = HashSet::new();
                for line in reader.lines() {
                    if let Ok(line) = line {
                        let word = line.trim().to_string();
                        if !word.is_empty() && !word.starts_with('#') {
                            new_words.insert(word.to_lowercase());
                        }
                    }
                }

                info!(
                    "[Romaji] Loaded {} words from {}",
                    new_words.len(),
                    self.dict_path.display()
                );

                let result = new_words.clone();
                *self.words.lock() = Some(new_words);
                *self.words_mtime.lock() = current_mtime;
                result
            }
            Err(e) => {
                error!("Error loading romaji dictionary: {}", e);
                *self.words.lock() = Some(HashSet::new());
                *self.words_mtime.lock() = 0.0;
                HashSet::new()
            }
        }
    }

    /// Save romaji dictionary to file.
    pub fn save(&self, words: &HashSet<String>) -> bool {
        match (|| -> Result<(), Box<dyn std::error::Error>> {
            let temp_path = self.config_dir.join("romaji_dictionary.tmp");
            {
                let mut f = fs::File::create(&temp_path)?;
                writeln!(f, "# Romaji Dictionary - Japanese words in romanized form")?;
                writeln!(
                    f,
                    "# Generated from JMdict - {} words",
                    words.len()
                )?;
                writeln!(f, "# https://www.edrdg.org/jmdict/j_jmdict.html")?;
                writeln!(f)?;

                let mut sorted: Vec<&String> = words.iter().collect();
                sorted.sort();
                for word in sorted {
                    writeln!(f, "{}", word)?;
                }
            }

            fs::rename(&temp_path, &self.dict_path)?;
            *self.words.lock() = Some(words.clone());
            info!(
                "Saved {} romaji words to {}",
                words.len(),
                self.dict_path.display()
            );
            Ok(())
        })() {
            Ok(()) => true,
            Err(e) => {
                error!("Error saving romaji dictionary: {}", e);
                false
            }
        }
    }

    /// Check if a word is a valid romaji word.
    pub fn is_valid_word(&self, word: &str) -> bool {
        let words = self.load();
        let word_lower = word.to_lowercase();
        let is_valid = words.contains(&word_lower);

        if !is_valid && !words.is_empty() {
            debug!(
                "Romaji check: '{}' not in dictionary ({} words loaded from {})",
                word_lower,
                words.len(),
                self.dict_path.display()
            );
        }

        is_valid
    }

    /// Download JMdict file.
    ///
    /// Note: In the Rust port, HTTP downloading is not implemented.
    /// Users should download JMdict_e.gz manually and place it in the config directory.
    pub fn download_jmdict(
        &self,
        _progress_callback: Option<&dyn Fn(usize, usize)>,
    ) -> bool {
        // HTTP downloading not implemented in this port.
        // Users should provide the JMdict file manually.
        warn!(
            "JMdict download not available in Rust port. \
             Please download JMdict_e.gz manually from {} and place in {}",
            JMDICT_URL,
            self.config_dir.display()
        );
        false
    }

    /// Build romaji dictionary from JMdict.
    ///
    /// Downloads JMdict if not cached, parses it, and saves the romaji wordlist.
    pub fn build_dictionary(
        &self,
        progress_callback: Option<&dyn Fn(&str, usize, usize)>,
    ) -> (bool, String) {
        // Download if needed
        if !self.jmdict_path.exists() {
            if let Some(cb) = progress_callback {
                cb("Downloading JMdict...", 0, 0);
            }

            if !self.download_jmdict(None) {
                return (false, "Failed to download JMdict".to_string());
            }
        }

        // Parse JMdict
        if let Some(cb) = progress_callback {
            cb("Parsing JMdict...", 0, 0);
        }

        let parser = JMdictParser::new(None);

        let parse_progress = |current: usize, total: usize| {
            if let Some(cb) = progress_callback {
                cb(&format!("Parsing... {} entries", current), current, total);
            }
        };

        match parser.parse_file(&self.jmdict_path, Some(&parse_progress)) {
            Ok(mut romaji_words) => {
                // Add common particles
                for word in Self::get_common_particles() {
                    romaji_words.insert(word.to_string());
                }

                if let Some(cb) = progress_callback {
                    cb("Saving dictionary...", 0, 0);
                }

                if self.save(&romaji_words) {
                    let msg = format!(
                        "Built romaji dictionary with {} words",
                        romaji_words.len()
                    );
                    (true, msg)
                } else {
                    (false, "Failed to save dictionary".to_string())
                }
            }
            Err(e) => {
                error!("Error building romaji dictionary: {}", e);
                (false, format!("Error: {}", e))
            }
        }
    }

    /// Get common Japanese particles and suffixes in romaji.
    fn get_common_particles() -> HashSet<&'static str> {
        let words: HashSet<&str> = [
            // Particles
            "wa", "wo", "ga", "no", "ni", "de", "to", "mo", "ka", "ne", "yo", "na", "he",
            "kara", "made", "yori", "dake", "shika", "bakari", "nado",
            // Common suffixes
            "san", "sama", "kun", "chan", "sensei", "senpai", "kouhai", "shi", "tachi", "ra",
            "domo",
            // Common words often in anime
            "hai", "iie", "nani", "dou", "naze", "dare", "doko", "itsu", "kore", "sore",
            "are", "dore", "kono", "sono", "ano", "dono", "kou", "sou", "aa", "sugoi",
            "kawaii", "kakkoii", "kirei", "utsukushii", "baka", "aho", "uso", "hontou",
            "maji", "gomen", "sumimasen", "arigatou", "doumo", "ohayou", "konnichiwa",
            "konbanwa", "sayonara", "oyasumi", "ittekimasu", "itterasshai", "tadaima",
            "okaeri", "itadakimasu", "gochisousama", "suki", "daisuki", "kirai",
            "daikirai", "onegai", "kudasai", "choudai", "chotto", "matte", "yamete",
            "dame", "yatta", "yosh", "ganbare", "ganbatte", "nee", "eto", "maa", "hora",
            // Common anime/otaku terms
            "anime", "manga", "otaku", "waifu", "husbando", "shonen", "shoujo", "mecha",
            "isekai", "ecchi", "hentai", "yaoi", "yuri", "chibi", "moe", "tsundere",
            "yandere", "kuudere", "dandere", "nakama", "tomodachi", "koibito", "kareshi",
            "kanojo",
        ]
        .into_iter()
        .collect();
        words
    }

    /// Get dictionary statistics.
    pub fn get_stats(&self) -> HashMap<String, serde_json::Value> {
        let words = self.load();
        let mut stats = HashMap::new();
        stats.insert(
            "word_count".to_string(),
            serde_json::Value::Number(words.len().into()),
        );
        stats.insert(
            "dict_exists".to_string(),
            serde_json::Value::Bool(self.dict_path.exists()),
        );
        stats.insert(
            "jmdict_cached".to_string(),
            serde_json::Value::Bool(self.jmdict_path.exists()),
        );
        stats.insert(
            "dict_path".to_string(),
            serde_json::Value::String(self.dict_path.display().to_string()),
        );
        stats
    }
}

/// Global romaji dictionary instance.
static ROMAJI_DICT: OnceLock<Mutex<Option<RomajiDictionary>>> = OnceLock::new();

/// Get or create the global romaji dictionary instance.
pub fn get_romaji_dictionary() -> &'static Mutex<Option<RomajiDictionary>> {
    ROMAJI_DICT.get_or_init(|| Mutex::new(None))
}

/// Initialize the global romaji dictionary with a config directory.
pub fn init_romaji_dictionary(config_dir: &Path) -> &'static Mutex<Option<RomajiDictionary>> {
    let lock = get_romaji_dictionary();
    let mut guard = lock.lock();
    *guard = Some(RomajiDictionary::new(config_dir));
    lock
}

/// Check if a word is a valid romaji word.
///
/// Convenience function for quick lookups.
pub fn is_romaji_word(word: &str) -> bool {
    let lock = get_romaji_dictionary();
    let guard = lock.lock();
    match guard.as_ref() {
        Some(dict) => dict.is_valid_word(word),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kana_to_romaji_basic() {
        let converter = KanaToRomaji::new();
        assert_eq!(converter.convert("あいうえお"), "aiueo");
        assert_eq!(converter.convert("かきくけこ"), "kakikukeko");
        assert_eq!(converter.convert("さしすせそ"), "sashisuseso");
    }

    #[test]
    fn test_kana_to_romaji_digraphs() {
        let converter = KanaToRomaji::new();
        assert_eq!(converter.convert("しゃ"), "sha");
        assert_eq!(converter.convert("ちゅ"), "chu");
        assert_eq!(converter.convert("きょ"), "kyo");
    }

    #[test]
    fn test_kana_to_romaji_sokuon() {
        let converter = KanaToRomaji::new();
        assert_eq!(converter.convert("がっこう"), "gakkou");
        assert_eq!(converter.convert("にっぽん"), "nippon");
    }

    #[test]
    fn test_kana_to_romaji_katakana() {
        let converter = KanaToRomaji::new();
        assert_eq!(converter.convert("アイウエオ"), "aiueo");
        assert_eq!(converter.convert("カタカナ"), "katakana");
    }

    #[test]
    fn test_kana_to_romaji_long_vowel() {
        let converter = KanaToRomaji::new();
        assert_eq!(converter.convert("ラーメン"), "raamen");
    }

    #[test]
    fn test_is_kana() {
        let converter = KanaToRomaji::new();
        assert!(converter.is_kana("あいうえお"));
        assert!(converter.is_kana("カタカナ"));
        assert!(!converter.is_kana("hello"));
        assert!(!converter.is_kana("漢字"));
    }

    #[test]
    fn test_common_particles_exist() {
        let particles = RomajiDictionary::get_common_particles();
        assert!(particles.contains("arigatou"));
        assert!(particles.contains("anime"));
        assert!(particles.contains("wa"));
    }
}
