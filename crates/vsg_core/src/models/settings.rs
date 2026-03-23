//! Application settings — 1:1 port of `vsg_core/models/settings.py`.
//!
//! THE SINGLE SOURCE OF TRUTH for all application settings.
//! All field names and defaults match the Python `AppSettings` class exactly.
//!
//! Settings are serialized as TOML with flat field names (no nested tables)
//! to maintain compatibility with the Python JSON field names.

use serde::{Deserialize, Serialize};

use super::enums::{
    AnalysisMode, CorrelationMethod, CorrelationMethodSourceSep, DelaySelectionMode,
    FilteringMethod, FrameComparisonMethod, FrameHashAlgorithm, OcrBinarizationMethod, OcrEngine,
    OcrOutputFormat, ResampleEngine, RubberbandTransients, SnapMode, SourceSeparationDevice,
    SourceSeparationMode, SteppingBoundaryMode, SteppingCorrectionMode, SteppingFilteredFallback,
    SteppingQualityMode, SubtitleRounding, SubtitleSyncMode, SyncMode,
    SyncStabilityOutlierMode, VideoVerifiedMethod,
};

/// Sentinel value for paths that need runtime resolution.
/// These will be resolved by the config manager based on the application directory.
pub const PATH_SENTINEL: &str = "__PATH_NEEDS_RESOLUTION__";

/// Complete application settings with typed fields and defaults.
///
/// All pipeline code should access settings through this struct.
/// Field names match the Python `AppSettings` class exactly for 1:1 compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    // ─── Path Settings ───────────────────────────────────────────────────────
    #[serde(default = "default_path_sentinel")]
    pub output_folder: String,
    #[serde(default = "default_path_sentinel")]
    pub temp_root: String,
    #[serde(default = "default_path_sentinel")]
    pub logs_folder: String,
    #[serde(default)]
    pub videodiff_path: String,
    #[serde(default)]
    pub fonts_directory: String,
    #[serde(default)]
    pub last_ref_path: String,
    #[serde(default)]
    pub last_sec_path: String,
    #[serde(default)]
    pub last_ter_path: String,
    #[serde(default = "default_path_sentinel")]
    pub source_separation_model_dir: String,

    // ─── Analysis Settings ───────────────────────────────────────────────────
    #[serde(default)]
    pub analysis_mode: AnalysisMode,
    #[serde(default)]
    pub analysis_lang_source1: String,
    #[serde(default)]
    pub analysis_lang_others: String,
    #[serde(default = "default_min_match_pct")]
    pub min_match_pct: f64,

    // Dense sliding window correlation (GPU)
    #[serde(default = "default_dense_window_s")]
    pub dense_window_s: f64,
    #[serde(default = "default_dense_hop_s")]
    pub dense_hop_s: f64,
    #[serde(default = "default_dense_silence_threshold_db")]
    pub dense_silence_threshold_db: f64,
    #[serde(default = "default_dense_outlier_threshold_ms")]
    pub dense_outlier_threshold_ms: f64,

    // VideoDiff settings
    #[serde(default)]
    pub videodiff_error_min: f64,
    #[serde(default = "default_100")]
    pub videodiff_error_max: f64,
    #[serde(default)]
    pub videodiff_sample_fps: f64,
    #[serde(default = "default_videodiff_match_threshold")]
    pub videodiff_match_threshold: i32,
    #[serde(default = "default_videodiff_min_matches")]
    pub videodiff_min_matches: i32,
    #[serde(default = "default_videodiff_inlier_threshold_ms")]
    pub videodiff_inlier_threshold_ms: f64,

    // ─── Chapter Settings ────────────────────────────────────────────────────
    #[serde(default)]
    pub rename_chapters: bool,
    #[serde(default)]
    pub snap_chapters: bool,
    #[serde(default)]
    pub snap_mode: SnapMode,
    #[serde(default = "default_snap_threshold_ms")]
    pub snap_threshold_ms: i32,
    #[serde(default = "default_true")]
    pub snap_starts_only: bool,

    // ─── Muxing Settings ─────────────────────────────────────────────────────
    #[serde(default)]
    pub apply_dialog_norm_gain: bool,
    #[serde(default)]
    pub disable_track_statistics_tags: bool,
    #[serde(default = "default_true")]
    pub disable_header_compression: bool,

    // ─── Post-Mux Settings ───────────────────────────────────────────────────
    #[serde(default)]
    pub post_mux_normalize_timestamps: bool,
    #[serde(default)]
    pub post_mux_strip_tags: bool,

    // ─── Logging Settings ────────────────────────────────────────────────────
    #[serde(default = "default_true")]
    pub log_compact: bool,
    #[serde(default = "default_true")]
    pub log_autoscroll: bool,
    #[serde(default = "default_log_error_tail")]
    pub log_error_tail: i32,
    #[serde(default)]
    pub log_tail_lines: i32,
    #[serde(default = "default_log_progress_step")]
    pub log_progress_step: i32,
    #[serde(default)]
    pub log_show_options_pretty: bool,
    #[serde(default)]
    pub log_show_options_json: bool,
    #[serde(default = "default_true")]
    pub log_audio_drift: bool,
    #[serde(default = "default_true")]
    pub archive_logs: bool,

    // ─── Timing Sync Settings ────────────────────────────────────────────────
    #[serde(default)]
    pub auto_apply_strict: bool,
    #[serde(default)]
    pub sync_mode: SyncMode,

    // ─── Segmented Audio Correction ──────────────────────────────────────────
    #[serde(default)]
    pub stepping_enabled: bool,

    // ─── Subtitle Sync Settings ──────────────────────────────────────────────
    #[serde(default)]
    pub subtitle_sync_mode: SubtitleSyncMode,
    #[serde(default)]
    pub time_based_use_raw_values: bool,
    #[serde(default = "default_true")]
    pub time_based_bypass_subtitle_data: bool,
    #[serde(default)]
    pub subtitle_rounding: SubtitleRounding,
    #[serde(default)]
    pub subtitle_target_fps: f64,

    // ─── Frame Matching Settings (video-verified classic) ────────────────────
    #[serde(default)]
    pub frame_hash_algorithm: FrameHashAlgorithm,
    #[serde(default = "default_8")]
    pub frame_hash_size: i32,
    #[serde(default = "default_5")]
    pub frame_hash_threshold: i32,
    #[serde(default = "default_5")]
    pub frame_window_radius: i32,
    #[serde(default)]
    pub frame_comparison_method: FrameComparisonMethod,
    #[serde(default = "default_10")]
    pub frame_ssim_threshold: i32,

    // ─── Video-Verified Sync Settings ────────────────────────────────────────
    #[serde(default = "default_3")]
    pub video_verified_zero_check_frames: i32,
    #[serde(default = "default_video_verified_min_quality_advantage")]
    pub video_verified_min_quality_advantage: f64,
    #[serde(default = "default_9")]
    pub video_verified_num_checkpoints: i32,
    #[serde(default = "default_3")]
    pub video_verified_search_range_frames: i32,
    #[serde(default = "default_10")]
    pub video_verified_sequence_length: i32,
    #[serde(default)]
    pub video_verified_use_pts_precision: bool,
    #[serde(default)]
    pub video_verified_frame_audit: bool,
    #[serde(default)]
    pub video_verified_visual_verify: bool,
    #[serde(default)]
    pub video_verified_method: VideoVerifiedMethod,

    // Neural feature matching
    #[serde(default = "default_10")]
    pub neural_window_seconds: i32,
    #[serde(default = "default_5")]
    pub neural_slide_range_seconds: i32,
    #[serde(default = "default_9")]
    pub neural_num_positions: i32,
    #[serde(default = "default_32")]
    pub neural_batch_size: i32,
    #[serde(default = "default_true")]
    pub neural_run_in_subprocess: bool,
    #[serde(default)]
    pub neural_debug_report: bool,

    // ─── Analysis/Correlation Settings ───────────────────────────────────────
    #[serde(default)]
    pub source_separation_mode: SourceSeparationMode,
    #[serde(default = "default_source_separation_model")]
    pub source_separation_model: String,
    #[serde(default)]
    pub source_separation_device: SourceSeparationDevice,
    #[serde(default = "default_source_separation_timeout")]
    pub source_separation_timeout: i32,
    #[serde(default)]
    pub filtering_method: FilteringMethod,
    #[serde(default)]
    pub correlation_method: CorrelationMethod,
    #[serde(default)]
    pub correlation_method_source_separated: CorrelationMethodSourceSep,

    // Delay selection
    #[serde(default)]
    pub delay_selection_mode: DelaySelectionMode,
    #[serde(default = "default_delay_selection_mode_source_separated")]
    pub delay_selection_mode_source_separated: DelaySelectionMode,
    #[serde(default = "default_min_accepted_pct")]
    pub min_accepted_pct: f64,
    #[serde(default = "default_15")]
    pub first_stable_early_pct: f64,
    #[serde(default = "default_15")]
    pub early_cluster_early_pct: f64,
    #[serde(default = "default_early_cluster_min_presence_pct")]
    pub early_cluster_min_presence_pct: f64,

    // Multi-correlation comparison
    #[serde(default)]
    pub multi_correlation_enabled: bool,
    #[serde(default = "default_true")]
    pub multi_corr_scc: bool,
    #[serde(default = "default_true")]
    pub multi_corr_gcc_phat: bool,
    #[serde(default)]
    pub multi_corr_onset: bool,
    #[serde(default)]
    pub multi_corr_gcc_scot: bool,
    #[serde(default)]
    pub multi_corr_gcc_whiten: bool,
    #[serde(default)]
    pub multi_corr_spectrogram: bool,

    // DSP & filtering
    #[serde(default = "default_filter_bandpass_lowcut_hz")]
    pub filter_bandpass_lowcut_hz: f64,
    #[serde(default = "default_filter_bandpass_highcut_hz")]
    pub filter_bandpass_highcut_hz: f64,
    #[serde(default = "default_5")]
    pub filter_bandpass_order: i32,
    #[serde(default = "default_filter_lowpass_taps")]
    pub filter_lowpass_taps: i32,
    #[serde(default)]
    pub scan_start_percentage: f64,
    #[serde(default = "default_100")]
    pub scan_end_percentage: f64,
    #[serde(default)]
    pub use_soxr: bool,
    #[serde(default)]
    pub audio_decode_native: bool,
    #[serde(default)]
    pub audio_peak_fit: bool,
    #[serde(default)]
    pub audio_bandlimit_hz: i32,

    // Drift detection
    #[serde(default = "default_detection_dbscan_epsilon_ms")]
    pub detection_dbscan_epsilon_ms: f64,
    #[serde(default = "default_detection_dbscan_min_samples_pct")]
    pub detection_dbscan_min_samples_pct: f64,
    #[serde(default = "default_drift_r2")]
    pub drift_detection_r2_threshold: f64,
    #[serde(default = "default_drift_r2_lossless")]
    pub drift_detection_r2_threshold_lossless: f64,
    #[serde(default = "default_drift_slope_lossy")]
    pub drift_detection_slope_threshold_lossy: f64,
    #[serde(default = "default_drift_slope_lossless")]
    pub drift_detection_slope_threshold_lossless: f64,

    // ─── Stepping Correction Settings ────────────────────────────────────────
    #[serde(default = "default_true")]
    pub stepping_adjust_subtitles: bool,
    #[serde(default = "default_true")]
    pub stepping_adjust_subtitles_no_audio: bool,
    #[serde(default)]
    pub stepping_boundary_mode: SteppingBoundaryMode,
    #[serde(default = "default_stepping_triage_std_dev_ms")]
    pub stepping_triage_std_dev_ms: i32,

    // Boundary refinement — silence detection
    #[serde(default = "default_stepping_silence_search_window_s")]
    pub stepping_silence_search_window_s: f64,
    #[serde(default = "default_stepping_silence_threshold_db")]
    pub stepping_silence_threshold_db: f64,
    #[serde(default = "default_stepping_silence_min_duration_ms")]
    pub stepping_silence_min_duration_ms: f64,

    // Boundary refinement — VAD
    #[serde(default = "default_true")]
    pub stepping_vad_enabled: bool,
    #[serde(default = "default_stepping_vad_aggressiveness")]
    pub stepping_vad_aggressiveness: i32,

    // Boundary refinement — transient detection
    #[serde(default = "default_true")]
    pub stepping_transient_detection_enabled: bool,
    #[serde(default = "default_stepping_transient_threshold")]
    pub stepping_transient_threshold: f64,

    // Boundary refinement — scoring weights
    #[serde(default = "default_stepping_fusion_weight_silence")]
    pub stepping_fusion_weight_silence: i32,
    #[serde(default = "default_stepping_fusion_weight_duration")]
    pub stepping_fusion_weight_duration: i32,

    // Boundary refinement — video keyframe snapping
    #[serde(default)]
    pub stepping_snap_to_video_frames: bool,
    #[serde(default = "default_stepping_video_snap_max_offset_s")]
    pub stepping_video_snap_max_offset_s: f64,

    // Track naming
    #[serde(default)]
    pub stepping_corrected_track_label: String,
    #[serde(default)]
    pub stepping_preserved_track_label: String,

    // Filtered stepping correction
    #[serde(default)]
    pub stepping_correction_mode: SteppingCorrectionMode,
    #[serde(default)]
    pub stepping_quality_mode: SteppingQualityMode,
    #[serde(default = "default_stepping_min_cluster_percentage")]
    pub stepping_min_cluster_percentage: f64,
    #[serde(default = "default_stepping_min_cluster_duration_s")]
    pub stepping_min_cluster_duration_s: f64,
    #[serde(default = "default_stepping_min_match_quality_pct")]
    pub stepping_min_match_quality_pct: f64,
    #[serde(default = "default_stepping_min_total_clusters")]
    pub stepping_min_total_clusters: i32,
    #[serde(default)]
    pub stepping_filtered_fallback: SteppingFilteredFallback,
    #[serde(default = "default_true")]
    pub stepping_diagnostics_verbose: bool,

    // Stepping QA
    #[serde(default = "default_stepping_qa_threshold")]
    pub stepping_qa_threshold: f64,
    #[serde(default = "default_stepping_qa_min_accepted_pct")]
    pub stepping_qa_min_accepted_pct: f64,

    // ─── Sync Stability Settings ─────────────────────────────────────────────
    #[serde(default = "default_true")]
    pub sync_stability_enabled: bool,
    #[serde(default = "default_sync_stability_variance_threshold")]
    pub sync_stability_variance_threshold: f64,
    #[serde(default = "default_3")]
    pub sync_stability_min_windows: i32,
    #[serde(default)]
    pub sync_stability_outlier_mode: SyncStabilityOutlierMode,
    #[serde(default = "default_sync_stability_outlier_threshold")]
    pub sync_stability_outlier_threshold: f64,

    // ─── Resampling Engine Settings ──────────────────────────────────────────
    #[serde(default)]
    pub segment_resample_engine: ResampleEngine,
    #[serde(default)]
    pub segment_rb_pitch_correct: bool,
    #[serde(default)]
    pub segment_rb_transients: RubberbandTransients,
    #[serde(default = "default_true")]
    pub segment_rb_smoother: bool,
    #[serde(default = "default_true")]
    pub segment_rb_pitchq: bool,

    // ─── OCR Settings ────────────────────────────────────────────────────────
    #[serde(default = "default_true")]
    pub ocr_enabled: bool,
    #[serde(default)]
    pub ocr_engine: OcrEngine,
    #[serde(default = "default_ocr_language")]
    pub ocr_language: String,
    #[serde(default = "default_ocr_psm")]
    pub ocr_psm: i32,
    #[serde(default)]
    pub ocr_char_whitelist: String,
    #[serde(default = "default_ocr_char_blacklist")]
    pub ocr_char_blacklist: String,
    #[serde(default = "default_ocr_low_confidence_threshold")]
    pub ocr_low_confidence_threshold: f64,
    #[serde(default = "default_true")]
    pub ocr_multi_pass: bool,
    #[serde(default)]
    pub ocr_output_format: OcrOutputFormat,

    // OCR preprocessing
    #[serde(default = "default_true")]
    pub ocr_preprocess_auto: bool,
    #[serde(default = "default_ocr_upscale_threshold")]
    pub ocr_upscale_threshold: i32,
    #[serde(default = "default_ocr_target_height")]
    pub ocr_target_height: i32,
    #[serde(default = "default_ocr_border_size")]
    pub ocr_border_size: i32,
    #[serde(default)]
    pub ocr_force_binarization: bool,
    #[serde(default)]
    pub ocr_binarization_method: OcrBinarizationMethod,
    #[serde(default)]
    pub ocr_denoise: bool,
    #[serde(default)]
    pub ocr_save_debug_images: bool,

    // OCR output & position
    #[serde(default = "default_true")]
    pub ocr_preserve_positions: bool,
    #[serde(default = "default_ocr_bottom_threshold")]
    pub ocr_bottom_threshold: f64,
    #[serde(default)]
    pub ocr_video_width: i32,
    #[serde(default)]
    pub ocr_video_height: i32,

    // OCR post-processing
    #[serde(default = "default_true")]
    pub ocr_cleanup_enabled: bool,
    #[serde(default)]
    pub ocr_custom_wordlist_path: String,

    // OCR debug & runtime
    #[serde(default)]
    pub ocr_debug_output: bool,
    #[serde(default = "default_true")]
    pub ocr_run_in_subprocess: bool,
    #[serde(default = "default_ocr_font_size_ratio")]
    pub ocr_font_size_ratio: f64,
    #[serde(default = "default_true")]
    pub ocr_generate_report: bool,
    #[serde(default = "default_ocr_max_workers")]
    pub ocr_max_workers: i32,
}

// ─── Default value functions ─────────────────────────────────────────────────
// Named to match the Python defaults exactly.

fn default_path_sentinel() -> String {
    PATH_SENTINEL.to_string()
}

fn default_true() -> bool {
    true
}

// Analysis
fn default_min_match_pct() -> f64 {
    10.0
}
fn default_dense_window_s() -> f64 {
    10.0
}
fn default_dense_hop_s() -> f64 {
    2.0
}
fn default_dense_silence_threshold_db() -> f64 {
    -60.0
}
fn default_dense_outlier_threshold_ms() -> f64 {
    50.0
}
fn default_100() -> f64 {
    100.0
}
fn default_videodiff_match_threshold() -> i32 {
    5
}
fn default_videodiff_min_matches() -> i32 {
    50
}
fn default_videodiff_inlier_threshold_ms() -> f64 {
    100.0
}

// Chapters
fn default_snap_threshold_ms() -> i32 {
    250
}

// Logging
fn default_log_error_tail() -> i32 {
    20
}
fn default_log_progress_step() -> i32 {
    20
}

// Frame matching
fn default_3() -> i32 {
    3
}
fn default_5() -> i32 {
    5
}
fn default_8() -> i32 {
    8
}
fn default_9() -> i32 {
    9
}
fn default_10() -> i32 {
    10
}
fn default_32() -> i32 {
    32
}

// Video verified
fn default_video_verified_min_quality_advantage() -> f64 {
    0.1
}

// Source separation
fn default_source_separation_model() -> String {
    "default".to_string()
}
fn default_source_separation_timeout() -> i32 {
    900
}

// Delay selection
fn default_delay_selection_mode_source_separated() -> DelaySelectionMode {
    DelaySelectionMode::ModeClustered
}
fn default_min_accepted_pct() -> f64 {
    5.0
}
fn default_15() -> f64 {
    15.0
}
fn default_early_cluster_min_presence_pct() -> f64 {
    10.0
}

// DSP
fn default_filter_bandpass_lowcut_hz() -> f64 {
    300.0
}
fn default_filter_bandpass_highcut_hz() -> f64 {
    3400.0
}
fn default_filter_lowpass_taps() -> i32 {
    101
}

// Drift detection
fn default_detection_dbscan_epsilon_ms() -> f64 {
    20.0
}
fn default_detection_dbscan_min_samples_pct() -> f64 {
    1.5
}
fn default_drift_r2() -> f64 {
    0.90
}
fn default_drift_r2_lossless() -> f64 {
    0.95
}
fn default_drift_slope_lossy() -> f64 {
    0.7
}
fn default_drift_slope_lossless() -> f64 {
    0.2
}

// Stepping
fn default_stepping_triage_std_dev_ms() -> i32 {
    50
}
fn default_stepping_silence_search_window_s() -> f64 {
    5.0
}
fn default_stepping_silence_threshold_db() -> f64 {
    -40.0
}
fn default_stepping_silence_min_duration_ms() -> f64 {
    100.0
}
fn default_stepping_vad_aggressiveness() -> i32 {
    2
}
fn default_stepping_transient_threshold() -> f64 {
    8.0
}
fn default_stepping_fusion_weight_silence() -> i32 {
    10
}
fn default_stepping_fusion_weight_duration() -> i32 {
    2
}
fn default_stepping_video_snap_max_offset_s() -> f64 {
    2.0
}
fn default_stepping_min_cluster_percentage() -> f64 {
    5.0
}
fn default_stepping_min_cluster_duration_s() -> f64 {
    20.0
}
fn default_stepping_min_match_quality_pct() -> f64 {
    85.0
}
fn default_stepping_min_total_clusters() -> i32 {
    2
}
fn default_stepping_qa_threshold() -> f64 {
    85.0
}
fn default_stepping_qa_min_accepted_pct() -> f64 {
    90.0
}

// Sync stability
fn default_sync_stability_variance_threshold() -> f64 {
    1.0
}
fn default_sync_stability_outlier_threshold() -> f64 {
    1.0
}

// OCR
fn default_ocr_language() -> String {
    "eng".to_string()
}
fn default_ocr_psm() -> i32 {
    7
}
fn default_ocr_char_blacklist() -> String {
    "|".to_string()
}
fn default_ocr_low_confidence_threshold() -> f64 {
    60.0
}
fn default_ocr_upscale_threshold() -> i32 {
    40
}
fn default_ocr_target_height() -> i32 {
    80
}
fn default_ocr_border_size() -> i32 {
    5
}
fn default_ocr_bottom_threshold() -> f64 {
    75.0
}
fn default_ocr_font_size_ratio() -> f64 {
    5.80
}
fn default_ocr_max_workers() -> i32 {
    1
}

impl Default for AppSettings {
    fn default() -> Self {
        // Use TOML round-trip to ensure serde defaults are applied consistently
        toml::from_str("").expect("empty TOML should produce valid defaults")
    }
}

impl AppSettings {
    /// Get all field names as a set (mirrors Python's `get_field_names()`).
    pub fn field_names() -> &'static [&'static str] {
        &[
            "output_folder",
            "temp_root",
            "logs_folder",
            "videodiff_path",
            "fonts_directory",
            "last_ref_path",
            "last_sec_path",
            "last_ter_path",
            "source_separation_model_dir",
            "analysis_mode",
            "analysis_lang_source1",
            "analysis_lang_others",
            "min_match_pct",
            "dense_window_s",
            "dense_hop_s",
            "dense_silence_threshold_db",
            "dense_outlier_threshold_ms",
            "videodiff_error_min",
            "videodiff_error_max",
            "videodiff_sample_fps",
            "videodiff_match_threshold",
            "videodiff_min_matches",
            "videodiff_inlier_threshold_ms",
            "rename_chapters",
            "snap_chapters",
            "snap_mode",
            "snap_threshold_ms",
            "snap_starts_only",
            "apply_dialog_norm_gain",
            "disable_track_statistics_tags",
            "disable_header_compression",
            "post_mux_normalize_timestamps",
            "post_mux_strip_tags",
            "log_compact",
            "log_autoscroll",
            "log_error_tail",
            "log_tail_lines",
            "log_progress_step",
            "log_show_options_pretty",
            "log_show_options_json",
            "log_audio_drift",
            "archive_logs",
            "auto_apply_strict",
            "sync_mode",
            "stepping_enabled",
            "subtitle_sync_mode",
            "time_based_use_raw_values",
            "time_based_bypass_subtitle_data",
            "subtitle_rounding",
            "subtitle_target_fps",
            "frame_hash_algorithm",
            "frame_hash_size",
            "frame_hash_threshold",
            "frame_window_radius",
            "frame_comparison_method",
            "frame_ssim_threshold",
            "video_verified_zero_check_frames",
            "video_verified_min_quality_advantage",
            "video_verified_num_checkpoints",
            "video_verified_search_range_frames",
            "video_verified_sequence_length",
            "video_verified_use_pts_precision",
            "video_verified_frame_audit",
            "video_verified_visual_verify",
            "video_verified_method",
            "neural_window_seconds",
            "neural_slide_range_seconds",
            "neural_num_positions",
            "neural_batch_size",
            "neural_run_in_subprocess",
            "neural_debug_report",
            "source_separation_mode",
            "source_separation_model",
            "source_separation_device",
            "source_separation_timeout",
            "filtering_method",
            "correlation_method",
            "correlation_method_source_separated",
            "delay_selection_mode",
            "delay_selection_mode_source_separated",
            "min_accepted_pct",
            "first_stable_early_pct",
            "early_cluster_early_pct",
            "early_cluster_min_presence_pct",
            "multi_correlation_enabled",
            "multi_corr_scc",
            "multi_corr_gcc_phat",
            "multi_corr_onset",
            "multi_corr_gcc_scot",
            "multi_corr_gcc_whiten",
            "multi_corr_spectrogram",
            "filter_bandpass_lowcut_hz",
            "filter_bandpass_highcut_hz",
            "filter_bandpass_order",
            "filter_lowpass_taps",
            "scan_start_percentage",
            "scan_end_percentage",
            "use_soxr",
            "audio_decode_native",
            "audio_peak_fit",
            "audio_bandlimit_hz",
            "detection_dbscan_epsilon_ms",
            "detection_dbscan_min_samples_pct",
            "drift_detection_r2_threshold",
            "drift_detection_r2_threshold_lossless",
            "drift_detection_slope_threshold_lossy",
            "drift_detection_slope_threshold_lossless",
            "stepping_adjust_subtitles",
            "stepping_adjust_subtitles_no_audio",
            "stepping_boundary_mode",
            "stepping_triage_std_dev_ms",
            "stepping_silence_search_window_s",
            "stepping_silence_threshold_db",
            "stepping_silence_min_duration_ms",
            "stepping_vad_enabled",
            "stepping_vad_aggressiveness",
            "stepping_transient_detection_enabled",
            "stepping_transient_threshold",
            "stepping_fusion_weight_silence",
            "stepping_fusion_weight_duration",
            "stepping_snap_to_video_frames",
            "stepping_video_snap_max_offset_s",
            "stepping_corrected_track_label",
            "stepping_preserved_track_label",
            "stepping_correction_mode",
            "stepping_quality_mode",
            "stepping_min_cluster_percentage",
            "stepping_min_cluster_duration_s",
            "stepping_min_match_quality_pct",
            "stepping_min_total_clusters",
            "stepping_filtered_fallback",
            "stepping_diagnostics_verbose",
            "stepping_qa_threshold",
            "stepping_qa_min_accepted_pct",
            "sync_stability_enabled",
            "sync_stability_variance_threshold",
            "sync_stability_min_windows",
            "sync_stability_outlier_mode",
            "sync_stability_outlier_threshold",
            "segment_resample_engine",
            "segment_rb_pitch_correct",
            "segment_rb_transients",
            "segment_rb_smoother",
            "segment_rb_pitchq",
            "ocr_enabled",
            "ocr_engine",
            "ocr_language",
            "ocr_psm",
            "ocr_char_whitelist",
            "ocr_char_blacklist",
            "ocr_low_confidence_threshold",
            "ocr_multi_pass",
            "ocr_output_format",
            "ocr_preprocess_auto",
            "ocr_upscale_threshold",
            "ocr_target_height",
            "ocr_border_size",
            "ocr_force_binarization",
            "ocr_binarization_method",
            "ocr_denoise",
            "ocr_save_debug_images",
            "ocr_preserve_positions",
            "ocr_bottom_threshold",
            "ocr_video_width",
            "ocr_video_height",
            "ocr_cleanup_enabled",
            "ocr_custom_wordlist_path",
            "ocr_debug_output",
            "ocr_run_in_subprocess",
            "ocr_font_size_ratio",
            "ocr_generate_report",
            "ocr_max_workers",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_creates_successfully() {
        let settings = AppSettings::default();
        assert_eq!(settings.output_folder, PATH_SENTINEL);
        assert_eq!(settings.analysis_mode, AnalysisMode::AudioCorrelation);
    }

    #[test]
    fn default_values_match_python() {
        let s = AppSettings::default();

        // Path defaults
        assert_eq!(s.output_folder, PATH_SENTINEL);
        assert_eq!(s.videodiff_path, "");

        // Analysis defaults
        assert_eq!(s.min_match_pct, 10.0);
        assert_eq!(s.dense_window_s, 10.0);
        assert_eq!(s.dense_hop_s, 2.0);

        // Correlation defaults (Python: GCC-PHAT)
        assert_eq!(s.correlation_method, CorrelationMethod::GccPhat);
        assert_eq!(s.filtering_method, FilteringMethod::DialogueBandPass);

        // Sync defaults
        assert_eq!(s.sync_mode, SyncMode::PositiveOnly);
        assert_eq!(s.subtitle_sync_mode, SubtitleSyncMode::TimeBased);

        // DSP defaults
        assert_eq!(s.use_soxr, false);
        assert_eq!(s.audio_peak_fit, false);
        assert_eq!(s.scan_start_percentage, 0.0);
        assert_eq!(s.scan_end_percentage, 100.0);

        // Logging defaults
        assert_eq!(s.log_compact, true);
        assert_eq!(s.log_progress_step, 20);
        assert_eq!(s.archive_logs, true);

        // Chapter defaults
        assert_eq!(s.rename_chapters, false);
        assert_eq!(s.snap_mode, SnapMode::Previous);
        assert_eq!(s.snap_threshold_ms, 250);

        // OCR defaults
        assert_eq!(s.ocr_engine, OcrEngine::Tesseract);
        assert_eq!(s.ocr_language, "eng");
        assert_eq!(s.ocr_max_workers, 1);
    }

    #[test]
    fn settings_toml_round_trip() {
        let settings = AppSettings::default();
        let toml_str = toml::to_string_pretty(&settings).unwrap();
        let parsed: AppSettings = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.correlation_method, settings.correlation_method);
        assert_eq!(parsed.dense_window_s, settings.dense_window_s);
        assert_eq!(parsed.ocr_engine, settings.ocr_engine);
    }

    #[test]
    fn missing_fields_get_defaults() {
        let minimal = "min_match_pct = 42.0";
        let parsed: AppSettings = toml::from_str(minimal).unwrap();
        assert_eq!(parsed.min_match_pct, 42.0);
        // Everything else gets defaults
        assert_eq!(parsed.correlation_method, CorrelationMethod::GccPhat);
        assert_eq!(parsed.log_compact, true);
        assert_eq!(parsed.ocr_enabled, true);
    }

    #[test]
    fn field_names_count_matches_python() {
        // Python has ~160 fields (excluding PATH_SENTINEL ClassVar)
        let names = AppSettings::field_names();
        assert!(names.len() >= 155, "Expected ~160 fields, got {}", names.len());
    }

    #[test]
    fn json_compatibility() {
        // Settings should also serialize/deserialize from JSON
        // (needed for job queue and subprocess communication)
        let settings = AppSettings::default();
        let json = serde_json::to_string_pretty(&settings).unwrap();
        let parsed: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.correlation_method, settings.correlation_method);
    }
}
