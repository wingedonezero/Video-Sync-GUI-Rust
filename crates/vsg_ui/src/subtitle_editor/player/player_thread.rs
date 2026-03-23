//! Player thread — 1:1 port of `vsg_qt/subtitle_editor/player/player_thread.py`.
//!
//! Background thread for video frame decoding and playback timing.
//! Uses ffmpeg CLI pipe for frame extraction (matching the vsg_core approach).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Video player state shared between UI and player thread.
pub struct PlayerState {
    /// Current playback position in seconds.
    pub position: f64,
    /// Video duration in seconds.
    pub duration: f64,
    /// Whether playback is active.
    pub playing: Arc<AtomicBool>,
    /// Request to stop the player thread.
    pub stop_requested: Arc<AtomicBool>,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            position: 0.0,
            duration: 0.0,
            playing: Arc::new(AtomicBool::new(false)),
            stop_requested: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl PlayerState {
    /// Request the player to stop.
    pub fn request_stop(&self) {
        self.stop_requested.store(true, Ordering::Relaxed);
    }

    /// Check if stop was requested.
    pub fn should_stop(&self) -> bool {
        self.stop_requested.load(Ordering::Relaxed)
    }
}
