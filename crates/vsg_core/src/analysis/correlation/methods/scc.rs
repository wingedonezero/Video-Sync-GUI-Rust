//! Standard Cross-Correlation (SCC) — 1:1 port of `methods/scc.py`.

use tch::Kind;

use super::super::gpu_backend::{get_device, to_torch};
use super::super::gpu_correlation::{extract_peak, scc_confidence};
use super::super::registry::CorrelationMethod;

pub struct Scc {
    pub peak_fit: bool,
}

impl Scc {
    pub fn new(peak_fit: bool) -> Self {
        Self { peak_fit }
    }
}

impl CorrelationMethod for Scc {
    fn name(&self) -> &str { "Standard Correlation (SCC)" }
    fn config_key(&self) -> &str { "multi_corr_scc" }

    fn find_delay(&self, ref_chunk: &[f32], tgt_chunk: &[f32], sr: i64) -> (f64, f64) {
        let device = get_device();
        let ref_t = to_torch(ref_chunk, device);
        let tgt_t = to_torch(tgt_chunk, device);

        let ref_n = (&ref_t - ref_t.mean(Kind::Float)) / (ref_t.std(false) + 1e-9);
        let tgt_n = (&tgt_t - tgt_t.mean(Kind::Float)) / (tgt_t.std(false) + 1e-9);

        let n = ref_n.size1().unwrap() + tgt_n.size1().unwrap() - 1;
        let n_fft = next_pow2(n);

        let r = ref_n.fft_rfft(Some(n_fft), -1, "backward");
        let t = tgt_n.fft_rfft(Some(n_fft), -1, "backward");
        let g = &r * t.conj();
        let corr = g.fft_irfft(Some(n_fft), -1, "backward");

        let (delay_ms, peak_idx) = extract_peak(&corr, n_fft, sr, self.peak_fit);
        let confidence = scc_confidence(&corr, peak_idx, &ref_n, &tgt_n);
        (delay_ms, confidence)
    }
}

fn next_pow2(n: i64) -> i64 { let mut p = 1i64; while p < n { p <<= 1; } p }
