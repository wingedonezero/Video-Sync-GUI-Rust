//! Onset Detection — 1:1 port of `methods/onset.py`.

use tch::{Kind, Tensor};

use super::super::gpu_backend::{get_device, to_torch};
use super::super::gpu_correlation::extract_peak_feature;
use super::super::registry::CorrelationMethod;

pub struct OnsetDetection;

impl CorrelationMethod for OnsetDetection {
    fn name(&self) -> &str { "Onset Detection" }
    fn config_key(&self) -> &str { "multi_corr_onset" }

    fn find_delay(&self, ref_chunk: &[f32], tgt_chunk: &[f32], sr: i64) -> (f64, f64) {
        let hop_length: i64 = 512;
        let n_fft_spec: i64 = 2048;

        let device = get_device();
        let ref_t = to_torch(ref_chunk, device);
        let tgt_t = to_torch(tgt_chunk, device);

        let window = Tensor::hann_window(n_fft_spec, (Kind::Float, device));

        // STFT with 10 args for tch 0.23
        let ref_stft = ref_t.stft(
            n_fft_spec, Some(hop_length), None, Some(&window),
            false, true, true, false,
        );
        let tgt_stft = tgt_t.stft(
            n_fft_spec, Some(hop_length), None, Some(&window),
            false, true, true, false,
        );

        let ref_spec = ref_stft.abs();
        let tgt_spec = tgt_stft.abs();

        let ref_diff = ref_spec.diff(1, -1, None::<&Tensor>, None::<&Tensor>);
        let tgt_diff = tgt_spec.diff(1, -1, None::<&Tensor>, None::<&Tensor>);
        let ref_flux = ref_diff.clamp_min(0.0).mean_dim(-2i64, false, Kind::Float);
        let tgt_flux = tgt_diff.clamp_min(0.0).mean_dim(-2i64, false, Kind::Float);

        let ref_env = (&ref_flux - ref_flux.mean(Kind::Float)) / (ref_flux.std(false) + 1e-9);
        let tgt_env = (&tgt_flux - tgt_flux.mean(Kind::Float)) / (tgt_flux.std(false) + 1e-9);

        let frame_sr = sr as f64 / hop_length as f64;
        let n_frames = ref_env.size1().unwrap_or(1).min(tgt_env.size1().unwrap_or(1));
        let max_delay_frames = n_frames / 2;

        let n = ref_env.size1().unwrap() + tgt_env.size1().unwrap() - 1;
        let n_fft = next_pow2(n);

        let r = ref_env.fft_rfft(Some(n_fft), -1, "backward");
        let t = tgt_env.fft_rfft(Some(n_fft), -1, "backward");
        let g = &r * t.conj();
        let g_phat = &g / (g.abs() + 1e-9);
        let corr = g_phat.fft_irfft(Some(n_fft), -1, "backward");

        extract_peak_feature(&corr, n_fft, max_delay_frames, frame_sr)
    }
}

fn next_pow2(n: i64) -> i64 { let mut p = 1i64; while p < n { p <<= 1; } p }
