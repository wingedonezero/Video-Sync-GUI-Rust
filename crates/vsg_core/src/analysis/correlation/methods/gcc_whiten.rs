//! GCC Whitened — 1:1 port of `methods/gcc_whiten.py`.



use super::super::gpu_backend::{get_device, to_torch};
use super::super::gpu_correlation::{bandpass_mask, extract_peak, psr_confidence};
use super::super::registry::CorrelationMethod;

pub struct GccWhiten;

impl CorrelationMethod for GccWhiten {
    fn name(&self) -> &str { "Whitened Cross-Correlation" }
    fn config_key(&self) -> &str { "multi_corr_gcc_whiten" }

    fn find_delay(&self, ref_chunk: &[f32], tgt_chunk: &[f32], sr: i64) -> (f64, f64) {
        let device = get_device();
        let ref_t = to_torch(ref_chunk, device);
        let tgt_t = to_torch(tgt_chunk, device);

        let n = ref_t.size1().unwrap() + tgt_t.size1().unwrap() - 1;
        let n_fft = next_pow2(n);

        let mut r = ref_t.fft_rfft(Some(n_fft), -1, "backward");
        let mut t = tgt_t.fft_rfft(Some(n_fft), -1, "backward");

        let bp = bandpass_mask(n_fft, sr, 300.0, 6000.0, device);
        let bp_inv = bp.logical_not();
        r = r.masked_fill(&bp_inv.unsqueeze(-1), 0.0);
        t = t.masked_fill(&bp_inv.unsqueeze(-1), 0.0);

        let r_white = &r / (r.abs() + 1e-9);
        let t_white = &t / (t.abs() + 1e-9);
        let r_white = r_white.masked_fill(&bp_inv.unsqueeze(-1), 0.0);
        let t_white = t_white.masked_fill(&bp_inv.unsqueeze(-1), 0.0);

        let g_white = &r_white * t_white.conj();
        let corr = g_white.fft_irfft(Some(n_fft), -1, "backward");

        let (delay_ms, peak_idx) = extract_peak(&corr, n_fft, sr, false);
        let confidence = psr_confidence(&corr, peak_idx, 100);
        (delay_ms, confidence)
    }
}

fn next_pow2(n: i64) -> i64 { let mut p = 1i64; while p < n { p <<= 1; } p }
