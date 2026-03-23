//! Correlation method plugin registry — 1:1 port of `correlation/registry.py`.

use std::collections::HashMap;
use std::sync::Mutex;

use once_cell::sync::Lazy;

/// Trait that all correlation method plugins must implement — `CorrelationMethod`
pub trait CorrelationMethod: Send + Sync {
    /// Display name shown in UI and logs.
    fn name(&self) -> &str;

    /// AppSettings attribute for multi-correlation toggle.
    fn config_key(&self) -> &str;

    /// Compute delay between two audio chunks.
    ///
    /// Args:
    ///   ref_chunk: Reference audio (mono f32 slice).
    ///   tgt_chunk: Target audio (mono f32 slice).
    ///   sr: Sample rate in Hz.
    ///
    /// Returns: (delay_ms, confidence) where confidence is 0-100.
    fn find_delay(&self, ref_chunk: &[f32], tgt_chunk: &[f32], sr: i64) -> (f64, f64);
}

// ── Registry ─────────────────────────────────────────────────────────────────

static METHODS: Lazy<Mutex<HashMap<String, Box<dyn CorrelationMethod>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Register a correlation method plugin.
pub fn register(method: Box<dyn CorrelationMethod>) {
    let name = method.name().to_string();
    METHODS.lock().unwrap().insert(name, method);
}

/// Look up a method by its display name.
pub fn get_method(name: &str) -> Option<&'static dyn CorrelationMethod> {
    // SAFETY: We leak the reference to get a 'static lifetime.
    // Methods are registered once at startup and never removed.
    let methods = METHODS.lock().unwrap();
    methods.get(name).map(|m| {
        let ptr: *const dyn CorrelationMethod = &**m;
        unsafe { &*ptr }
    })
}

/// Return all registered method names.
pub fn list_methods() -> Vec<String> {
    METHODS
        .lock()
        .unwrap()
        .keys()
        .cloned()
        .collect()
}
