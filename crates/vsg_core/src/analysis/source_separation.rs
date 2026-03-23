//! Source separation — 1:1 port of `vsg_core/analysis/source_separation.py`.
//!
//! Runs audio source separation via python-audio-separator subprocess.
//! The actual separation runs in a Python subprocess for complete memory
//! cleanup after processing.

use std::collections::HashMap;

/// Separation modes available in the UI — `SEPARATION_MODES`
pub fn separation_modes() -> HashMap<&'static str, Option<&'static str>> {
    HashMap::from([
        ("none", None),
        ("instrumental", Some("Instrumental")),
        ("vocals", Some("Vocals")),
    ])
}

/// Check if audio-separator is available — `is_audio_separator_available`
pub fn is_audio_separator_available() -> bool {
    // In the Python version, this checks for the audio_separator Python package.
    // In Rust, we'd check if the subprocess command is available.
    // For now, return false until the subprocess wrapper is implemented.
    false
}

/// List available separation models — `list_available_models`
pub fn list_available_models(_model_dir: &str) -> Vec<String> {
    // TODO: Port the model discovery logic from source_separation.py
    // This scans the model directory for .ckpt and .yaml files
    Vec::new()
}

/// Run source separation on audio data — `run_source_separation`
///
/// TODO: Port the full subprocess-based separation pipeline.
/// This involves:
/// 1. Writing audio data to a temp WAV file
/// 2. Running python-audio-separator as a subprocess
/// 3. Reading the separated audio back
/// 4. Resampling to match the original sample rate
#[allow(clippy::too_many_arguments)]
pub fn run_source_separation(
    _ref_pcm: &[f32],
    _tgt_pcm: &[f32],
    _sample_rate: i64,
    _mode: &str,
    _model: &str,
    log: &dyn Fn(&str),
    _device: &str,
    _timeout: i32,
    _model_dir: &str,
) -> (Vec<f32>, Vec<f32>) {
    log("[SOURCE SEPARATION] Not yet implemented in Rust port");
    log("[SOURCE SEPARATION] Using original audio for correlation");
    // Return empty vecs — caller should use originals
    (Vec::new(), Vec::new())
}
