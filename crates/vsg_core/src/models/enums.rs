//! Core enums — 1:1 port of `vsg_core/models/types.py`.
//!
//! Each enum maps to a Python `Literal[...]` type alias.
//! Serde rename values match the Python string values exactly.

use serde::{Deserialize, Serialize};

// ─── Track types ─────────────────────────────────────────────────────────────

/// Media track type — `TrackTypeStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackType {
    Video,
    Audio,
    Subtitles,
}

impl std::fmt::Display for TrackType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Video => write!(f, "video"),
            Self::Audio => write!(f, "audio"),
            Self::Subtitles => write!(f, "subtitles"),
        }
    }
}

// ─── Analysis mode ───────────────────────────────────────────────────────────

/// Analysis method — `AnalysisModeStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AnalysisMode {
    #[default]
    #[serde(rename = "Audio Correlation")]
    AudioCorrelation,
    #[serde(rename = "VideoDiff")]
    VideoDiff,
}

impl std::fmt::Display for AnalysisMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AudioCorrelation => write!(f, "Audio Correlation"),
            Self::VideoDiff => write!(f, "VideoDiff"),
        }
    }
}

// ─── Snap mode ───────────────────────────────────────────────────────────────

/// Chapter snap mode — `SnapModeStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SnapMode {
    #[default]
    Previous,
    Nearest,
}

impl std::fmt::Display for SnapMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Previous => write!(f, "previous"),
            Self::Nearest => write!(f, "nearest"),
        }
    }
}

// ─── Subtitle sync mode ─────────────────────────────────────────────────────

/// Subtitle sync method — `SubtitleSyncModeStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SubtitleSyncMode {
    #[default]
    TimeBased,
    VideoVerified,
}

impl std::fmt::Display for SubtitleSyncMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TimeBased => write!(f, "time-based"),
            Self::VideoVerified => write!(f, "video-verified"),
        }
    }
}

// ─── Subtitle rounding ──────────────────────────────────────────────────────

/// Subtitle rounding mode — `SubtitleRoundingStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubtitleRounding {
    #[default]
    Floor,
    Round,
    Ceil,
}

impl std::fmt::Display for SubtitleRounding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Floor => write!(f, "floor"),
            Self::Round => write!(f, "round"),
            Self::Ceil => write!(f, "ceil"),
        }
    }
}

// ─── Sync mode (timing direction) ───────────────────────────────────────────

/// Sync timing direction — `SyncModeStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncMode {
    #[default]
    PositiveOnly,
    AllowNegative,
}

impl std::fmt::Display for SyncMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PositiveOnly => write!(f, "positive_only"),
            Self::AllowNegative => write!(f, "allow_negative"),
        }
    }
}

// ─── Frame hash algorithm ────────────────────────────────────────────────────

/// Frame hash algorithm — `FrameHashAlgorithmStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrameHashAlgorithm {
    #[default]
    Dhash,
    Phash,
    #[serde(rename = "average_hash")]
    AverageHash,
    Whash,
}

impl std::fmt::Display for FrameHashAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dhash => write!(f, "dhash"),
            Self::Phash => write!(f, "phash"),
            Self::AverageHash => write!(f, "average_hash"),
            Self::Whash => write!(f, "whash"),
        }
    }
}

// ─── Frame comparison method ─────────────────────────────────────────────────

/// Frame comparison method — `FrameComparisonMethodStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrameComparisonMethod {
    #[default]
    Hash,
    Ssim,
    Mse,
}

impl std::fmt::Display for FrameComparisonMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hash => write!(f, "hash"),
            Self::Ssim => write!(f, "ssim"),
            Self::Mse => write!(f, "mse"),
        }
    }
}

// ─── Video-verified matching method ──────────────────────────────────────────

/// Video-verified matching method — `VideoVerifiedMethodStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoVerifiedMethod {
    #[default]
    Classic,
    Neural,
}

impl std::fmt::Display for VideoVerifiedMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Classic => write!(f, "classic"),
            Self::Neural => write!(f, "neural"),
        }
    }
}

// ─── Source separation ───────────────────────────────────────────────────────

/// Source separation mode — `SourceSeparationModeStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceSeparationMode {
    #[default]
    None,
    Instrumental,
    Vocals,
}

impl std::fmt::Display for SourceSeparationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Instrumental => write!(f, "instrumental"),
            Self::Vocals => write!(f, "vocals"),
        }
    }
}

/// Source separation device — `SourceSeparationDeviceStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceSeparationDevice {
    #[default]
    Auto,
    Cpu,
    Cuda,
    Rocm,
    Mps,
}

impl std::fmt::Display for SourceSeparationDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Cpu => write!(f, "cpu"),
            Self::Cuda => write!(f, "cuda"),
            Self::Rocm => write!(f, "rocm"),
            Self::Mps => write!(f, "mps"),
        }
    }
}

// ─── Filtering method ────────────────────────────────────────────────────────

/// Audio filtering method — `FilteringMethodStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FilteringMethod {
    #[serde(rename = "None")]
    NoFilter,
    #[serde(rename = "Low-Pass Filter")]
    LowPass,
    #[default]
    #[serde(rename = "Dialogue Band-Pass Filter")]
    DialogueBandPass,
}

impl std::fmt::Display for FilteringMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoFilter => write!(f, "None"),
            Self::LowPass => write!(f, "Low-Pass Filter"),
            Self::DialogueBandPass => write!(f, "Dialogue Band-Pass Filter"),
        }
    }
}

// ─── Correlation method ──────────────────────────────────────────────────────

/// Correlation algorithm — `CorrelationMethodStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CorrelationMethod {
    #[serde(rename = "Standard Correlation (SCC)")]
    Scc,
    #[default]
    #[serde(rename = "Phase Correlation (GCC-PHAT)")]
    GccPhat,
    #[serde(rename = "Onset Detection")]
    OnsetDetection,
    #[serde(rename = "GCC-SCOT")]
    GccScot,
    #[serde(rename = "Whitened Cross-Correlation")]
    Whitened,
    #[serde(rename = "Spectrogram Correlation")]
    Spectrogram,
    #[serde(rename = "VideoDiff")]
    VideoDiff,
}

impl std::fmt::Display for CorrelationMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scc => write!(f, "Standard Correlation (SCC)"),
            Self::GccPhat => write!(f, "Phase Correlation (GCC-PHAT)"),
            Self::OnsetDetection => write!(f, "Onset Detection"),
            Self::GccScot => write!(f, "GCC-SCOT"),
            Self::Whitened => write!(f, "Whitened Cross-Correlation"),
            Self::Spectrogram => write!(f, "Spectrogram Correlation"),
            Self::VideoDiff => write!(f, "VideoDiff"),
        }
    }
}

/// Correlation algorithm for source-separated audio — `CorrelationMethodSourceSepStr`
/// Same as `CorrelationMethod` but without `VideoDiff`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CorrelationMethodSourceSep {
    #[serde(rename = "Standard Correlation (SCC)")]
    Scc,
    #[default]
    #[serde(rename = "Phase Correlation (GCC-PHAT)")]
    GccPhat,
    #[serde(rename = "Onset Detection")]
    OnsetDetection,
    #[serde(rename = "GCC-SCOT")]
    GccScot,
    #[serde(rename = "Whitened Cross-Correlation")]
    Whitened,
    #[serde(rename = "Spectrogram Correlation")]
    Spectrogram,
}

impl std::fmt::Display for CorrelationMethodSourceSep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scc => write!(f, "Standard Correlation (SCC)"),
            Self::GccPhat => write!(f, "Phase Correlation (GCC-PHAT)"),
            Self::OnsetDetection => write!(f, "Onset Detection"),
            Self::GccScot => write!(f, "GCC-SCOT"),
            Self::Whitened => write!(f, "Whitened Cross-Correlation"),
            Self::Spectrogram => write!(f, "Spectrogram Correlation"),
        }
    }
}

// ─── Delay selection mode ────────────────────────────────────────────────────

/// Delay selection strategy — `DelaySelectionModeStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DelaySelectionMode {
    #[default]
    #[serde(rename = "Mode (Most Common)")]
    Mode,
    #[serde(rename = "Mode (Clustered)")]
    ModeClustered,
    #[serde(rename = "Mode (Early Cluster)")]
    ModeEarly,
    #[serde(rename = "First Stable")]
    FirstStable,
    #[serde(rename = "Average")]
    Average,
}

impl std::fmt::Display for DelaySelectionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mode => write!(f, "Mode (Most Common)"),
            Self::ModeClustered => write!(f, "Mode (Clustered)"),
            Self::ModeEarly => write!(f, "Mode (Early Cluster)"),
            Self::FirstStable => write!(f, "First Stable"),
            Self::Average => write!(f, "Average"),
        }
    }
}

// ─── Stepping correction ─────────────────────────────────────────────────────

/// Stepping correction mode — `SteppingCorrectionModeStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SteppingCorrectionMode {
    #[default]
    Full,
    Filtered,
    Strict,
    Disabled,
}

impl std::fmt::Display for SteppingCorrectionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full => write!(f, "full"),
            Self::Filtered => write!(f, "filtered"),
            Self::Strict => write!(f, "strict"),
            Self::Disabled => write!(f, "disabled"),
        }
    }
}

/// Stepping quality mode — `SteppingQualityModeStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SteppingQualityMode {
    Strict,
    #[default]
    Normal,
    Lenient,
    Custom,
}

impl std::fmt::Display for SteppingQualityMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Strict => write!(f, "strict"),
            Self::Normal => write!(f, "normal"),
            Self::Lenient => write!(f, "lenient"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

/// Filtered stepping fallback — `SteppingFilteredFallbackStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SteppingFilteredFallback {
    #[default]
    Nearest,
    Interpolate,
    Uniform,
    Skip,
    Reject,
}

impl std::fmt::Display for SteppingFilteredFallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nearest => write!(f, "nearest"),
            Self::Interpolate => write!(f, "interpolate"),
            Self::Uniform => write!(f, "uniform"),
            Self::Skip => write!(f, "skip"),
            Self::Reject => write!(f, "reject"),
        }
    }
}

/// Stepping boundary mode — `SteppingBoundaryModeStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SteppingBoundaryMode {
    #[default]
    Start,
    Majority,
    Midpoint,
}

impl std::fmt::Display for SteppingBoundaryMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Start => write!(f, "start"),
            Self::Majority => write!(f, "majority"),
            Self::Midpoint => write!(f, "midpoint"),
        }
    }
}

// ─── Resampling ──────────────────────────────────────────────────────────────

/// Resampling engine — `ResampleEngineStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResampleEngine {
    #[default]
    Aresample,
    Atempo,
    Rubberband,
}

impl std::fmt::Display for ResampleEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Aresample => write!(f, "aresample"),
            Self::Atempo => write!(f, "atempo"),
            Self::Rubberband => write!(f, "rubberband"),
        }
    }
}

/// Rubberband transient handling — `RubberbandTransientsStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RubberbandTransients {
    #[default]
    Crisp,
    Mixed,
    Smooth,
}

impl std::fmt::Display for RubberbandTransients {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Crisp => write!(f, "crisp"),
            Self::Mixed => write!(f, "mixed"),
            Self::Smooth => write!(f, "smooth"),
        }
    }
}

// ─── Sync stability ──────────────────────────────────────────────────────────

/// Outlier detection mode — `SyncStabilityOutlierModeStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncStabilityOutlierMode {
    Any,
    #[default]
    Threshold,
}

impl std::fmt::Display for SyncStabilityOutlierMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Any => write!(f, "any"),
            Self::Threshold => write!(f, "threshold"),
        }
    }
}

// ─── OCR ─────────────────────────────────────────────────────────────────────

/// OCR engine — `OcrEngineStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OcrEngine {
    #[default]
    Tesseract,
    Easyocr,
    Paddleocr,
}

impl std::fmt::Display for OcrEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tesseract => write!(f, "tesseract"),
            Self::Easyocr => write!(f, "easyocr"),
            Self::Paddleocr => write!(f, "paddleocr"),
        }
    }
}

/// OCR output format — `OcrOutputFormatStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OcrOutputFormat {
    #[default]
    Ass,
    Srt,
}

impl std::fmt::Display for OcrOutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ass => write!(f, "ass"),
            Self::Srt => write!(f, "srt"),
        }
    }
}

/// OCR binarization method — `OcrBinarizationMethodStr`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OcrBinarizationMethod {
    #[default]
    Otsu,
}

impl std::fmt::Display for OcrBinarizationMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Otsu => write!(f, "otsu"),
        }
    }
}

// ─── Job status ──────────────────────────────────────────────────────────────

/// Status of a completed job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Merged,
    Analyzed,
    Failed,
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Merged => write!(f, "merged"),
            Self::Analyzed => write!(f, "analyzed"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_mode_serde_round_trip() {
        let json = serde_json::to_string(&AnalysisMode::AudioCorrelation).unwrap();
        assert_eq!(json, "\"Audio Correlation\"");
        let parsed: AnalysisMode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, AnalysisMode::AudioCorrelation);
    }

    #[test]
    fn correlation_method_serde_round_trip() {
        let json = serde_json::to_string(&CorrelationMethod::GccPhat).unwrap();
        assert_eq!(json, "\"Phase Correlation (GCC-PHAT)\"");
        let parsed: CorrelationMethod = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, CorrelationMethod::GccPhat);
    }

    #[test]
    fn filtering_method_default_matches_python() {
        // Python default: "Dialogue Band-Pass Filter"
        assert_eq!(FilteringMethod::default(), FilteringMethod::DialogueBandPass);
    }

    #[test]
    fn sync_mode_serde() {
        let json = serde_json::to_string(&SyncMode::AllowNegative).unwrap();
        assert_eq!(json, "\"allow_negative\"");
    }

    #[test]
    fn subtitle_sync_mode_serde() {
        let json = serde_json::to_string(&SubtitleSyncMode::VideoVerified).unwrap();
        assert_eq!(json, "\"video-verified\"");
    }

    #[test]
    fn all_delay_selection_modes_serialize() {
        let modes = [
            (DelaySelectionMode::Mode, "Mode (Most Common)"),
            (DelaySelectionMode::ModeClustered, "Mode (Clustered)"),
            (DelaySelectionMode::ModeEarly, "Mode (Early Cluster)"),
            (DelaySelectionMode::FirstStable, "First Stable"),
            (DelaySelectionMode::Average, "Average"),
        ];
        for (mode, expected) in modes {
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(json, format!("\"{expected}\""));
        }
    }
}
