//! Layout validation — 1:1 port of `vsg_core/job_layouts/validation.py`.
//!
//! Validates that loaded layout data is well-formed.

use serde_json::Value;

/// Validates that loaded layout data is well-formed — `LayoutValidator`.
pub struct LayoutValidator;

impl LayoutValidator {
    /// Validates the structure and content of a layout data dictionary.
    ///
    /// Returns `(true, "Valid")` on success, or `(false, reason)` on failure.
    pub fn validate(layout_data: &Value) -> (bool, String) {
        let obj = match layout_data.as_object() {
            Some(o) => o,
            None => return (false, "Layout data is not a dictionary.".to_string()),
        };

        let required_fields = [
            "job_id",
            "sources",
            "enhanced_layout",
            "track_signature",
            "structure_signature",
        ];

        for field in &required_fields {
            if !obj.contains_key(*field) {
                return (false, format!("Missing required field: {field}"));
            }
        }

        let enhanced_layout = match obj.get("enhanced_layout").and_then(|v| v.as_array()) {
            Some(arr) => arr,
            None => return (false, "Enhanced layout must be a list.".to_string()),
        };

        let item_required = ["source", "id", "type", "user_order_index"];
        for (i, item) in enhanced_layout.iter().enumerate() {
            if let Some(item_obj) = item.as_object() {
                for field in &item_required {
                    if !item_obj.contains_key(*field) {
                        return (
                            false,
                            format!("Layout item {i} missing required field: {field}"),
                        );
                    }
                }
            } else {
                return (false, format!("Layout item {i} is not a dictionary."));
            }
        }

        (true, "Valid".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_valid_layout() {
        let data = json!({
            "job_id": "abc123",
            "sources": {"Source 1": "/path/to/file.mkv"},
            "enhanced_layout": [
                {"source": "Source 1", "id": 0, "type": "video", "user_order_index": 0}
            ],
            "track_signature": {},
            "structure_signature": {}
        });
        let (valid, _) = LayoutValidator::validate(&data);
        assert!(valid);
    }

    #[test]
    fn test_missing_field() {
        let data = json!({
            "job_id": "abc123",
            "sources": {}
        });
        let (valid, reason) = LayoutValidator::validate(&data);
        assert!(!valid);
        assert!(reason.contains("enhanced_layout"));
    }
}
