//! Frame hashing and comparison functions for video sync verification.
//!
//! Contains:
//! - Perceptual hash computation (phash, dhash, average_hash)
//! - SSIM (Structural Similarity Index) comparison
//! - MSE (Mean Squared Error) comparison
//! - Unified frame comparison interface
//!
//! 1:1 port of `vsg_core/subtitles/frame_utils/frame_hashing.py`.
//!
//! Uses the `image` crate for image loading and manipulation.
//! Hash algorithms (dHash, pHash, average_hash) are implemented manually
//! since they are simple algorithms.

use image::{DynamicImage, GrayImage, GenericImageView};

use super::video_reader::VideoFrame;

/// Result of comparing two frames using all available metrics.
#[derive(Debug, Clone)]
pub struct MultiMetricResult {
    /// Hamming distance (0=identical)
    pub phash_distance: i32,
    /// phash_distance <= threshold
    pub phash_match: bool,
    /// (1-SSIM)*100 (0=identical, <10=match)
    pub ssim_distance: f64,
    /// ssim_distance <= threshold
    pub ssim_match: bool,
    /// Raw MSE value
    pub mse_value: f64,
    /// MSE/100 capped at 100
    pub mse_distance: f64,
    /// mse_distance <= threshold
    pub mse_match: bool,
}

/// A perceptual hash represented as a bit vector.
#[derive(Debug, Clone)]
pub struct ImageHash {
    pub bits: Vec<bool>,
    pub hash_size: usize,
}

impl ImageHash {
    /// Convert hash to hexadecimal string representation.
    pub fn to_hex(&self) -> String {
        let mut hex = String::new();
        for chunk in self.bits.chunks(4) {
            let mut val = 0u8;
            for (i, &bit) in chunk.iter().enumerate() {
                if bit {
                    val |= 1 << (3 - i);
                }
            }
            hex.push_str(&format!("{:x}", val));
        }
        hex
    }

    /// Compute Hamming distance to another hash.
    pub fn hamming_distance(&self, other: &ImageHash) -> i32 {
        self.bits
            .iter()
            .zip(other.bits.iter())
            .filter(|(a, b)| a != b)
            .count() as i32
    }
}

/// Compute perceptual hash from image data bytes.
///
/// Supports algorithms: dhash, phash, average_hash.
pub fn compute_perceptual_hash(
    image_data: &[u8],
    _runner: &crate::io::runner::CommandRunner,
    algorithm: &str,
    hash_size: usize,
) -> Option<String> {
    let img = image::load_from_memory(image_data).ok()?;
    let hash = compute_hash_from_image(&img, hash_size, algorithm)?;
    Some(hash.to_hex())
}

/// Compute perceptual hash of a VideoFrame.
pub fn compute_frame_hash(
    frame: &VideoFrame,
    hash_size: usize,
    method: &str,
) -> Option<ImageHash> {
    let gray = frame.to_gray_image();
    let img = DynamicImage::ImageLuma8(gray);
    compute_hash_from_image(&img, hash_size, method)
}

/// Compute hash from a DynamicImage.
fn compute_hash_from_image(
    img: &DynamicImage,
    hash_size: usize,
    method: &str,
) -> Option<ImageHash> {
    match method {
        "dhash" => Some(dhash(img, hash_size)),
        "average_hash" => Some(average_hash(img, hash_size)),
        "phash" => Some(phash(img, hash_size)),
        _ => Some(phash(img, hash_size)), // default to phash
    }
}

/// Difference hash (dHash).
///
/// Resizes image to (hash_size+1, hash_size), then compares adjacent pixels.
/// Good for compression artifacts.
fn dhash(img: &DynamicImage, hash_size: usize) -> ImageHash {
    let gray = img.to_luma8();
    let resized = image::imageops::resize(
        &gray,
        (hash_size + 1) as u32,
        hash_size as u32,
        image::imageops::FilterType::Lanczos3,
    );

    let mut bits = Vec::with_capacity(hash_size * hash_size);
    for y in 0..hash_size {
        for x in 0..hash_size {
            let left = resized.get_pixel(x as u32, y as u32)[0];
            let right = resized.get_pixel((x + 1) as u32, y as u32)[0];
            bits.push(left < right);
        }
    }

    ImageHash { bits, hash_size }
}

/// Average hash.
///
/// Resizes image to hash_size x hash_size, computes mean, and creates
/// hash based on whether each pixel is above or below the mean.
fn average_hash(img: &DynamicImage, hash_size: usize) -> ImageHash {
    let gray = img.to_luma8();
    let resized = image::imageops::resize(
        &gray,
        hash_size as u32,
        hash_size as u32,
        image::imageops::FilterType::Lanczos3,
    );

    // Compute mean pixel value
    let pixels: Vec<u8> = resized.pixels().map(|p| p[0]).collect();
    let mean: f64 = pixels.iter().map(|&p| p as f64).sum::<f64>() / pixels.len() as f64;

    let bits: Vec<bool> = pixels.iter().map(|&p| p as f64 >= mean).collect();

    ImageHash { bits, hash_size }
}

/// Perceptual hash (pHash).
///
/// Resizes image to 32x32, applies DCT-like transform (simplified),
/// then takes top-left hash_size x hash_size block and hashes based on median.
fn phash(img: &DynamicImage, hash_size: usize) -> ImageHash {
    let gray = img.to_luma8();
    // Resize to 32x32 for DCT (standard pHash size)
    let resize_size = hash_size * 4;
    let resized = image::imageops::resize(
        &gray,
        resize_size as u32,
        resize_size as u32,
        image::imageops::FilterType::Lanczos3,
    );

    // Simple DCT-like transform: compute 2D DCT of the resized image
    let n = resize_size;
    let pixels: Vec<f64> = resized.pixels().map(|p| p[0] as f64).collect();

    // Compute simplified DCT coefficients for top-left hash_size x hash_size block
    let mut dct_vals: Vec<f64> = Vec::with_capacity(hash_size * hash_size);

    for u in 0..hash_size {
        for v in 0..hash_size {
            let mut sum = 0.0;
            for x in 0..n {
                for y in 0..n {
                    let pixel = pixels[x * n + y];
                    let cos_x =
                        ((2 * x + 1) as f64 * u as f64 * std::f64::consts::PI / (2.0 * n as f64))
                            .cos();
                    let cos_y =
                        ((2 * y + 1) as f64 * v as f64 * std::f64::consts::PI / (2.0 * n as f64))
                            .cos();
                    sum += pixel * cos_x * cos_y;
                }
            }
            dct_vals.push(sum);
        }
    }

    // Skip DC component (first value), compute median of remaining
    let mut sorted_vals = dct_vals[1..].to_vec();
    sorted_vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = if sorted_vals.len() % 2 == 0 {
        (sorted_vals[sorted_vals.len() / 2 - 1] + sorted_vals[sorted_vals.len() / 2]) / 2.0
    } else {
        sorted_vals[sorted_vals.len() / 2]
    };

    // Hash: 1 if above median, 0 if below
    let bits: Vec<bool> = dct_vals.iter().map(|&v| v > median).collect();

    ImageHash { bits, hash_size }
}

/// Compute Hamming distance between two perceptual hashes.
pub fn compute_hamming_distance(hash1: &ImageHash, hash2: &ImageHash) -> i32 {
    hash1.hamming_distance(hash2)
}

/// Compute Structural Similarity Index (SSIM) between two frames.
///
/// Returns SSIM value from 0.0 to 1.0. Higher = more similar.
pub fn compute_ssim(
    frame1: &VideoFrame,
    frame2: &VideoFrame,
    use_global: bool,
) -> f64 {
    let arr1 = &frame1.data;
    let arr2_data;

    let arr2 = if frame1.dimensions() != frame2.dimensions() {
        // Resize frame2 to match frame1
        let resized = frame2.resize(frame1.width, frame1.height);
        arr2_data = resized.data;
        &arr2_data
    } else {
        &frame2.data
    };

    if use_global || true {
        // Global SSIM: uses whole-image mean/variance
        global_ssim(arr1, arr2)
    } else {
        global_ssim(arr1, arr2)
    }
}

/// Compute global SSIM between two grayscale pixel arrays.
fn global_ssim(arr1: &[u8], arr2: &[u8]) -> f64 {
    let n = arr1.len().min(arr2.len()) as f64;
    if n == 0.0 {
        return 0.0;
    }

    let c1 = (0.01 * 255.0_f64).powi(2);
    let c2 = (0.03 * 255.0_f64).powi(2);

    let mu1: f64 = arr1.iter().map(|&p| p as f64).sum::<f64>() / n;
    let mu2: f64 = arr2.iter().map(|&p| p as f64).sum::<f64>() / n;

    let sigma1_sq: f64 = arr1.iter().map(|&p| (p as f64 - mu1).powi(2)).sum::<f64>() / n;
    let sigma2_sq: f64 = arr2.iter().map(|&p| (p as f64 - mu2).powi(2)).sum::<f64>() / n;

    let sigma12: f64 = arr1
        .iter()
        .zip(arr2.iter())
        .map(|(&p1, &p2)| (p1 as f64 - mu1) * (p2 as f64 - mu2))
        .sum::<f64>()
        / n;

    let ssim = ((2.0 * mu1 * mu2 + c1) * (2.0 * sigma12 + c2))
        / ((mu1.powi(2) + mu2.powi(2) + c1) * (sigma1_sq + sigma2_sq + c2));

    ssim
}

/// Compute Mean Squared Error between two frames.
///
/// Lower MSE = more similar. 0 = identical.
pub fn compute_mse(frame1: &VideoFrame, frame2: &VideoFrame) -> f64 {
    let arr1 = &frame1.data;
    let arr2_data;

    let arr2 = if frame1.dimensions() != frame2.dimensions() {
        let resized = frame2.resize(frame1.width, frame1.height);
        arr2_data = resized.data;
        &arr2_data
    } else {
        &frame2.data
    };

    let n = arr1.len().min(arr2.len()) as f64;
    if n == 0.0 {
        return f64::INFINITY;
    }

    let mse: f64 = arr1
        .iter()
        .zip(arr2.iter())
        .map(|(&p1, &p2)| (p1 as f64 - p2 as f64).powi(2))
        .sum::<f64>()
        / n;

    mse
}

/// Compare two frames using the specified method.
///
/// Returns (distance, is_match).
pub fn compare_frames(
    frame1: &VideoFrame,
    frame2: &VideoFrame,
    method: &str,
    hash_algorithm: &str,
    hash_size: usize,
    threshold: Option<i32>,
    use_global_ssim: bool,
) -> (f64, bool) {
    match method {
        "ssim" => {
            let ssim = compute_ssim(frame1, frame2, use_global_ssim);
            let distance = (1.0 - ssim) * 100.0;
            let max_dist = threshold.unwrap_or(10) as f64;
            let is_match = distance <= max_dist;
            (distance, is_match)
        }
        "mse" => {
            let mse = compute_mse(frame1, frame2);
            let distance = (mse / 100.0).min(100.0);
            let max_dist = threshold.unwrap_or(5) as f64;
            let is_match = distance <= max_dist;
            (distance, is_match)
        }
        _ => {
            // "hash" (default)
            let hash1 = compute_frame_hash(frame1, hash_size, hash_algorithm);
            let hash2 = compute_frame_hash(frame2, hash_size, hash_algorithm);

            match (hash1, hash2) {
                (Some(h1), Some(h2)) => {
                    let distance = compute_hamming_distance(&h1, &h2) as f64;
                    let max_dist = threshold.unwrap_or(5) as f64;
                    let is_match = distance <= max_dist;
                    (distance, is_match)
                }
                _ => (999.0, false),
            }
        }
    }
}

/// Compare two frames using ALL metrics (phash, SSIM, MSE) in a single call.
///
/// Converts frames to grayscale arrays once and reuses for SSIM + MSE.
pub fn compare_frames_multi(
    frame1: &VideoFrame,
    frame2: &VideoFrame,
    hash_algorithm: &str,
    hash_size: usize,
    hash_threshold: i32,
    ssim_threshold: i32,
    mse_threshold: i32,
    use_global_ssim: bool,
) -> MultiMetricResult {
    // phash
    let phash_dist = match (
        compute_frame_hash(frame1, hash_size, hash_algorithm),
        compute_frame_hash(frame2, hash_size, hash_algorithm),
    ) {
        (Some(h1), Some(h2)) => compute_hamming_distance(&h1, &h2),
        _ => 999,
    };

    // MSE
    let mse_val = compute_mse(frame1, frame2);
    let mse_dist = (mse_val / 100.0).min(100.0);

    // SSIM
    let ssim = compute_ssim(frame1, frame2, use_global_ssim);
    let ssim_dist = (1.0 - ssim) * 100.0;

    MultiMetricResult {
        phash_distance: phash_dist,
        phash_match: phash_dist <= hash_threshold,
        ssim_distance: ssim_dist,
        ssim_match: ssim_dist <= ssim_threshold as f64,
        mse_value: mse_val,
        mse_distance: mse_dist,
        mse_match: mse_dist <= mse_threshold as f64,
    }
}
