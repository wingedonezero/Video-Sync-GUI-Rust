//! Spectrogram Correlation — 1:1 port of `methods/spectrogram.py`.

use tch::{Kind, Tensor};

use super::super::gpu_backend::{get_device, to_torch};
use super::super::gpu_correlation::extract_peak_feature;
use super::super::registry::CorrelationMethod;

pub struct SpectrogramCorrelation;

impl CorrelationMethod for SpectrogramCorrelation {
    fn name(&self) -> &str { "Spectrogram Correlation" }
    fn config_key(&self) -> &str { "multi_corr_spectrogram" }

    fn find_delay(&self, ref_chunk: &[f32], tgt_chunk: &[f32], sr: i64) -> (f64, f64) {
        let hop_length: i64 = 512;
        let n_fft_spec: i64 = 2048;
        let n_mels: i64 = 64;

        let device = get_device();
        let ref_t = to_torch(ref_chunk, device);
        let tgt_t = to_torch(tgt_chunk, device);

        let window = Tensor::hann_window(n_fft_spec, (Kind::Float, device));

        let ref_stft = ref_t.stft(
            n_fft_spec, Some(hop_length), None, Some(&window),
            false, true, true, false,
        );
        let tgt_stft = tgt_t.stft(
            n_fft_spec, Some(hop_length), None, Some(&window),
            false, true, true, false,
        );

        let ref_power = ref_stft.abs().pow_tensor_scalar(2.0);
        let tgt_power = tgt_stft.abs().pow_tensor_scalar(2.0);

        let mel_fb = create_mel_filterbank(sr, n_fft_spec, n_mels, device);
        let ref_mel = mel_fb.matmul(&ref_power);
        let tgt_mel = mel_fb.matmul(&tgt_power);

        let ref_db = ref_mel.clamp_min(1e-10).log10() * 10.0;
        let tgt_db = tgt_mel.clamp_min(1e-10).log10() * 10.0;

        let ref_db = &ref_db - ref_db.max();
        let tgt_db = &tgt_db - tgt_db.max();

        let ref_flat = ref_db.mean_dim(-2i64, false, Kind::Float);
        let tgt_flat = tgt_db.mean_dim(-2i64, false, Kind::Float);

        let ref_norm = (&ref_flat - ref_flat.mean(Kind::Float)) / (ref_flat.std(false) + 1e-9);
        let tgt_norm = (&tgt_flat - tgt_flat.mean(Kind::Float)) / (tgt_flat.std(false) + 1e-9);

        let frame_sr = sr as f64 / hop_length as f64;
        let n_frames = ref_norm.size1().unwrap_or(1).min(tgt_norm.size1().unwrap_or(1));
        let max_delay_frames = n_frames / 2;

        let n = ref_norm.size1().unwrap() + tgt_norm.size1().unwrap() - 1;
        let n_fft = next_pow2(n);

        let r = ref_norm.fft_rfft(Some(n_fft), -1, "backward");
        let t = tgt_norm.fft_rfft(Some(n_fft), -1, "backward");
        let g = &r * t.conj();
        let g_phat = &g / (g.abs() + 1e-9);
        let corr = g_phat.fft_irfft(Some(n_fft), -1, "backward");

        extract_peak_feature(&corr, n_fft, max_delay_frames, frame_sr)
    }
}

fn create_mel_filterbank(sr: i64, n_fft: i64, n_mels: i64, device: tch::Device) -> Tensor {
    let n_freqs = n_fft / 2 + 1;
    let f_max = sr as f64 / 2.0;
    let mel_min = hz_to_mel(0.0);
    let mel_max = hz_to_mel(f_max);

    let mel_points: Vec<f64> = (0..=(n_mels + 1))
        .map(|i| mel_min + (mel_max - mel_min) * i as f64 / (n_mels + 1) as f64)
        .collect();
    let hz_points: Vec<f64> = mel_points.iter().map(|&m| mel_to_hz(m)).collect();
    let fft_freqs: Vec<f64> = (0..n_freqs)
        .map(|i| sr as f64 * i as f64 / n_fft as f64)
        .collect();

    let mut fb = vec![0.0f32; (n_mels * n_freqs) as usize];
    for m in 0..n_mels as usize {
        let (f_left, f_center, f_right) = (hz_points[m], hz_points[m + 1], hz_points[m + 2]);
        for k in 0..n_freqs as usize {
            let freq = fft_freqs[k];
            if freq >= f_left && freq <= f_center && (f_center - f_left) > 0.0 {
                fb[m * n_freqs as usize + k] = ((freq - f_left) / (f_center - f_left)) as f32;
            } else if freq > f_center && freq <= f_right && (f_right - f_center) > 0.0 {
                fb[m * n_freqs as usize + k] = ((f_right - freq) / (f_right - f_center)) as f32;
            }
        }
    }

    Tensor::from_slice(&fb).reshape([n_mels, n_freqs]).to(device)
}

fn hz_to_mel(hz: f64) -> f64 { 2595.0 * (1.0 + hz / 700.0).log10() }
fn mel_to_hz(mel: f64) -> f64 { 700.0 * (10.0f64.powf(mel / 2595.0) - 1.0) }
fn next_pow2(n: i64) -> i64 { let mut p = 1i64; while p < n { p <<= 1; } p }
