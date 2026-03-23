//! Favorite colors manager — 1:1 port of `vsg_core/favorite_colors.py`.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;
use serde_json::{json, Value};
use uuid::Uuid;

/// Manages favorite colors stored in JSON — `FavoriteColorsManager`
pub struct FavoriteColorsManager {
    config_file: PathBuf,
    favorites: Vec<Value>,
}

impl FavoriteColorsManager {
    const VERSION: i32 = 1;

    pub fn new(config_dir: &Path) -> Self {
        let config_file = config_dir.join("favorite_colors.json");
        let mut mgr = Self {
            config_file,
            favorites: Vec::new(),
        };
        mgr.load();
        mgr
    }

    fn load(&mut self) {
        if !self.config_file.exists() {
            self.favorites = Vec::new();
            return;
        }
        match fs::read_to_string(&self.config_file) {
            Ok(content) => match serde_json::from_str::<Value>(&content) {
                Ok(data) => {
                    self.favorites = data["favorites"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter(|f| Self::validate_favorite(f))
                                .cloned()
                                .collect()
                        })
                        .unwrap_or_default();
                }
                Err(_) => self.favorites = Vec::new(),
            },
            Err(_) => self.favorites = Vec::new(),
        }
    }

    fn validate_favorite(fav: &Value) -> bool {
        let has_keys = fav.get("id").is_some()
            && fav.get("name").is_some()
            && fav.get("hex").is_some();
        if !has_keys {
            return false;
        }
        fav["hex"]
            .as_str()
            .map(|s| s.starts_with('#'))
            .unwrap_or(false)
    }

    fn save(&self) {
        if let Some(parent) = self.config_file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let data = json!({
            "version": Self::VERSION,
            "favorites": self.favorites,
        });
        let _ = fs::write(
            &self.config_file,
            serde_json::to_string_pretty(&data).unwrap_or_default(),
        );
    }

    pub fn get_all(&self) -> Vec<Value> {
        self.favorites.clone()
    }

    pub fn add(&mut self, name: &str, hex_color: &str) -> String {
        let mut hex = hex_color.to_uppercase();
        if !hex.starts_with('#') {
            hex = format!("#{hex}");
        }
        let id = Uuid::new_v4().to_string();
        let name = if name.trim().is_empty() {
            "Unnamed Color"
        } else {
            name.trim()
        };

        self.favorites.push(json!({
            "id": id,
            "name": name,
            "hex": hex,
            "created": Local::now().to_rfc3339(),
        }));
        self.save();
        id
    }

    pub fn update(&mut self, favorite_id: &str, name: Option<&str>, hex_color: Option<&str>) -> bool {
        for fav in &mut self.favorites {
            if fav["id"].as_str() == Some(favorite_id) {
                if let Some(n) = name {
                    fav["name"] = json!(if n.trim().is_empty() { "Unnamed Color" } else { n.trim() });
                }
                if let Some(h) = hex_color {
                    let mut hex = h.to_uppercase();
                    if !hex.starts_with('#') {
                        hex = format!("#{hex}");
                    }
                    fav["hex"] = json!(hex);
                }
                self.save();
                return true;
            }
        }
        false
    }

    pub fn remove(&mut self, favorite_id: &str) -> bool {
        let original_len = self.favorites.len();
        self.favorites.retain(|f| f["id"].as_str() != Some(favorite_id));
        if self.favorites.len() < original_len {
            self.save();
            true
        } else {
            false
        }
    }

    pub fn reorder(&mut self, favorite_ids: &[String]) {
        let id_to_fav: std::collections::HashMap<String, Value> = self
            .favorites
            .iter()
            .filter_map(|f| f["id"].as_str().map(|id| (id.to_string(), f.clone())))
            .collect();

        let mut new_order = Vec::new();
        for id in favorite_ids {
            if let Some(fav) = id_to_fav.get(id) {
                new_order.push(fav.clone());
            }
        }
        // Add any not in the list at the end
        for fav in &self.favorites {
            if let Some(id) = fav["id"].as_str() {
                if !favorite_ids.contains(&id.to_string()) {
                    new_order.push(fav.clone());
                }
            }
        }
        self.favorites = new_order;
        self.save();
    }

    pub fn clear_all(&mut self) {
        self.favorites.clear();
        self.save();
    }
}
