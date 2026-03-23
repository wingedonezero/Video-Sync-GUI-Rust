//! Favorites dialog logic — 1:1 port of `vsg_qt/favorites_dialog/ui.py`.
//!
//! Manages saved favorite colors using `vsg_core::favorite_colors::FavoriteColorsManager`.

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// FavoritesLogic QObject.
        #[qobject]
        #[qml_element]
        #[qproperty(i32, favorite_count)]
        type FavoritesLogic = super::FavoritesLogicRust;

        /// Initialize with config directory path.
        #[qinvokable]
        fn initialize(self: Pin<&mut FavoritesLogic>, config_dir: QString);

        /// Load all favorites. Returns JSON array of {id, name, hex}.
        #[qinvokable]
        fn load_favorites(self: Pin<&mut FavoritesLogic>) -> QString;

        /// Add a new favorite color.
        #[qinvokable]
        fn add_favorite(self: Pin<&mut FavoritesLogic>, name: QString, hex: QString);

        /// Update a favorite by id.
        #[qinvokable]
        fn update_favorite(self: Pin<&mut FavoritesLogic>, id: QString, name: QString, hex: QString);

        /// Delete a favorite by id.
        #[qinvokable]
        fn delete_favorite(self: Pin<&mut FavoritesLogic>, id: QString);

        /// Signal: favorites list changed.
        #[qsignal]
        fn favorites_changed(self: Pin<&mut FavoritesLogic>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

use core::pin::Pin;
use std::path::PathBuf;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use vsg_core::favorite_colors::FavoriteColorsManager;

#[derive(Default)]
pub struct FavoritesLogicRust {
    favorite_count: i32,
    manager: Option<FavoriteColorsManager>,
}


impl ffi::FavoritesLogic {
    fn initialize(mut self: Pin<&mut Self>, config_dir: QString) {
        let dir = PathBuf::from(config_dir.to_string());
        self.as_mut().rust_mut().manager = Some(FavoriteColorsManager::new(&dir));
        // Refresh count
        let count = self
            .rust()
            .manager
            .as_ref()
            .map(|m| m.get_all().len())
            .unwrap_or(0);
        self.as_mut().set_favorite_count(count as i32);
    }

    fn load_favorites(self: Pin<&mut Self>) -> QString {
        let favorites = self
            .rust()
            .manager
            .as_ref()
            .map(|m| {
                let all = m.get_all();
                serde_json::to_string(&all).unwrap_or_else(|_| "[]".to_string())
            })
            .unwrap_or_else(|| "[]".to_string());
        QString::from(favorites.as_str())
    }

    fn add_favorite(mut self: Pin<&mut Self>, name: QString, hex: QString) {
        if let Some(m) = self.as_mut().rust_mut().manager.as_mut() {
            m.add(&name.to_string(), &hex.to_string());
        }
        let count = self
            .rust()
            .manager
            .as_ref()
            .map(|m| m.get_all().len())
            .unwrap_or(0);
        self.as_mut().set_favorite_count(count as i32);
        self.as_mut().favorites_changed();
    }

    fn update_favorite(mut self: Pin<&mut Self>, id: QString, name: QString, hex: QString) {
        if let Some(m) = self.as_mut().rust_mut().manager.as_mut() {
            let name_str = name.to_string();
            let hex_str = hex.to_string();
            m.update(
                &id.to_string(),
                if name_str.is_empty() { None } else { Some(&name_str) },
                if hex_str.is_empty() { None } else { Some(&hex_str) },
            );
        }
        self.as_mut().favorites_changed();
    }

    fn delete_favorite(mut self: Pin<&mut Self>, id: QString) {
        if let Some(m) = self.as_mut().rust_mut().manager.as_mut() {
            m.remove(&id.to_string());
        }
        let count = self
            .rust()
            .manager
            .as_ref()
            .map(|m| m.get_all().len())
            .unwrap_or(0);
        self.as_mut().set_favorite_count(count as i32);
        self.as_mut().favorites_changed();
    }
}
