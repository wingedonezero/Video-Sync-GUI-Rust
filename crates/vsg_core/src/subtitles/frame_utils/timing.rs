//! Frame/time conversion functions for subtitle synchronization.
//!
//! Contains:
//! - CFR timing modes (floor, middle, aegisub)
//! - VFR (FPSTimestamps-based) timing
//! - VFR cache management
//!
//! 1:1 port of `vsg_core/subtitles/frame_utils/timing.py`.

use std::collections::HashMap;
use std::sync::Mutex;

use once_cell::sync::Lazy;

use crate::io::runner::CommandRunner;

// ============================================================================
// MODE 0: FRAME START (For Correlation-Frame-Snap - STABLE & DETERMINISTIC)
// ============================================================================

/// Convert timestamp to frame number using FLOOR with epsilon protection.
///
/// This gives the frame that is currently displaying at the given time.
/// This is the preferred method for sync math because:
/// - Deterministic (no rounding ambiguity at boundaries)
/// - Stable under floating point drift
/// - Maps to actual frame boundaries (frame N starts at N * frame_duration)
///
/// # Examples at 23.976 fps (frame_duration = 41.708ms):
/// ```text
/// time_to_frame_floor(0.0, 23.976) -> 0
/// time_to_frame_floor(41.707, 23.976) -> 0  (still in frame 0)
/// time_to_frame_floor(41.708, 23.976) -> 1  (frame 1 starts)
/// time_to_frame_floor(1000.999, 23.976) -> 23  (FP drift protected)
/// time_to_frame_floor(1001.0, 23.976) -> 24
/// ```
pub fn time_to_frame_floor(time_ms: f64, fps: f64) -> i64 {
    let frame_duration_ms = 1000.0 / fps;
    // Add small epsilon to protect against FP errors where time_ms is slightly under frame boundary
    let epsilon = 1e-6;
    ((time_ms + epsilon) / frame_duration_ms) as i64
}

/// Convert frame number to its START timestamp (exact, no rounding).
///
/// This is the preferred method for sync math because:
/// - Frame N starts at exactly N * frame_duration
/// - No rounding (exact calculation)
/// - Guarantees frame-aligned timing
pub fn frame_to_time_floor(frame_num: i64, fps: f64) -> f64 {
    let frame_duration_ms = 1000.0 / fps;
    frame_num as f64 * frame_duration_ms
}

// ============================================================================
// MODE 1: MIDDLE OF FRAME (Current Implementation)
// ============================================================================

/// Convert timestamp to frame number, accounting for +0.5 offset.
///
/// MODE: Middle of frame window.
pub fn time_to_frame_middle(time_ms: f64, fps: f64) -> i64 {
    let frame_duration_ms = 1000.0 / fps;
    (time_ms / frame_duration_ms - 0.5).round() as i64
}

/// Targets the middle of the frame's display window with +0.5 offset.
///
/// MODE: Middle of frame window.
///
/// Example at 23.976 fps:
/// - Frame 24 displays from 1001.001ms to 1042.709ms
/// - Calculation: 24.5 x 41.708 = 1022ms
/// - After centisecond rounding: 1020ms (safely in frame 24)
pub fn frame_to_time_middle(frame_num: i64, fps: f64) -> i64 {
    let frame_duration_ms = 1000.0 / fps;
    ((frame_num as f64 + 0.5) * frame_duration_ms).round() as i64
}

// ============================================================================
// MODE 2: AEGISUB-STYLE (Ceil to Centisecond)
// ============================================================================

/// Convert timestamp to frame using floor division (which frame is currently displaying).
///
/// MODE: Aegisub-style timing.
pub fn time_to_frame_aegisub(time_ms: f64, fps: f64) -> i64 {
    let frame_duration_ms = 1000.0 / fps;
    (time_ms / frame_duration_ms) as i64
}

/// Matches Aegisub's algorithm: Calculate exact frame start, then round UP
/// to the next centisecond to ensure timestamp falls within the frame.
///
/// MODE: Aegisub-style timing.
///
/// Example at 23.976 fps:
/// - Frame 24 starts at 1001.001ms
/// - Exact calculation: 24 x 41.708 = 1001.001ms
/// - Round UP to next centisecond: ceil(1001.001 / 10) x 10 = 1010ms
/// - Result: 1010ms (safely in frame 24: 1001-1043ms)
pub fn frame_to_time_aegisub(frame_num: i64, fps: f64) -> i64 {
    let frame_duration_ms = 1000.0 / fps;
    let exact_time_ms = frame_num as f64 * frame_duration_ms;

    // Round UP to next centisecond (ASS format precision)
    // This ensures the timestamp is guaranteed to fall within the frame
    let centiseconds = (exact_time_ms / 10.0).ceil() as i64;
    centiseconds * 10
}

// ============================================================================
// MODE 3: VFR (FPSTimestamps-based)
// ============================================================================

/// Represents a VFR/CFR timestamp handler.
///
/// In the Python version, this uses the `video_timestamps` library with
/// `FPSTimestamps` and `VideoTimestamps`. In Rust, we implement a lightweight
/// CFR timestamp calculator since we don't have a direct Rust equivalent.
#[derive(Debug, Clone)]
pub struct FpsTimestamps {
    /// FPS numerator
    pub fps_num: u64,
    /// FPS denominator
    pub fps_den: u64,
    /// Rounding method: "ROUND" or "FLOOR"
    pub rounding_method: String,
}

impl FpsTimestamps {
    /// Create a new FPSTimestamps handler for CFR content.
    pub fn new(fps_num: u64, fps_den: u64, rounding_method: &str) -> Self {
        Self {
            fps_num,
            fps_den,
            rounding_method: rounding_method.to_string(),
        }
    }

    /// Get the FPS as a float.
    pub fn fps(&self) -> f64 {
        self.fps_num as f64 / self.fps_den as f64
    }

    /// Convert frame number to time in milliseconds.
    pub fn frame_to_time(&self, frame_num: i64) -> i64 {
        let fps = self.fps();
        let exact_ms = frame_num as f64 * 1000.0 / fps;

        match self.rounding_method.as_str() {
            "FLOOR" => exact_ms.floor() as i64,
            _ => exact_ms.round() as i64, // "ROUND" or default
        }
    }

    /// Convert time in milliseconds to frame number.
    pub fn time_to_frame(&self, time_ms: i64) -> i64 {
        let fps = self.fps();
        let frame = time_ms as f64 * fps / 1000.0;

        match self.rounding_method.as_str() {
            "FLOOR" => frame.floor() as i64,
            _ => frame.round() as i64, // "ROUND" or default
        }
    }
}

/// Cache for FpsTimestamps instances to avoid re-parsing video.
/// Thread-safe: accessed from thread pool workers.
static VFR_CACHE: Lazy<Mutex<HashMap<String, FpsTimestamps>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Clear the VFR cache to release FpsTimestamps instances.
///
/// This should be called on application shutdown or when clearing resources.
pub fn clear_vfr_cache() {
    let mut cache = VFR_CACHE.lock().unwrap();
    cache.clear();
}

/// Get appropriate timestamp handler based on video type.
///
/// For CFR videos: Uses lightweight FpsTimestamps (just calculations).
///
/// # Arguments
/// * `video_path` - Path to video file
/// * `fps` - Frame rate
/// * `runner` - CommandRunner for logging
pub fn get_vfr_timestamps(
    video_path: &str,
    fps: f64,
    runner: &CommandRunner,
) -> Option<FpsTimestamps> {
    // Rounding method for VideoTimestamps (ROUND is the standard default)
    let rounding_str = "ROUND";

    // Create cache key that includes rounding method
    let cache_key = format!("{}_{}", video_path, rounding_str);

    // Thread-safe cache access
    {
        let cache = VFR_CACHE.lock().unwrap();
        if let Some(vts) = cache.get(&cache_key) {
            return Some(vts.clone());
        }
    }

    // Convert FPS to exact fraction for NTSC drop-frame rates
    // NTSC standards use fractional rates (N*1000/1001) to avoid color/audio drift
    let (fps_num, fps_den): (u64, u64) = if (fps - 23.976).abs() < 0.001 {
        (24000, 1001) // 23.976fps - NTSC film
    } else if (fps - 29.97).abs() < 0.01 {
        (30000, 1001) // 29.97fps - NTSC video
    } else if (fps - 59.94).abs() < 0.01 {
        (60000, 1001) // 59.94fps - NTSC high fps
    } else {
        // Use decimal FPS as fraction for non-NTSC rates (PAL, web video, etc.)
        let num = (fps * 1000.0) as u64;
        (num, 1000)
    };

    // Use FPSTimestamps for CFR (constant framerate) - lightweight!
    let vts = FpsTimestamps::new(fps_num, fps_den, rounding_str);

    runner.log_message(&format!(
        "[VideoTimestamps] Using FPSTimestamps for CFR video at {:.3} fps",
        fps
    ));
    runner.log_message(&format!(
        "[VideoTimestamps] RoundingMethod: {}",
        rounding_str
    ));

    // Thread-safe cache write
    {
        let mut cache = VFR_CACHE.lock().unwrap();
        cache.insert(cache_key, vts.clone());
    }

    Some(vts)
}
