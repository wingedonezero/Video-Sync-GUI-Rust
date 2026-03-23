//! GPU-accelerated correlation utilities — 1:1 port of `correlation/gpu_correlation.py`.

use tch::{Device, Kind, Tensor};

/// Create a frequency-domain bandpass mask — `bandpass_mask`
pub fn bandpass_mask(n_fft: i64, sr: i64, lo_hz: f64, hi_hz: f64, device: Device) -> Tensor {
    let freqs = Tensor::fft_rfftfreq(n_fft, 1.0 / sr as f64, (Kind::Float, device));
    let lo = freqs.ge(lo_hz);
    let hi = freqs.le(hi_hz);
    lo.logical_and(&hi)
}

/// Extract delay and peak index from correlation — `extract_peak`
pub fn extract_peak(corr: &Tensor, n_fft: i64, sr: i64, peak_fit: bool) -> (f64, i64) {
    let abs_corr = corr.abs();
    let k = abs_corr.argmax(None, false).int64_value(&[]);

    // Convert circular index to signed lag
    let mut lag_samples: f64 = if k <= n_fft / 2 { k as f64 } else { (k - n_fft) as f64 };

    // Parabolic sub-sample peak fitting
    let len = abs_corr.size1().unwrap_or(0);
    if peak_fit && k > 0 && k < len - 1 {
        let y1 = abs_corr.double_value(&[k - 1]);
        let y2 = abs_corr.double_value(&[k]);
        let y3 = abs_corr.double_value(&[k + 1]);
        let denom = y1 - 2.0 * y2 + y3;
        if denom.abs() > 1e-12 {
            let delta = 0.5 * (y1 - y3) / denom;
            if delta > -1.0 && delta < 1.0 {
                lag_samples += delta;
            }
        }
    }

    let delay_ms = lag_samples / sr as f64 * 1000.0;
    (delay_ms, k)
}

/// SCC-specific confidence — `scc_confidence`
pub fn scc_confidence(corr: &Tensor, peak_idx: i64, ref_norm: &Tensor, tgt_norm: &Tensor) -> f64 {
    let peak_val = corr.abs().double_value(&[peak_idx]);
    let energy_ref = (ref_norm * ref_norm).sum(Kind::Float).double_value(&[]);
    let energy_tgt = (tgt_norm * tgt_norm).sum(Kind::Float).double_value(&[]);
    let match_pct = peak_val / ((energy_ref * energy_tgt).sqrt() + 1e-9) * 100.0;
    match_pct.clamp(0.0, 100.0)
}

/// Peak-to-Sidelobe Ratio confidence — `psr_confidence`
pub fn psr_confidence(corr: &Tensor, peak_idx: i64, exclude_radius: i64) -> f64 {
    let abs_corr = corr.abs();
    let peak_value = abs_corr.double_value(&[peak_idx]);
    let n = abs_corr.size1().unwrap_or(0);

    // Exclude mainlobe region
    let mask = Tensor::ones([n], (Kind::Bool, abs_corr.device()));
    let lo = (peak_idx - exclude_radius).max(0);
    let hi = (peak_idx + exclude_radius + 1).min(n);
    if hi > lo {
        let _ = mask.narrow(0, lo, hi - lo).fill_(0i64);
    }

    let sidelobes = abs_corr.masked_select(&mask);
    if sidelobes.numel() < 10 {
        return 0.0;
    }

    let mean_sl = sidelobes.mean(Kind::Float).double_value(&[]);
    let std_sl = sidelobes.std(false).double_value(&[]);

    if std_sl < 1e-12 {
        return 0.0;
    }

    let psr = (peak_value - mean_sl) / std_sl;

    if psr <= 10.0 {
        0.0
    } else if psr >= 20.0 {
        100.0
    } else {
        (psr - 10.0) / 10.0 * 100.0
    }
}

/// Extract delay from feature-domain correlation — `extract_peak_feature`
pub fn extract_peak_feature(
    corr: &Tensor,
    n_fft: i64,
    max_delay_frames: i64,
    frame_sr: f64,
) -> (f64, f64) {
    let pos_part = corr.narrow(0, 0, max_delay_frames + 1);
    let neg_part = corr.narrow(0, n_fft - max_delay_frames, max_delay_frames);
    let search_region = Tensor::cat(&[&neg_part, &pos_part], 0);

    let abs_search = search_region.abs();
    let k = abs_search.argmax(None, false).int64_value(&[]);
    let lag_frames = k as f64 - max_delay_frames as f64;

    let delay_ms = lag_frames / frame_sr * 1000.0;

    // Confidence
    let peak_val = abs_search.double_value(&[k]);
    let median_val = abs_search.median().double_value(&[]);
    let n = abs_search.size1().unwrap_or(1);
    let neighbor = (n / 100).max(1);

    let mask = abs_search.ones_like().to_kind(Kind::Bool);
    let lo = (k - neighbor).max(0);
    let hi = (k + neighbor + 1).min(n);
    if hi > lo {
        let _ = mask.narrow(0, lo, hi - lo).fill_(0i64);
    }

    let has_any = mask.any().int64_value(&[]) != 0;
    let second_best = if has_any {
        abs_search.masked_select(&mask).max().double_value(&[])
    } else {
        median_val
    };

    let prominence = peak_val / (median_val + 1e-9);
    let uniqueness = peak_val / (second_best + 1e-9);
    let confidence = ((prominence * 5.0 + uniqueness * 8.0) / 2.0).clamp(0.0, 100.0);

    (delay_ms, confidence)
}
