//! Adaptive Image Preprocessing for OCR
//!
//! Prepares subtitle images for optimal OCR accuracy.
//!
//! Key preprocessing steps:
//!     1. Convert to grayscale
//!     2. Ensure black text on white background
//!     3. Upscale small images
//!     4. Add white border for better recognition
//!     5. Optional: Binarization (Otsu thresholding)

use std::path::{Path, PathBuf};
use std::collections::HashMap;

use image::{GrayImage, RgbaImage, Luma, imageops};
use tracing::debug;

use super::parsers::SubtitleImage;

/// Configuration for preprocessing pipeline.
#[derive(Debug, Clone)]
pub struct PreprocessingConfig {
    /// Auto-detect vs forced settings
    pub auto_detect: bool,
    /// Upscale if height < this
    pub upscale_threshold_height: u32,
    /// Target height after upscaling
    pub target_height: u32,
    /// White border in pixels
    pub border_size: u32,
    /// Whether to always binarize
    pub force_binarization: bool,
    /// Binarization method: "otsu", "adaptive", "none"
    pub binarization_method: String,
    /// Denoising enabled
    pub denoise: bool,
    /// Denoise kernel half-size
    pub denoise_strength: u32,
    /// Save debug images
    pub save_debug_images: bool,
    /// Debug output directory
    pub debug_dir: Option<PathBuf>,
}

impl Default for PreprocessingConfig {
    fn default() -> Self {
        Self {
            auto_detect: true,
            upscale_threshold_height: 40,
            target_height: 80,
            border_size: 10,
            force_binarization: true,
            binarization_method: "otsu".to_string(),
            denoise: false,
            denoise_strength: 3,
            save_debug_images: false,
            debug_dir: None,
        }
    }
}

/// Result of preprocessing a subtitle image.
pub struct PreprocessedImage {
    /// Preprocessed image (grayscale)
    pub image: GrayImage,
    /// Original image for reference
    pub original: RgbaImage,
    /// Subtitle index
    pub subtitle_index: usize,
    pub was_inverted: bool,
    pub was_upscaled: bool,
    pub was_binarized: bool,
    pub scale_factor: f64,
    pub debug_path: Option<PathBuf>,
}

/// Adaptive preprocessing pipeline for subtitle images.
pub struct ImagePreprocessor {
    pub config: PreprocessingConfig,
}

impl ImagePreprocessor {
    pub fn new(config: PreprocessingConfig) -> Self {
        Self { config }
    }

    /// Preprocess a subtitle image for OCR.
    pub fn preprocess(
        &self,
        subtitle: &SubtitleImage,
        work_dir: Option<&Path>,
    ) -> PreprocessedImage {
        let mut result = PreprocessedImage {
            image: GrayImage::new(1, 1),
            original: subtitle.image.clone(),
            subtitle_index: subtitle.index,
            was_inverted: false,
            was_upscaled: false,
            was_binarized: false,
            scale_factor: 1.0,
            debug_path: None,
        };

        // Step 1: Convert RGBA to grayscale, handling transparency
        let mut gray = self.convert_to_grayscale(&subtitle.image);

        // Step 2: Analyze image to determine if we need to invert
        if self.should_invert(&gray) {
            imageops::invert(&mut gray);
            result.was_inverted = true;
        }

        // Step 3: Upscale if image is too small
        if gray.height() < self.config.upscale_threshold_height {
            let (upscaled, scale) = self.upscale(&gray);
            gray = upscaled;
            result.was_upscaled = true;
            result.scale_factor = scale;
        }

        // Step 4: Apply binarization if configured
        if self.config.force_binarization || self.should_binarize(&gray) {
            gray = self.binarize(&gray);
            result.was_binarized = true;
        }

        // Step 5: Add white border
        gray = self.add_border(&gray);

        // Step 6: Optional denoising (simple median-like approach)
        if self.config.denoise {
            gray = self.denoise(&gray);
        }

        result.image = gray;

        // Save debug image if configured
        if self.config.save_debug_images {
            if let Some(dir) = work_dir {
                let fallback = dir.join("preprocessed");
                let debug_dir = self.config.debug_dir.as_deref()
                    .unwrap_or(&fallback);
                let _ = std::fs::create_dir_all(debug_dir);
                let filename = format!("sub_{:04}_preprocessed.png", result.subtitle_index);
                let output_path = PathBuf::from(debug_dir).join(filename);
                let _ = result.image.save(&output_path);
                result.debug_path = Some(output_path);
            }
        }

        result
    }

    /// Convert RGBA image to grayscale, handling transparency.
    fn convert_to_grayscale(&self, image: &RgbaImage) -> GrayImage {
        let (width, height) = image.dimensions();
        let mut gray = GrayImage::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let pixel = image.get_pixel(x, y);
                let r = pixel[0] as f64;
                let g = pixel[1] as f64;
                let b = pixel[2] as f64;
                let a = pixel[3] as f64 / 255.0;

                // Composite onto white background
                let comp_r = r * a + 255.0 * (1.0 - a);
                let comp_g = g * a + 255.0 * (1.0 - a);
                let comp_b = b * a + 255.0 * (1.0 - a);

                // Convert to grayscale using luminance formula
                let luma = (0.299 * comp_r + 0.587 * comp_g + 0.114 * comp_b) as u8;
                gray.put_pixel(x, y, Luma([luma]));
            }
        }

        gray
    }

    /// Determine if image should be inverted.
    fn should_invert(&self, gray: &GrayImage) -> bool {
        if !self.config.auto_detect {
            return false;
        }

        let sum: f64 = gray.pixels().map(|p| p[0] as f64).sum();
        let count = (gray.width() * gray.height()) as f64;
        let mean_brightness = sum / count;

        mean_brightness < 128.0
    }

    /// Determine if binarization would help this image.
    fn should_binarize(&self, gray: &GrayImage) -> bool {
        if !self.config.auto_detect {
            return false;
        }

        let count = (gray.width() * gray.height()) as f64;
        let mean: f64 = gray.pixels().map(|p| p[0] as f64).sum::<f64>() / count;
        let variance: f64 = gray.pixels()
            .map(|p| {
                let diff = p[0] as f64 - mean;
                diff * diff
            })
            .sum::<f64>() / count;
        let std_dev = variance.sqrt();

        if std_dev < 10.0 {
            return false; // Already nearly uniform
        }
        if std_dev > 80.0 {
            return false; // Good contrast
        }

        true // Medium contrast - binarization might help
    }

    /// Upscale image to target height using nearest-neighbor (fast).
    fn upscale(&self, gray: &GrayImage) -> (GrayImage, f64) {
        let current_height = gray.height();
        if current_height >= self.config.target_height {
            return (gray.clone(), 1.0);
        }

        let scale = self.config.target_height as f64 / current_height as f64;
        let new_width = (gray.width() as f64 * scale) as u32;
        let new_height = self.config.target_height;

        let upscaled = imageops::resize(
            gray,
            new_width,
            new_height,
            imageops::FilterType::Lanczos3,
        );

        (upscaled, scale)
    }

    /// Apply Otsu binarization to image.
    fn binarize(&self, gray: &GrayImage) -> GrayImage {
        if self.config.binarization_method == "none" {
            return gray.clone();
        }

        // Compute Otsu threshold
        let threshold = self.otsu_threshold(gray);

        let (width, height) = gray.dimensions();
        let mut binary = GrayImage::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let val = gray.get_pixel(x, y)[0];
                let out = if val > threshold { 255 } else { 0 };
                binary.put_pixel(x, y, Luma([out]));
            }
        }

        binary
    }

    /// Compute Otsu's threshold.
    fn otsu_threshold(&self, gray: &GrayImage) -> u8 {
        let mut histogram = [0u32; 256];
        for p in gray.pixels() {
            histogram[p[0] as usize] += 1;
        }

        let total = (gray.width() * gray.height()) as f64;
        let mut sum_total = 0.0f64;
        for (i, &count) in histogram.iter().enumerate() {
            sum_total += i as f64 * count as f64;
        }

        let mut sum_bg = 0.0f64;
        let mut weight_bg = 0.0f64;
        let mut max_variance = 0.0f64;
        let mut threshold = 0u8;

        for (i, &count) in histogram.iter().enumerate() {
            weight_bg += count as f64;
            if weight_bg == 0.0 {
                continue;
            }

            let weight_fg = total - weight_bg;
            if weight_fg == 0.0 {
                break;
            }

            sum_bg += i as f64 * count as f64;
            let mean_bg = sum_bg / weight_bg;
            let mean_fg = (sum_total - sum_bg) / weight_fg;

            let between_variance = weight_bg * weight_fg * (mean_bg - mean_fg).powi(2);

            if between_variance > max_variance {
                max_variance = between_variance;
                threshold = i as u8;
            }
        }

        threshold
    }

    /// Add white border around image.
    fn add_border(&self, gray: &GrayImage) -> GrayImage {
        let size = self.config.border_size;
        if size == 0 {
            return gray.clone();
        }

        let (w, h) = gray.dimensions();
        let new_w = w + 2 * size;
        let new_h = h + 2 * size;

        let mut bordered = GrayImage::from_pixel(new_w, new_h, Luma([255]));

        for y in 0..h {
            for x in 0..w {
                bordered.put_pixel(x + size, y + size, *gray.get_pixel(x, y));
            }
        }

        bordered
    }

    /// Simple denoising using median-like approach.
    fn denoise(&self, gray: &GrayImage) -> GrayImage {
        // Simple 3x3 median filter
        let (w, h) = gray.dimensions();
        let mut result = gray.clone();

        for y in 1..h.saturating_sub(1) {
            for x in 1..w.saturating_sub(1) {
                let mut values: Vec<u8> = Vec::with_capacity(9);
                for dy in 0..3u32 {
                    for dx in 0..3u32 {
                        values.push(gray.get_pixel(x - 1 + dx, y - 1 + dy)[0]);
                    }
                }
                values.sort_unstable();
                result.put_pixel(x, y, Luma([values[4]]));
            }
        }

        result
    }
}

/// Create preprocessor from settings dictionary.
pub fn create_preprocessor(
    settings_dict: &HashMap<String, serde_json::Value>,
    work_dir: Option<&Path>,
) -> ImagePreprocessor {
    let ocr_engine = settings_dict.get("ocr_engine")
        .and_then(|v| v.as_str())
        .unwrap_or("tesseract");

    let (force_binarization, border_size) = if ocr_engine == "easyocr" || ocr_engine == "paddleocr" {
        (false, settings_dict.get("ocr_border_size").and_then(|v| v.as_u64()).unwrap_or(5) as u32)
    } else {
        (
            settings_dict.get("ocr_force_binarization").and_then(|v| v.as_bool()).unwrap_or(true),
            settings_dict.get("ocr_border_size").and_then(|v| v.as_u64()).unwrap_or(10) as u32,
        )
    };

    let config = PreprocessingConfig {
        auto_detect: settings_dict.get("ocr_preprocess_auto").and_then(|v| v.as_bool()).unwrap_or(true),
        upscale_threshold_height: settings_dict.get("ocr_upscale_threshold").and_then(|v| v.as_u64()).unwrap_or(40) as u32,
        target_height: settings_dict.get("ocr_target_height").and_then(|v| v.as_u64()).unwrap_or(80) as u32,
        border_size,
        force_binarization,
        binarization_method: settings_dict.get("ocr_binarization_method").and_then(|v| v.as_str()).unwrap_or("otsu").to_string(),
        denoise: settings_dict.get("ocr_denoise").and_then(|v| v.as_bool()).unwrap_or(false),
        save_debug_images: settings_dict.get("ocr_save_debug_images").and_then(|v| v.as_bool()).unwrap_or(false),
        debug_dir: work_dir.map(|d| d.join("preprocessed")),
        ..Default::default()
    };

    ImagePreprocessor::new(config)
}
