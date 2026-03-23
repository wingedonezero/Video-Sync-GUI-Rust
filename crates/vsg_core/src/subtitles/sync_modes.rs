//! Subtitle synchronization modes - Plugin system — 1:1 port of `sync_modes.py`.
//!
//! Sync modes are plugins that adjust subtitle timing to synchronize
//! with a target video. All modes work the same way:
//! 1. Receive SubtitleData with float ms timing
//! 2. Apply timing adjustments directly to events
//! 3. Return OperationResult with statistics
//!
//! Registry pattern allows easy addition of new sync modes.
//!
//! Plugins are located in: subtitles/sync_mode_plugins/

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::subtitles::data::{OperationResult, SubtitleData};

// =============================================================================
// Plugin System
// =============================================================================

/// Parameters for sync plugin application.
pub struct SyncParams<'a> {
    pub total_delay_ms: f64,
    pub global_shift_ms: f64,
    pub target_fps: Option<f64>,
    pub source_video: Option<&'a str>,
    pub target_video: Option<&'a str>,
    pub log: Option<&'a dyn Fn(&str)>,
    /// Additional mode-specific parameters
    pub extra: HashMap<String, serde_json::Value>,
}

impl<'a> SyncParams<'a> {
    pub fn new(total_delay_ms: f64, global_shift_ms: f64) -> Self {
        Self {
            total_delay_ms,
            global_shift_ms,
            target_fps: None,
            source_video: None,
            target_video: None,
            log: None,
            extra: HashMap::new(),
        }
    }
}

/// Base trait for sync mode plugins.
///
/// Each sync mode implements this interface to integrate with SubtitleData.
pub trait SyncPlugin: Send + Sync {
    /// Plugin name (must match what's used in settings)
    fn name(&self) -> &str;

    /// Human-readable description
    fn description(&self) -> &str;

    /// Apply sync to subtitle data.
    ///
    /// This modifies subtitle_data.events in place.
    fn apply(
        &self,
        subtitle_data: &mut SubtitleData,
        params: &SyncParams,
    ) -> OperationResult;
}

// Plugin registry
static SYNC_PLUGINS: OnceLock<HashMap<String, Box<dyn SyncPlugin>>> = OnceLock::new();

/// Initialize the plugin registry with all known plugins.
fn init_plugins() -> HashMap<String, Box<dyn SyncPlugin>> {
    let mut plugins: HashMap<String, Box<dyn SyncPlugin>> = HashMap::new();

    // Register time-based plugin
    let time_based = super::sync_mode_plugins::time_based::TimeBasedSync;
    plugins.insert(time_based.name().to_string(), Box::new(time_based));

    plugins
}

/// Get the plugin registry, initializing if needed.
fn get_registry() -> &'static HashMap<String, Box<dyn SyncPlugin>> {
    SYNC_PLUGINS.get_or_init(init_plugins)
}

/// Get a sync plugin instance by name.
pub fn get_sync_plugin(name: &str) -> Option<&'static dyn SyncPlugin> {
    get_registry().get(name).map(|p| p.as_ref())
}

/// List all registered sync plugins.
///
/// Returns a map of name -> description.
pub fn list_sync_plugins() -> HashMap<String, String> {
    get_registry()
        .iter()
        .map(|(name, plugin)| (name.clone(), plugin.description().to_string()))
        .collect()
}
