//! romaji_dictionary — stub for `vsg_core/subtitles/ocr/romaji_dictionary.py`.

use std::sync::OnceLock;

/// Kana to Romaji conversion.
pub struct KanaToRomaji;
/// Romaji dictionary.
pub struct RomajiDictionary;

/// Get the romaji dictionary (singleton).
pub fn get_romaji_dictionary() -> &'static RomajiDictionary {
    static DICT: OnceLock<RomajiDictionary> = OnceLock::new();
    DICT.get_or_init(|| RomajiDictionary)
}

/// Check if a word is a valid romaji word.
pub fn is_romaji_word(_word: &str) -> bool {
    false
}
