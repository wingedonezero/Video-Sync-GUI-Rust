//! GCC-SCOT — 1:1 port of `methods/gcc_scot.py`.



use super::super::gpu_backend::{get_device, to_torch};
use super::super::gpu_correlation::{bandpass_mask, extract_peak, psr_confidence};
use super::super::registry::CorrelationMethod;

pub struct GccScot;

impl CorrelationMethod for GccScot {
    fn name(&self) -> &str { "GCC-SCOT" }
    fn config_key(&self) -> &str { "multi_corr_gcc_scot" }

    fn find_delay(&self, ref_chunk: &[f32], tgt_chunk: &[f32], sr: i64) -> (f64, f64) {
        let device = get_device();
        let ref_t = to_torch(ref_chunk, device);
        let tgt_t = to_torch(tgt_chunk, device);

        let n = ref_t.size1().unwrap() + tgt_t.size1().unwrap() - 1;
        let n_fft = next_pow2(n);

        let r = ref_t.fft_rfft(Some(n_fft), -1, "backward");
        let t = tgt_t.fft_rfft(Some(n_fft), -1, "backward");
        let g = &r * t.conj();

        let bp = bandpass_mask(n_fft, sr, 300.0, 6000.0, device);
        let bp_inv = bp.logical_not();
        let g = g.masked_fill(&bp_inv.unsqueeze(-1), 0.0);

        let r_power = r.abs().pow_tensor_scalar(2.0);
        let t_power = t.abs().pow_tensor_scalar(2.0);
        let scot_weight = (&r_power * &t_power).sqrt() + 1e-9;

        let g_scot = &g / &scot_weight;
        let g_scot = g_scot.masked_fill(&bp_inv.unsqueeze(-1), 0.0);
        let corr = g_scot.fft_irfft(Some(n_fft), -1, "backward");

        let (delay_ms, peak_idx) = extract_peak(&corr, n_fft, sr, false);
        let confidence = psr_confidence(&corr, peak_idx, 100);
        (delay_ms, confidence)
    }
}

fn next_pow2(n: i64) -> i64 { let mut p = 1i64; while p < n { p <<= 1; } p }
