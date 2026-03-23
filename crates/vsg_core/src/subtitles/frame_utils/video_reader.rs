//! Video reader with FFmpeg pipe + opencv backends for frame extraction.
//!
//! Contains:
//! - VideoReader class with FFmpeg pipe and opencv::Mat support
//! - Frame extraction as grayscale images
//!
//! 1:1 port of `vsg_core/subtitles/frame_utils/video_reader.py`.
//! In the Python version, VapourSynth and FFMS2 are the preferred backends.
//! In Rust, we use FFmpeg subprocess for frame extraction and opencv for
//! image operations, since there are no VapourSynth/FFMS2 Rust bindings.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::io::runner::CommandRunner;

/// Efficient video reader that extracts frames via FFmpeg subprocess.
///
/// For the Rust port, we use FFmpeg pipe as the primary backend since
/// VapourSynth/FFMS2 don't have Rust bindings. Frames are extracted
/// as raw grayscale (Y plane) data for hashing.
pub struct VideoReader {
    pub video_path: String,
    pub fps: Option<f64>,
    pub temp_dir: Option<PathBuf>,
    pub is_interlaced: bool,
    pub field_order: String,
    pub is_vfr: bool,
    pub is_soft_telecine: bool,
    pub target_fps: Option<f64>,
    pub real_fps: Option<f64>,
    width: i32,
    height: i32,
    frame_count: i64,
    duration_ms: f64,
}

/// A decoded video frame as a grayscale image.
#[derive(Debug, Clone)]
pub struct VideoFrame {
    /// Raw grayscale pixel data (Y plane), row-major
    pub data: Vec<u8>,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
}

impl VideoFrame {
    /// Convert to an `image::GrayImage` for use with image crate operations.
    pub fn to_gray_image(&self) -> image::GrayImage {
        image::GrayImage::from_raw(self.width, self.height, self.data.clone())
            .unwrap_or_else(|| image::GrayImage::new(self.width, self.height))
    }

    /// Get dimensions as (width, height).
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Resize the frame to the given dimensions using nearest-neighbor.
    pub fn resize(&self, new_width: u32, new_height: u32) -> VideoFrame {
        let img = self.to_gray_image();
        let resized = image::imageops::resize(
            &img,
            new_width,
            new_height,
            image::imageops::FilterType::Lanczos3,
        );
        VideoFrame {
            data: resized.into_raw(),
            width: new_width,
            height: new_height,
        }
    }
}

impl VideoReader {
    /// Create a new VideoReader for the given video file.
    ///
    /// Detects video properties (FPS, resolution, interlacing) via ffprobe.
    pub fn new(
        video_path: &str,
        runner: &CommandRunner,
        temp_dir: Option<PathBuf>,
    ) -> Self {
        let mut reader = Self {
            video_path: video_path.to_string(),
            fps: None,
            temp_dir,
            is_interlaced: false,
            field_order: "progressive".to_string(),
            is_vfr: false,
            is_soft_telecine: false,
            target_fps: None,
            real_fps: None,
            width: 1920,
            height: 1080,
            frame_count: 0,
            duration_ms: 0.0,
        };

        reader.detect_properties(runner);
        reader
    }

    /// Detect video properties (FPS, interlacing metadata, VFR info).
    fn detect_properties(&mut self, runner: &CommandRunner) {
        let props = super::video_properties::detect_video_properties(&self.video_path, runner);

        self.is_interlaced = props
            .get("interlaced")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        self.field_order = props
            .get("field_order")
            .and_then(|v| v.as_str())
            .unwrap_or("progressive")
            .to_string();
        self.is_vfr = props
            .get("is_vfr")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        self.is_soft_telecine = props
            .get("is_soft_telecine")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        self.fps = props.get("fps").and_then(|v| v.as_f64());
        self.width = props.get("width").and_then(|v| v.as_i64()).unwrap_or(1920) as i32;
        self.height = props.get("height").and_then(|v| v.as_i64()).unwrap_or(1080) as i32;
        self.frame_count = props.get("frame_count").and_then(|v| v.as_i64()).unwrap_or(0);
        self.duration_ms = props.get("duration_ms").and_then(|v| v.as_f64()).unwrap_or(0.0);

        if self.is_soft_telecine {
            self.target_fps = props.get("original_fps").and_then(|v| v.as_f64());
            if self.target_fps.is_none() {
                self.target_fps = Some(23.976);
            }
            runner.log_message(&format!(
                "[FrameUtils] Soft-telecine detected: container={:.3}fps, original={:.3}fps",
                self.fps.unwrap_or(0.0),
                self.target_fps.unwrap_or(0.0)
            ));
        }

        runner.log_message(&format!(
            "[FrameUtils] Using FFmpeg for frame access (FPS: {:.3})",
            self.fps.unwrap_or(23.976)
        ));
    }

    /// Extract frame at specified timestamp as a grayscale image.
    ///
    /// Uses ffmpeg subprocess to extract a single frame at the given time.
    pub fn get_frame_at_time(&self, time_ms: i64) -> Option<VideoFrame> {
        self.extract_frame_ffmpeg(time_ms)
    }

    /// Extract frame by frame number directly.
    ///
    /// Converts frame number to time, then extracts via ffmpeg.
    pub fn get_frame_at_index(&self, frame_num: i64) -> Option<VideoFrame> {
        let fps = self.fps.unwrap_or(23.976);
        let time_ms = (frame_num as f64 * 1000.0 / fps) as i64;
        self.extract_frame_ffmpeg(time_ms)
    }

    /// Get the frame index closest to a given timestamp.
    ///
    /// For CFR content, uses simple division. For VFR, this is an approximation
    /// since we don't have VapourSynth's _AbsoluteTime in the Rust port.
    pub fn get_frame_index_for_time(&self, time_ms: f64) -> Option<i64> {
        let fps = self.real_fps.unwrap_or_else(|| self.fps.unwrap_or(23.976));
        Some((time_ms / (1000.0 / fps)) as i64)
    }

    /// Get the Presentation Time Stamp (PTS) of a frame in milliseconds.
    ///
    /// For CFR content, calculates from frame index * frame duration.
    pub fn get_frame_pts(&self, frame_num: i64) -> Option<f64> {
        let fps = self.fps?;
        Some(frame_num as f64 * 1000.0 / fps)
    }

    /// Get total frame count of the video.
    pub fn get_frame_count(&self) -> i64 {
        self.frame_count
    }

    /// Extract a single frame via ffmpeg pipe as raw grayscale data.
    ///
    /// Uses ffmpeg to seek to the timestamp and output a single raw gray8 frame.
    fn extract_frame_ffmpeg(&self, time_ms: i64) -> Option<VideoFrame> {
        let time_sec = time_ms as f64 / 1000.0;

        // Use a reasonable output size for hashing (downscale large videos)
        let out_width = if self.width > 640 { 320 } else { self.width };
        let out_height = if self.height > 480 { 240 } else { self.height };

        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-ss",
            &format!("{:.3}", time_sec),
            "-i",
            &self.video_path,
            "-vframes",
            "1",
            "-vf",
            &format!("scale={}:{},format=gray", out_width, out_height),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "gray",
            "-v",
            "quiet",
            "-",
        ]);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::null());

        let mut child = cmd.spawn().ok()?;
        let mut stdout = child.stdout.take()?;

        let expected_size = (out_width * out_height) as usize;
        let mut buffer = vec![0u8; expected_size];
        let bytes_read = stdout.read(&mut buffer).ok()?;

        let _ = child.wait();

        if bytes_read < expected_size {
            return None;
        }

        Some(VideoFrame {
            data: buffer,
            width: out_width as u32,
            height: out_height as u32,
        })
    }

    /// Release video resources.
    pub fn close(&mut self) {
        // FFmpeg subprocess-based reader has no persistent state to close.
        // This is provided for API compatibility with the Python version.
    }
}

/// Generate cache path for FFMS2 index in job's temp directory.
///
/// This is provided for compatibility with code that references this function
/// (e.g., neural_matcher.rs). In the Rust port, we don't use FFMS2 directly,
/// but the function is kept for API compatibility.
pub fn get_ffms2_cache_path(video_path: &str, temp_dir: Option<&Path>) -> PathBuf {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let video_path_obj = Path::new(video_path);

    // Build cache key from path components
    let parent_dir = video_path_obj
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let stem = video_path_obj
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    let cache_key = if parent_dir.is_empty() || parent_dir == "." {
        let mut hasher = DefaultHasher::new();
        video_path.hash(&mut hasher);
        let path_hash = format!("{:x}", hasher.finish());
        format!("{}_{}", stem, &path_hash[..8.min(path_hash.len())])
    } else {
        format!("{}_{}", parent_dir, stem)
    };

    let cache_dir = if let Some(td) = temp_dir {
        td.join("ffindex")
    } else {
        std::env::temp_dir().join("vsg_ffindex")
    };

    let _ = std::fs::create_dir_all(&cache_dir);

    cache_dir.join(format!("{}.ffindex", cache_key))
}
