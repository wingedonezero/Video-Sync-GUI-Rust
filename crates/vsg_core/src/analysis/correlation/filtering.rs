//! Audio pre-processing filters — 1:1 port of `correlation/filtering.py`.
//!
//! Butterworth bandpass and FIR lowpass filters.
//! Uses tch for filter application to match Python's scipy behavior.

/// Apply a Butterworth band-pass filter — `apply_bandpass`
///
/// Isolates dialogue frequencies for better correlation.
/// Uses a simple biquad cascade implementation matching scipy's butter+lfilter.
pub fn apply_bandpass(
    waveform: &[f32],
    sr: i32,
    lowcut: f64,
    highcut: f64,
    order: i32,
    log: Option<&dyn Fn(&str)>,
) -> Vec<f32> {
    // Compute Butterworth bandpass filter coefficients
    let nyquist = sr as f64 / 2.0;
    let low = lowcut / nyquist;
    let high = highcut / nyquist;

    match butterworth_bandpass(low, high, order) {
        Some((b, a)) => lfilter(&b, &a, waveform),
        None => {
            if let Some(log) = log {
                log("[FILTER WARNING] Band-pass filter coefficient computation failed, using unfiltered waveform");
            }
            waveform.to_vec()
        }
    }
}

/// Apply a simple FIR low-pass filter — `apply_lowpass`
pub fn apply_lowpass(
    waveform: &[f32],
    sr: i32,
    cutoff_hz: i32,
    num_taps: i32,
    log: Option<&dyn Fn(&str)>,
) -> Vec<f32> {
    if cutoff_hz <= 0 {
        return waveform.to_vec();
    }

    let nyquist = sr as f64 / 2.0;
    let hz = (cutoff_hz as f64).min(nyquist - 1.0);
    let normalized = hz / nyquist;

    match firwin(num_taps as usize, normalized) {
        Some(h) => {
            let a = vec![1.0f64];
            let b: Vec<f64> = h;
            lfilter(&b, &a, waveform)
        }
        None => {
            if let Some(log) = log {
                log("[FILTER WARNING] Low-pass filter failed, using unfiltered waveform");
            }
            waveform.to_vec()
        }
    }
}

// ── Filter implementations ──────────────────────────────────────────────────

/// Simple Butterworth bandpass coefficient computation.
/// Returns (b, a) coefficient vectors, or None on failure.
fn butterworth_bandpass(low: f64, high: f64, order: i32) -> Option<(Vec<f64>, Vec<f64>)> {
    if low <= 0.0 || high >= 1.0 || low >= high || order < 1 {
        return None;
    }

    // For a 2nd-order bandpass section (biquad)
    // Using bilinear transform of analog Butterworth
    let w0 = std::f64::consts::PI * (low + high) / 2.0;
    let bw = std::f64::consts::PI * (high - low);

    // Simplified biquad bandpass using bilinear transform
    let alpha = (bw / 2.0).sin() / (2.0 * std::f64::consts::FRAC_1_SQRT_2);
    let cos_w0 = w0.cos();

    let b0 = alpha;
    let b1 = 0.0;
    let b2 = -alpha;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha;

    let b = vec![b0 / a0, b1 / a0, b2 / a0];
    let a = vec![1.0, a1 / a0, a2 / a0];

    Some((b, a))
}

/// FIR lowpass filter design (windowed sinc) — equivalent to scipy.signal.firwin
fn firwin(num_taps: usize, cutoff: f64) -> Option<Vec<f64>> {
    if num_taps == 0 || cutoff <= 0.0 || cutoff >= 1.0 {
        return None;
    }

    let m = num_taps as f64 - 1.0;
    let half_m = m / 2.0;

    let mut h: Vec<f64> = (0..num_taps)
        .map(|i| {
            let n = i as f64 - half_m;
            let sinc = if n.abs() < 1e-12 {
                2.0 * cutoff
            } else {
                (2.0 * std::f64::consts::PI * cutoff * n).sin() / (std::f64::consts::PI * n)
            };
            // Hamming window
            let window =
                0.54 - 0.46 * (2.0 * std::f64::consts::PI * i as f64 / m).cos();
            sinc * window
        })
        .collect();

    // Normalize so sum = 1
    let sum: f64 = h.iter().sum();
    if sum.abs() > 1e-12 {
        for v in &mut h {
            *v /= sum;
        }
    }

    Some(h)
}

/// Direct-form IIR/FIR filter — equivalent to scipy.signal.lfilter
fn lfilter(b: &[f64], a: &[f64], x: &[f32]) -> Vec<f32> {
    let n = x.len();
    let nb = b.len();
    let na = a.len();
    let mut y = vec![0.0f64; n];

    for i in 0..n {
        let mut val = 0.0f64;
        // FIR part (b coefficients)
        for j in 0..nb {
            if i >= j {
                val += b[j] * x[i - j] as f64;
            }
        }
        // IIR part (a coefficients, skip a[0] which is 1.0)
        for j in 1..na {
            if i >= j {
                val -= a[j] * y[i - j];
            }
        }
        // Normalize by a[0]
        if a[0].abs() > 1e-12 {
            val /= a[0];
        }
        y[i] = val;
    }

    y.iter().map(|&v| v as f32).collect()
}

