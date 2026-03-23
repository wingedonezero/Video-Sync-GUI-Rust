//! GPU backend — 1:1 port of `correlation/gpu_backend.py`.
//!
//! Provides device management, spectrogram transform caching, and
//! cleanup utilities. All methods share this module to avoid
//! recreating GPU resources per chunk.

use std::collections::HashMap;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use tch::{Device, Kind, Tensor};

// ── Module State ────────────────────────────────────────────────────────────

static DEVICE: Lazy<Mutex<Option<Device>>> = Lazy::new(|| Mutex::new(None));

/// Cached Hann windows keyed by (n_fft,). Avoids recreating per chunk.
static WINDOW_CACHE: Lazy<Mutex<HashMap<i64, Tensor>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// ── Device Management ────────────────────────────────────────────────────────

/// Get the torch device — `get_device`
pub fn get_device() -> Device {
    let mut guard = DEVICE.lock().unwrap();
    if let Some(device) = *guard {
        return device;
    }

    let device = if tch::Cuda::is_available() {
        let gpu_name = tch::Cuda::device_count();
        tracing::info!(
            "GPU correlation backend: CUDA/ROCm available ({} device(s))",
            gpu_name
        );
        Device::Cuda(0)
    } else {
        tracing::info!("GPU correlation backend: CPU fallback (no CUDA/ROCm)");
        Device::Cpu
    };

    *guard = Some(device);
    device
}

/// Convert f32 slice to torch tensor on device — `to_torch`
pub fn to_torch(arr: &[f32], device: Device) -> Tensor {
    Tensor::from_slice(arr).to(device)
}

// ── Transform Functions ──────────────────────────────────────────────────────

/// Get a cached Hann window for STFT.
fn get_hann_window(n_fft: i64, device: Device) -> Tensor {
    let mut cache = WINDOW_CACHE.lock().unwrap();
    if let Some(window) = cache.get(&n_fft) {
        return window.shallow_clone();
    }
    let window = Tensor::hann_window(n_fft, (Kind::Float, device));
    cache.insert(n_fft, window.shallow_clone());
    window
}

/// Compute spectrogram using STFT — replaces `torchaudio.transforms.Spectrogram`.
/// 1:1 port of `get_spectrogram_transform()`.
///
/// Returns |STFT(signal)|^power with shape [freq_bins, time_frames].
pub fn spectrogram(
    signal: &Tensor,
    n_fft: i64,
    hop_length: i64,
    power: f64,
) -> Tensor {
    let device = signal.device();
    let window = get_hann_window(n_fft, device);

    // torch.stft returns complex tensor [freq_bins, time_frames]
    // tch stft: n_fft, hop_length, win_length, window, normalized, onesided, return_complex, pad
    let stft = signal.stft(n_fft, Some(hop_length), None, Some(&window), false, true, true, false);

    // Take magnitude: |complex| = sqrt(real^2 + imag^2)
    let magnitude = stft.abs();

    // Apply power (1.0 = magnitude spectrogram, 2.0 = power spectrogram)
    if (power - 1.0).abs() < 1e-6 {
        magnitude
    } else if (power - 2.0).abs() < 1e-6 {
        &magnitude * &magnitude
    } else {
        magnitude.pow_tensor_scalar(power)
    }
}

/// Compute mel spectrogram — replaces `torchaudio.transforms.MelSpectrogram`.
/// 1:1 port of `get_mel_spectrogram_transform()`.
///
/// Returns mel-scaled spectrogram with shape [n_mels, time_frames].
pub fn mel_spectrogram(
    signal: &Tensor,
    sample_rate: i64,
    n_fft: i64,
    hop_length: i64,
    n_mels: i64,
    power: f64,
) -> Tensor {
    // First compute the power spectrogram
    let spec = spectrogram(signal, n_fft, hop_length, power);

    // Build mel filterbank
    let device = signal.device();
    let mel_fb = mel_filterbank(sample_rate, n_fft, n_mels, device);

    // Apply mel filterbank: [n_mels, freq_bins] @ [freq_bins, time_frames] = [n_mels, time_frames]
    mel_fb.matmul(&spec)
}

/// Build a mel filterbank matrix — equivalent to torchaudio's internal mel scale.
/// Returns tensor of shape [n_mels, n_fft/2 + 1].
fn mel_filterbank(sample_rate: i64, n_fft: i64, n_mels: i64, device: Device) -> Tensor {
    let sr = sample_rate as f64;
    let n_freqs = n_fft / 2 + 1;

    // Hz to mel conversion: mel = 2595 * log10(1 + hz / 700)
    let hz_to_mel = |hz: f64| -> f64 { 2595.0 * (1.0 + hz / 700.0).log10() };
    let mel_to_hz = |mel: f64| -> f64 { 700.0 * (10.0_f64.powf(mel / 2595.0) - 1.0) };

    let mel_min = hz_to_mel(0.0);
    let mel_max = hz_to_mel(sr / 2.0);

    // Linearly spaced mel points
    let n_points = n_mels + 2;
    let mel_points: Vec<f64> = (0..n_points)
        .map(|i| mel_min + (mel_max - mel_min) * i as f64 / (n_points - 1) as f64)
        .collect();

    // Convert mel points back to Hz, then to FFT bin indices
    let hz_points: Vec<f64> = mel_points.iter().map(|&m| mel_to_hz(m)).collect();
    let bin_points: Vec<f64> = hz_points
        .iter()
        .map(|&hz| hz * n_fft as f64 / sr)
        .collect();

    // Build triangular filters
    let mut fb_data = vec![0.0f32; (n_mels * n_freqs) as usize];

    for m in 0..n_mels as usize {
        let left = bin_points[m];
        let center = bin_points[m + 1];
        let right = bin_points[m + 2];

        for k in 0..n_freqs as usize {
            let kf = k as f64;
            if kf >= left && kf <= center && center > left {
                fb_data[m * n_freqs as usize + k] = ((kf - left) / (center - left)) as f32;
            } else if kf > center && kf <= right && right > center {
                fb_data[m * n_freqs as usize + k] = ((right - kf) / (right - center)) as f32;
            }
        }
    }

    Tensor::from_slice(&fb_data)
        .reshape([n_mels, n_freqs])
        .to_kind(Kind::Float)
        .to(device)
}

// ── Cleanup ──────────────────────────────────────────────────────────────────

/// Release all cached GPU resources — `cleanup_gpu`
///
/// Call after each job's correlation finishes to prevent GPU memory
/// accumulation. Works with both CUDA and ROCm (HIP) backends.
pub fn cleanup_gpu() {
    // Clear cached windows
    if let Ok(mut cache) = WINDOW_CACHE.lock() {
        cache.clear();
    }

    if tch::Cuda::is_available() {
        tch::Cuda::synchronize(0);
        // Note: tch doesn't expose empty_cache() directly.
        // Memory is freed when tensors are dropped (RAII).
    }
}
