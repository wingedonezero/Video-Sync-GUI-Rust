//! Global shift calculation — 1:1 port of `vsg_core/analysis/global_shift.py`.

use std::collections::HashMap;

use super::types::{ContainerDelayInfo, GlobalShiftCalculation};
use crate::models::context_types::ManualLayoutItem;

/// Calculate global shift to eliminate negative delays — `calculate_global_shift`
pub fn calculate_global_shift(
    source_delays: &HashMap<String, i32>,
    raw_source_delays: &HashMap<String, f64>,
    manual_layout: &[ManualLayoutItem],
    container_info: Option<&ContainerDelayInfo>,
    global_shift_required: bool,
    log: &dyn Fn(&str),
) -> GlobalShiftCalculation {
    let mut delays_to_consider: Vec<i32> = Vec::new();
    let mut raw_delays_to_consider: Vec<f64> = Vec::new();

    if global_shift_required {
        log("[Global Shift] Identifying delays from sources contributing audio tracks...");

        for item in manual_layout {
            let item_source = item.source.as_deref().unwrap_or("");
            let item_type = item.track_type.as_deref().unwrap_or("");
            if item_type == "audio" {
                if let Some(&delay) = source_delays.get(item_source) {
                    if !delays_to_consider.contains(&delay) {
                        delays_to_consider.push(delay);
                        if let Some(&raw) = raw_source_delays.get(item_source) {
                            raw_delays_to_consider.push(raw);
                        }
                        log(&format!(
                            "  - Considering delay from {item_source}: {delay}ms"
                        ));
                    }
                }
            }
        }

        // Also consider Source 1 audio container delays
        if let Some(info) = container_info {
            let audio_delays: Vec<f64> = info.audio_delays_ms.values().copied().collect();
            if audio_delays.iter().any(|&d| d != 0.0) {
                delays_to_consider.extend(audio_delays.iter().map(|&d| d as i32));
                log("  - Considering Source 1 audio container delays (video delays ignored).");
            }
        }
    }

    let most_negative = delays_to_consider.iter().copied().min().unwrap_or(0);
    let most_negative_raw = raw_delays_to_consider
        .iter()
        .copied()
        .fold(f64::MAX, f64::min);
    let most_negative_raw = if most_negative_raw == f64::MAX {
        0.0
    } else {
        most_negative_raw
    };

    if most_negative < 0 {
        let global_shift_ms = most_negative.abs();
        let raw_global_shift_ms = most_negative_raw.abs();

        log(&format!(
            "[Delay] Most negative relevant delay: {most_negative}ms (rounded), {most_negative_raw:.3}ms (raw)"
        ));
        log(&format!(
            "[Delay] Applying lossless global shift: +{global_shift_ms}ms (rounded), +{raw_global_shift_ms:.3}ms (raw)"
        ));

        GlobalShiftCalculation {
            shift_ms: global_shift_ms,
            raw_shift_ms: raw_global_shift_ms,
            most_negative_ms: most_negative,
            most_negative_raw_ms: most_negative_raw,
            applied: true,
        }
    } else {
        log("[Delay] All relevant delays are non-negative. No global shift needed.");
        GlobalShiftCalculation {
            shift_ms: 0,
            raw_shift_ms: 0.0,
            most_negative_ms: most_negative,
            most_negative_raw_ms: most_negative_raw,
            applied: false,
        }
    }
}

/// Apply global shift to all source delays — `apply_global_shift_to_delays`
pub fn apply_global_shift_to_delays(
    source_delays: &HashMap<String, i32>,
    raw_source_delays: &HashMap<String, f64>,
    shift: &GlobalShiftCalculation,
    log: &dyn Fn(&str),
) -> (HashMap<String, i32>, HashMap<String, f64>) {
    if !shift.applied {
        return (source_delays.clone(), raw_source_delays.clone());
    }

    let mut updated_delays = HashMap::new();
    let mut updated_raw_delays = HashMap::new();

    log("[Delay] Adjusted delays after global shift:");
    let mut sorted_keys: Vec<&String> = source_delays.keys().collect();
    sorted_keys.sort();

    for source_key in sorted_keys {
        let original_delay = source_delays[source_key];
        let original_raw = raw_source_delays.get(source_key).copied().unwrap_or(original_delay as f64);

        let new_delay = original_delay + shift.shift_ms;
        let new_raw = original_raw + shift.raw_shift_ms;

        updated_delays.insert(source_key.clone(), new_delay);
        updated_raw_delays.insert(source_key.clone(), new_raw);

        log(&format!(
            "  - {source_key}: {original_delay:+.1}ms → {new_delay:+.1}ms \
             (raw: {original_raw:+.3}ms → {new_raw:+.3}ms)"
        ));
    }

    (updated_delays, updated_raw_delays)
}
