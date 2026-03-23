//! Time-based sync plugin for SubtitleData — 1:1 port of `sync_mode_plugins/time_based.py`.
//!
//! Simple delay application - applies raw delay to all events.
//! Used when mkvmerge --sync is not handling the delay.

use chrono::Local;

use crate::subtitles::data::{OperationRecord, OperationResult, SubtitleData};
use crate::subtitles::sync_modes::{SyncParams, SyncPlugin};
use crate::subtitles::sync_utils::apply_delay_to_events;

/// Simple time-based sync - applies raw delay to all events.
///
/// This is the baseline sync mode. For time-based with mkvmerge --sync,
/// no subtitle modification is needed (handled by mkvmerge).
pub struct TimeBasedSync;

impl SyncPlugin for TimeBasedSync {
    fn name(&self) -> &str {
        "time-based"
    }

    fn description(&self) -> &str {
        "Simple delay application (or mkvmerge --sync)"
    }

    fn apply(
        &self,
        subtitle_data: &mut SubtitleData,
        params: &SyncParams,
    ) -> OperationResult {
        let log_msg = |msg: &str| {
            if let Some(log_fn) = params.log {
                log_fn(msg);
            }
        };

        // Check if we should use raw values mode
        let use_raw_values = params
            .extra
            .get("time_based_use_raw_values")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !use_raw_values {
            // Default: mkvmerge --sync handles the delay
            log_msg("[TimeBased] Using mkvmerge --sync mode (no subtitle modification)");

            let record = OperationRecord {
                operation: "sync".to_string(),
                timestamp: Local::now().to_rfc3339(),
                parameters: serde_json::json!({
                    "mode": "time-based-mkvmerge",
                    "total_delay_ms": params.total_delay_ms,
                }),
                events_affected: 0,
                styles_affected: 0,
                summary: "Sync handled by mkvmerge --sync".to_string(),
            };
            subtitle_data.operations.push(record);

            let mut result = OperationResult::ok("sync");
            result.summary = "mkvmerge --sync mode (no subtitle modification)".to_string();
            result.details.insert(
                "method".to_string(),
                serde_json::json!("mkvmerge_sync"),
            );
            result.details.insert(
                "delay_ms".to_string(),
                serde_json::json!(params.total_delay_ms),
            );
            return result;
        }

        // Raw values mode: apply delay directly
        log_msg("[TimeBased] === Time-Based Sync (Raw Values) ===");
        log_msg(&format!(
            "[TimeBased] Events: {}",
            subtitle_data.events.len()
        ));
        log_msg(&format!(
            "[TimeBased] Delay: {:+.3}ms",
            params.total_delay_ms
        ));

        let events_synced = apply_delay_to_events(subtitle_data, params.total_delay_ms, false);

        let record = OperationRecord {
            operation: "sync".to_string(),
            timestamp: Local::now().to_rfc3339(),
            parameters: serde_json::json!({
                "mode": "time-based-raw",
                "total_delay_ms": params.total_delay_ms,
            }),
            events_affected: events_synced,
            styles_affected: 0,
            summary: format!(
                "Applied {:+.1}ms delay to {} events",
                params.total_delay_ms, events_synced
            ),
        };
        subtitle_data.operations.push(record.clone());

        log_msg(&format!(
            "[TimeBased] Applied delay to {events_synced} events"
        ));

        let mut result = OperationResult::ok("sync");
        result.events_affected = events_synced;
        result.summary = record.summary;
        result.details.insert(
            "delay_ms".to_string(),
            serde_json::json!(params.total_delay_ms),
        );
        result.details.insert(
            "events_synced".to_string(),
            serde_json::json!(events_synced),
        );
        result
    }
}
