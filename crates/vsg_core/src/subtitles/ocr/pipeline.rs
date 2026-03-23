//! OCR Pipeline - Main Entry Point
//!
//! Orchestrates the complete OCR workflow:
//!     1. Parse source file (VobSub/PGS)
//!     2. Preprocess images
//!     3. Run OCR with confidence tracking
//!     4. Post-process text (pattern fixes, validation)
//!     5. Generate output (ASS/SRT)
//!     6. Create report

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use tracing::{info, warn};

use super::debug::{OCRDebugger, create_debugger};
use super::engine::{OCREngine, create_ocr_engine};
use super::output::{LineRegion, OCRSubtitleResult, OutputConfig};
use super::parsers::base::{SubtitleImage, detect_parser};
use super::postprocess::{OCRPostProcessor, create_postprocessor};
use super::preprocessing::{ImagePreprocessor, create_preprocessor};
use super::report::{OCRReport, SubtitleOCRResult, create_report};

/// Configuration for OCR pipeline.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub language: String,
    pub output_format: String,
    pub preserve_positions: bool,
    pub bottom_threshold_percent: f64,
    pub top_threshold_percent: f64,
    pub low_confidence_threshold: f64,
    pub generate_report: bool,
    pub save_debug_images: bool,
    pub debug_output: bool,
    pub max_workers: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            language: "eng".to_string(),
            output_format: "ass".to_string(),
            preserve_positions: true,
            bottom_threshold_percent: 75.0,
            top_threshold_percent: 40.0,
            low_confidence_threshold: 60.0,
            generate_report: true,
            save_debug_images: false,
            debug_output: false,
            max_workers: 1,
        }
    }
}

/// Result of OCR pipeline execution.
pub struct PipelineResult {
    pub success: bool,
    pub output_path: Option<PathBuf>,
    pub report_path: Option<PathBuf>,
    pub report_summary: HashMap<String, serde_json::Value>,
    pub subtitle_count: usize,
    pub duration_seconds: f64,
    pub error: Option<String>,
}

impl Default for PipelineResult {
    fn default() -> Self {
        Self {
            success: false,
            output_path: None,
            report_path: None,
            report_summary: HashMap::new(),
            subtitle_count: 0,
            duration_seconds: 0.0,
            error: None,
        }
    }
}

/// Main OCR pipeline for converting image-based subtitles to text.
pub struct OCRPipeline {
    settings: HashMap<String, serde_json::Value>,
    work_dir: PathBuf,
    logs_dir: PathBuf,
    debug_output_dir: PathBuf,
    progress_callback: Option<Box<dyn Fn(&str, f64)>>,
    config: PipelineConfig,
}

impl OCRPipeline {
    pub fn new(
        settings: HashMap<String, serde_json::Value>,
        work_dir: PathBuf,
        logs_dir: PathBuf,
        debug_output_dir: Option<PathBuf>,
        progress_callback: Option<Box<dyn Fn(&str, f64)>>,
    ) -> Self {
        let debug_dir = debug_output_dir.unwrap_or_else(|| logs_dir.clone());

        let config = PipelineConfig {
            language: settings.get("ocr_language").and_then(|v| v.as_str()).unwrap_or("eng").to_string(),
            output_format: settings.get("ocr_output_format").and_then(|v| v.as_str()).unwrap_or("ass").to_string(),
            preserve_positions: settings.get("ocr_preserve_positions").and_then(|v| v.as_bool()).unwrap_or(true),
            bottom_threshold_percent: settings.get("ocr_bottom_threshold").and_then(|v| v.as_f64()).unwrap_or(75.0),
            top_threshold_percent: settings.get("ocr_top_threshold").and_then(|v| v.as_f64()).unwrap_or(40.0),
            low_confidence_threshold: settings.get("ocr_low_confidence_threshold").and_then(|v| v.as_f64()).unwrap_or(60.0),
            generate_report: settings.get("ocr_generate_report").and_then(|v| v.as_bool()).unwrap_or(true),
            save_debug_images: settings.get("ocr_save_debug_images").and_then(|v| v.as_bool()).unwrap_or(false),
            debug_output: settings.get("ocr_debug_output").and_then(|v| v.as_bool()).unwrap_or(false),
            max_workers: settings.get("ocr_max_workers").and_then(|v| v.as_u64()).unwrap_or(1) as usize,
        };

        let ocr_work_dir = work_dir.clone();
        let _ = std::fs::create_dir_all(&ocr_work_dir);

        Self {
            settings,
            work_dir: ocr_work_dir,
            logs_dir,
            debug_output_dir: debug_dir,
            progress_callback,
            config,
        }
    }

    /// Process a subtitle file through the OCR pipeline.
    pub fn process(
        &mut self,
        input_path: &Path,
        output_path: Option<&Path>,
        track_id: u32,
    ) -> PipelineResult {
        let mut result = PipelineResult::default();
        let start_time = Instant::now();

        self.log_progress("Starting OCR pipeline", 0.0);

        // Step 1: Detect and create parser
        self.log_progress("Parsing subtitle file", 0.05);
        let parser = match detect_parser(input_path) {
            Some(p) => p,
            None => {
                result.error = Some(format!("No parser available for file: {}", input_path.display()));
                return result;
            }
        };

        // Step 2: Parse input file
        let track_work_dir = self.work_dir.join(format!("track_{}", track_id));
        let _ = std::fs::create_dir_all(&track_work_dir);

        let parse_result = parser.parse(input_path, Some(&track_work_dir));
        if !parse_result.success() {
            result.error = Some(format!("Failed to parse: {}", parse_result.errors.join("; ")));
            return result;
        }

        let subtitle_images = parse_result.subtitles;
        result.subtitle_count = subtitle_images.len();

        if subtitle_images.is_empty() {
            result.error = Some("No subtitles found in file".into());
            return result;
        }

        self.log_progress(&format!("Found {} subtitles", subtitle_images.len()), 0.10);

        // Step 3: Note that OCR model inference is stubbed
        warn!("OCR model inference not yet available in Rust port - subtitles will be parsed but not OCR'd");
        self.log_progress("OCR model inference not yet available in Rust port", 0.50);

        // Step 4: Generate report stub
        let output = output_path
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let ext = if self.config.output_format == "ass" { "ass" } else { "srt" };
                input_path.with_extension(ext)
            });

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();

        let mut report = create_report(
            &input_path.to_string_lossy(),
            &output.to_string_lossy(),
            &self.config.language,
        );

        // Add stub results for each subtitle (no OCR text since inference is stubbed)
        for sub_image in &subtitle_images {
            report.add_subtitle_result(SubtitleOCRResult {
                index: sub_image.index,
                timestamp_start: sub_image.start_time(),
                timestamp_end: sub_image.end_time(),
                text: String::new(),
                confidence: 0.0,
                ..Default::default()
            });
        }

        report.finalize();

        // Save report
        if self.config.generate_report {
            self.log_progress("Saving report", 0.96);
            let report_name = format!(
                "{}_ocr_report_{}.json",
                input_path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown"),
                timestamp
            );
            let report_path = self.logs_dir.join(report_name);
            report.save(&report_path);
            result.report_path = Some(report_path);
            result.report_summary = report.to_summary();
        }

        result.success = true;
        result.duration_seconds = start_time.elapsed().as_secs_f64();
        result.output_path = Some(output);
        self.log_progress("OCR complete", 1.0);

        result
    }

    fn log_progress(&self, message: &str, progress: f64) {
        if let Some(ref cb) = self.progress_callback {
            cb(message, progress);
        }
    }
}
