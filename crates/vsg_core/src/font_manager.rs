//! Font manager — 1:1 port of `vsg_core/font_manager.py`.
//!
//! Font scanning, parsing, and replacement tracking for subtitle files.
//! Uses fontconfig (fc-query) for font info, with freetype-rs planned for
//! direct font parsing (matching Python's fonttools TTFont usage).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

// ─── Fontconfig helpers ──────────────────────────────────────────────────────

/// Get font info using fontconfig's fc-query — `_get_fontconfig_info`
fn get_fontconfig_info(font_path: &Path) -> HashMap<String, String> {
    let mut result = HashMap::new();
    result.insert("family".to_string(), String::new());
    result.insert("fullname".to_string(), String::new());
    result.insert("style".to_string(), String::new());

    let output = Command::new("fc-query")
        .args(["-f", "%{family}\\n%{fullname}\\n%{style}", &font_path.to_string_lossy()])
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = stdout.trim().split('\n').collect();
            if let Some(line) = lines.first() {
                result.insert("family".to_string(), line.split(',').next().unwrap_or("").trim().to_string());
            }
            if let Some(line) = lines.get(1) {
                result.insert("fullname".to_string(), line.split(',').next().unwrap_or("").trim().to_string());
            }
            if let Some(line) = lines.get(2) {
                result.insert("style".to_string(), line.split(',').next().unwrap_or("").trim().to_string());
            }
        }
    }

    result
}

// ─── FontInfo ────────────────────────────────────────────────────────────────

/// Information about a font file — `FontInfo`
#[derive(Debug, Clone)]
pub struct FontInfo {
    pub file_path: PathBuf,
    pub filename: String,
    pub family_name: String,
    pub subfamily: String,
    pub full_name: String,
    pub postscript_name: String,
    pub is_valid: bool,
    pub error: Option<String>,
}

impl FontInfo {
    pub fn new(file_path: &Path) -> Self {
        let mut info = Self {
            file_path: file_path.to_path_buf(),
            filename: file_path.file_name().unwrap_or_default().to_string_lossy().to_string(),
            family_name: String::new(),
            subfamily: String::new(),
            full_name: String::new(),
            postscript_name: String::new(),
            is_valid: false,
            error: None,
        };
        info.parse_font();
        info
    }

    fn parse_font(&mut self) {
        // TODO: When freetype-rs is added, use it for direct font parsing
        // (matching Python's fonttools TTFont for name table IDs 1,2,4,6).
        // For now, use fontconfig (fc-query) which is what libass uses.

        let fc_info = get_fontconfig_info(&self.file_path);
        let family = fc_info.get("family").map(|s| s.as_str()).unwrap_or("");

        if !family.is_empty() {
            self.family_name = family.to_string();
            let fullname = fc_info.get("fullname").map(|s| s.as_str()).unwrap_or("");
            self.full_name = if fullname.is_empty() {
                family.to_string()
            } else {
                fullname.to_string()
            };
            self.subfamily = fc_info.get("style").map(|s| s.to_string()).unwrap_or_default();
            self.is_valid = true;
        } else {
            // Fallback to filename stem
            let stem = self.file_path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            self.family_name = stem.clone();
            self.full_name = stem;
            self.is_valid = true;
        }
    }
}

// ─── FontScanner ─────────────────────────────────────────────────────────────

/// Scans directories for font files — `FontScanner`
pub struct FontScanner {
    fonts_dir: PathBuf,
    font_cache: HashMap<String, FontInfo>,
}

const FONT_EXTENSIONS: &[&str] = &[".ttf", ".otf", ".ttc", ".woff", ".woff2"];

impl FontScanner {
    pub fn new(fonts_dir: &Path) -> Self {
        Self {
            fonts_dir: fonts_dir.to_path_buf(),
            font_cache: HashMap::new(),
        }
    }

    pub fn scan(&mut self, include_subdirs: bool) -> Vec<FontInfo> {
        if !self.fonts_dir.exists() {
            return Vec::new();
        }

        let mut fonts = Vec::new();
        let mut seen_paths = std::collections::HashSet::new();

        self.scan_dir(&self.fonts_dir.clone(), include_subdirs, &mut fonts, &mut seen_paths);
        fonts
    }

    fn scan_dir(
        &mut self,
        dir: &Path,
        recurse: bool,
        fonts: &mut Vec<FontInfo>,
        seen: &mut std::collections::HashSet<String>,
    ) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let ext = path.extension()
                    .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
                    .unwrap_or_default();
                if !FONT_EXTENSIONS.contains(&ext.as_str()) {
                    continue;
                }

                let key = path.to_string_lossy().to_string();
                if seen.contains(&key) {
                    continue;
                }
                seen.insert(key.clone());

                if !self.font_cache.contains_key(&key) {
                    self.font_cache.insert(key.clone(), FontInfo::new(&path));
                }
                if let Some(info) = self.font_cache.get(&key) {
                    fonts.push(info.clone());
                }
            } else if path.is_dir() && recurse {
                self.scan_dir(&path, true, fonts, seen);
            }
        }
    }

    pub fn get_font_by_family(&mut self, family_name: &str) -> Vec<FontInfo> {
        let fonts = self.scan(true);
        fonts.into_iter()
            .filter(|f| f.family_name.to_lowercase() == family_name.to_lowercase())
            .collect()
    }

    pub fn get_font_families(&mut self) -> HashMap<String, Vec<FontInfo>> {
        let fonts = self.scan(true);
        let mut families: HashMap<String, Vec<FontInfo>> = HashMap::new();
        for font in fonts {
            families.entry(font.family_name.clone()).or_default().push(font);
        }
        families
    }

    pub fn clear_cache(&mut self) {
        self.font_cache.clear();
    }
}

// ─── FontReplacementManager ─────────────────────────────────────────────────

/// Manages font replacement operations — `FontReplacementManager`
pub struct FontReplacementManager {
    pub fonts_dir: PathBuf,
    replacements: HashMap<String, HashMap<String, Value>>,
}

impl FontReplacementManager {
    pub fn new(fonts_dir: &Path) -> Self {
        Self {
            fonts_dir: fonts_dir.to_path_buf(),
            replacements: HashMap::new(),
        }
    }

    pub fn add_replacement(
        &mut self,
        style_name: &str,
        original_font: &str,
        new_font_name: &str,
        font_file_path: Option<&Path>,
    ) -> String {
        let mut entry = HashMap::new();
        entry.insert("original_font".to_string(), serde_json::json!(original_font));
        entry.insert("new_font_name".to_string(), serde_json::json!(new_font_name));
        entry.insert(
            "font_file_path".to_string(),
            font_file_path
                .map(|p| serde_json::json!(p.to_string_lossy()))
                .unwrap_or(serde_json::json!(null)),
        );
        self.replacements.insert(style_name.to_string(), entry);
        style_name.to_string()
    }

    pub fn remove_replacement(&mut self, style_name: &str) -> bool {
        self.replacements.remove(style_name).is_some()
    }

    pub fn get_replacements(&self) -> &HashMap<String, HashMap<String, Value>> {
        &self.replacements
    }

    pub fn clear_replacements(&mut self) {
        self.replacements.clear();
    }

    pub fn validate_replacement_files(&self) -> Vec<String> {
        let mut errors = Vec::new();
        for (style_name, repl) in &self.replacements {
            if let Some(path_val) = repl.get("font_file_path") {
                if let Some(path_str) = path_val.as_str() {
                    if !Path::new(path_str).exists() {
                        errors.push(format!(
                            "Font file not found for style '{style_name}': {path_str}"
                        ));
                    }
                }
            }
        }
        errors
    }

    pub fn copy_fonts_to_temp(&self, temp_dir: &Path) -> Vec<PathBuf> {
        let fonts_temp = temp_dir.join("replacement_fonts");
        let _ = std::fs::create_dir_all(&fonts_temp);

        let mut copied = Vec::new();
        for repl in self.replacements.values() {
            if let Some(path_str) = repl.get("font_file_path").and_then(|v| v.as_str()) {
                let src = Path::new(path_str);
                if src.exists() {
                    if let Some(file_name) = src.file_name() {
                        let dst = fonts_temp.join(file_name);
                        if std::fs::copy(src, &dst).is_ok() {
                            copied.push(dst);
                        }
                    }
                }
            }
        }
        copied
    }
}

// ─── Validation (standalone functions) ───────────────────────────────────────

/// Validate font replacements — `validate_font_replacements`
pub fn validate_font_replacements(
    replacements: &HashMap<String, HashMap<String, Value>>,
) -> HashMap<String, Value> {
    let mut result = serde_json::json!({
        "valid": true,
        "missing_files": [],
        "missing_styles": [],
        "warnings": [],
        "errors": [],
    });

    if replacements.is_empty() {
        return serde_json::from_value(result).unwrap_or_default();
    }

    for (style_name, repl) in replacements {
        if let Some(path_str) = repl.get("font_file_path").and_then(|v| v.as_str()) {
            if !Path::new(path_str).exists() {
                if let Some(arr) = result["missing_files"].as_array_mut() {
                    arr.push(serde_json::json!(path_str));
                }
                if let Some(arr) = result["errors"].as_array_mut() {
                    arr.push(serde_json::json!(format!(
                        "Font file not found for style '{style_name}': {}",
                        Path::new(path_str).file_name().unwrap_or_default().to_string_lossy()
                    )));
                }
                result["valid"] = serde_json::json!(false);
            }
        }
    }

    serde_json::from_value(result).unwrap_or_default()
}
