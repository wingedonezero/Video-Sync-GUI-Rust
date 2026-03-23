//! Timeline conversion — 1:1 port of `vsg_core/correction/stepping/timeline.py`.
//!
//! Convention (empirically verified):
//!   SCC delay of +Xms means Source 2 content is X ms EARLY relative to Source 1.
//!   To find where Source 1 content at ref_time appears in Source 2:
//!     src2_time = ref_time - delay_ms / 1000
//!
//! This module centralises the arithmetic so no other code does ad-hoc
//! timeline conversions.

/// Convert reference timeline position to Source 2 position — `ref_to_src2`
///
/// Source 2's actual audio content that matches Source 1 at `ref_time_s`
/// is located at `ref_time_s - delay_ms / 1000` in Source 2's file.
pub fn ref_to_src2(ref_time_s: f64, delay_ms: f64) -> f64 {
    ref_time_s - delay_ms / 1000.0
}

/// Convert Source 2 timeline position to reference position — `src2_to_ref`
pub fn src2_to_ref(src2_time_s: f64, delay_ms: f64) -> f64 {
    src2_time_s + delay_ms / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let ref_t = 10.0;
        let delay = 500.0;
        let src2 = ref_to_src2(ref_t, delay);
        let back = src2_to_ref(src2, delay);
        assert!((back - ref_t).abs() < 1e-10);
    }

    #[test]
    fn positive_delay_shifts_left() {
        // +500ms delay means Source 2 content is early -> src2 position is earlier
        let src2 = ref_to_src2(10.0, 500.0);
        assert!((src2 - 9.5).abs() < 1e-10);
    }
}
