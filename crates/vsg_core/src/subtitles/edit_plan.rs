//! Non-destructive subtitle edit plan system — 1:1 port of `edit_plan.py`.
//!
//! The editor creates an EditPlan describing changes to make.
//! The plan is saved to JSON and applied at job execution time.
//!
//! This allows:
//! - Preview in editor without modifying original data
//! - Same workflow for ASS, SRT, and OCR sources
//! - Video sync (click line -> seek) works via start_ms/end_ms
//! - Future PyonFX integration for effects

use std::collections::HashSet;
use std::path::Path;

use chrono::Local;
use serde::{Deserialize, Serialize};

use super::data::{SubtitleData, SubtitleEvent, SubtitleStyle};

// =============================================================================
// Event Group
// =============================================================================

/// Predefined event groups for subtitle organization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventGroup {
    Dialogue,
    Op,
    Ed,
    Insert,
    Signs,
    Titles,
    Preview,
    Custom,
}

impl EventGroup {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Dialogue => "dialogue",
            Self::Op => "op",
            Self::Ed => "ed",
            Self::Insert => "insert",
            Self::Signs => "signs",
            Self::Titles => "titles",
            Self::Preview => "preview",
            Self::Custom => "custom",
        }
    }
}

// =============================================================================
// Event Edit
// =============================================================================

/// Planned edit for a single subtitle event.
///
/// Identifies event by original_index (stable across session).
/// Only `Some` fields are applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEdit {
    /// Event identification (by original index in loaded data)
    pub event_index: i32,

    // Text changes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_text: Option<String>,

    // Style assignment
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_style: Option<String>,

    // Group assignment (for sync behavior, visual grouping)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,

    // Manual timing adjustments (added to existing timing)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_offset_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_offset_ms: Option<f64>,

    // Absolute timing override (replaces existing timing)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_start_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_end_ms: Option<f64>,

    // Layer change
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_layer: Option<i32>,

    // Actor/name field
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_name: Option<String>,

    // Effect field
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_effect: Option<String>,

    // Comment toggle
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_comment: Option<bool>,
}

impl EventEdit {
    pub fn new(event_index: i32) -> Self {
        Self {
            event_index,
            new_text: None,
            new_style: None,
            group: None,
            start_offset_ms: None,
            end_offset_ms: None,
            new_start_ms: None,
            new_end_ms: None,
            new_layer: None,
            new_name: None,
            new_effect: None,
            set_comment: None,
        }
    }

    /// Check if this edit has any actual changes.
    pub fn has_changes(&self) -> bool {
        self.new_text.is_some()
            || self.new_style.is_some()
            || self.group.is_some()
            || self.start_offset_ms.is_some()
            || self.end_offset_ms.is_some()
            || self.new_start_ms.is_some()
            || self.new_end_ms.is_some()
            || self.new_layer.is_some()
            || self.new_name.is_some()
            || self.new_effect.is_some()
            || self.set_comment.is_some()
    }
}

// =============================================================================
// Style Edit
// =============================================================================

/// Planned edit for a subtitle style.
///
/// Only `Some` fields are applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleEdit {
    pub style_name: String,

    // Font changes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_fontname: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_fontsize: Option<f64>,

    // Color changes (ASS format: &HAABBGGRR)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_primary_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_secondary_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_outline_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_back_color: Option<String>,

    // Text decoration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_bold: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_italic: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_underline: Option<i32>,

    // Scaling
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_scale_x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_scale_y: Option<f64>,

    // Spacing and angle
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_spacing: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_angle: Option<f64>,

    // Border
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_border_style: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_outline: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_shadow: Option<f64>,

    // Alignment and margins
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_alignment: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_margin_l: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_margin_r: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_margin_v: Option<i32>,
}

impl StyleEdit {
    pub fn new(style_name: &str) -> Self {
        Self {
            style_name: style_name.to_string(),
            new_fontname: None,
            new_fontsize: None,
            new_primary_color: None,
            new_secondary_color: None,
            new_outline_color: None,
            new_back_color: None,
            new_bold: None,
            new_italic: None,
            new_underline: None,
            new_scale_x: None,
            new_scale_y: None,
            new_spacing: None,
            new_angle: None,
            new_border_style: None,
            new_outline: None,
            new_shadow: None,
            new_alignment: None,
            new_margin_l: None,
            new_margin_r: None,
            new_margin_v: None,
        }
    }

    /// Check if this edit has any actual changes.
    pub fn has_changes(&self) -> bool {
        self.new_fontname.is_some()
            || self.new_fontsize.is_some()
            || self.new_primary_color.is_some()
            || self.new_secondary_color.is_some()
            || self.new_outline_color.is_some()
            || self.new_back_color.is_some()
            || self.new_bold.is_some()
            || self.new_italic.is_some()
            || self.new_underline.is_some()
            || self.new_scale_x.is_some()
            || self.new_scale_y.is_some()
            || self.new_spacing.is_some()
            || self.new_angle.is_some()
            || self.new_border_style.is_some()
            || self.new_outline.is_some()
            || self.new_shadow.is_some()
            || self.new_alignment.is_some()
            || self.new_margin_l.is_some()
            || self.new_margin_r.is_some()
            || self.new_margin_v.is_some()
    }
}

// =============================================================================
// New Event Spec
// =============================================================================

/// Specification for a new event to be added.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewEventSpec {
    pub start_ms: f64,
    pub end_ms: f64,
    pub text: String,
    #[serde(default = "default_style_name")]
    pub style: String,
    #[serde(default)]
    pub layer: i32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub effect: String,
    #[serde(default)]
    pub is_comment: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,

    /// Insert position (index in final event list, or None for append)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insert_at: Option<usize>,
}

fn default_style_name() -> String {
    "Default".to_string()
}

impl NewEventSpec {
    pub fn new(start_ms: f64, end_ms: f64, text: &str) -> Self {
        Self {
            start_ms,
            end_ms,
            text: text.to_string(),
            style: "Default".to_string(),
            layer: 0,
            name: String::new(),
            effect: String::new(),
            is_comment: false,
            group: None,
            insert_at: None,
        }
    }
}

// =============================================================================
// New Style Spec
// =============================================================================

/// Specification for a new style to be added.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewStyleSpec {
    pub name: String,
    #[serde(default = "default_arial")]
    pub fontname: String,
    #[serde(default = "default_fontsize")]
    pub fontsize: f64,
    #[serde(default = "default_primary_color")]
    pub primary_color: String,
    #[serde(default = "default_secondary_color")]
    pub secondary_color: String,
    #[serde(default = "default_outline_color")]
    pub outline_color: String,
    #[serde(default = "default_back_color")]
    pub back_color: String,
    #[serde(default)]
    pub bold: i32,
    #[serde(default)]
    pub italic: i32,
    #[serde(default)]
    pub underline: i32,
    #[serde(default = "default_100f")]
    pub scale_x: f64,
    #[serde(default = "default_100f")]
    pub scale_y: f64,
    #[serde(default)]
    pub spacing: f64,
    #[serde(default)]
    pub angle: f64,
    #[serde(default = "default_1i")]
    pub border_style: i32,
    #[serde(default = "default_2f")]
    pub outline: f64,
    #[serde(default = "default_2f")]
    pub shadow: f64,
    #[serde(default = "default_2i")]
    pub alignment: i32,
    #[serde(default = "default_10i")]
    pub margin_l: i32,
    #[serde(default = "default_10i")]
    pub margin_r: i32,
    #[serde(default = "default_10i")]
    pub margin_v: i32,
    #[serde(default = "default_1i")]
    pub encoding: i32,
}

fn default_arial() -> String { "Arial".to_string() }
fn default_fontsize() -> f64 { 48.0 }
fn default_primary_color() -> String { "&H00FFFFFF".to_string() }
fn default_secondary_color() -> String { "&H000000FF".to_string() }
fn default_outline_color() -> String { "&H00000000".to_string() }
fn default_back_color() -> String { "&H00000000".to_string() }
fn default_100f() -> f64 { 100.0 }
fn default_1i() -> i32 { 1 }
fn default_2i() -> i32 { 2 }
fn default_2f() -> f64 { 2.0 }
fn default_10i() -> i32 { 10 }

impl NewStyleSpec {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            fontname: "Arial".to_string(),
            fontsize: 48.0,
            primary_color: "&H00FFFFFF".to_string(),
            secondary_color: "&H000000FF".to_string(),
            outline_color: "&H00000000".to_string(),
            back_color: "&H00000000".to_string(),
            bold: 0,
            italic: 0,
            underline: 0,
            scale_x: 100.0,
            scale_y: 100.0,
            spacing: 0.0,
            angle: 0.0,
            border_style: 1,
            outline: 2.0,
            shadow: 2.0,
            alignment: 2,
            margin_l: 10,
            margin_r: 10,
            margin_v: 10,
            encoding: 1,
        }
    }
}

// =============================================================================
// Group Definition
// =============================================================================

/// Definition of a custom event group.
///
/// Groups can have associated styles and sync behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupDefinition {
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default = "default_group_color")]
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(default)]
    pub skip_sync: bool,
    #[serde(default)]
    pub description: String,
}

fn default_group_color() -> String {
    "#808080".to_string()
}

impl GroupDefinition {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            display_name: String::new(),
            color: "#808080".to_string(),
            style: None,
            skip_sync: false,
            description: String::new(),
        }
    }

    /// Get display name, falling back to name.
    pub fn effective_display_name(&self) -> &str {
        if self.display_name.is_empty() {
            &self.name
        } else {
            &self.display_name
        }
    }
}

// =============================================================================
// Apply Result
// =============================================================================

/// Result of applying an edit plan.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApplyResult {
    pub success: bool,
    pub events_deleted: i32,
    pub events_modified: i32,
    pub events_added: i32,
    pub styles_deleted: i32,
    pub styles_modified: i32,
    pub styles_added: i32,
    pub global_offset_applied: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ApplyResult {
    /// Total number of changes made.
    pub fn total_changes(&self) -> i32 {
        self.events_deleted
            + self.events_modified
            + self.events_added
            + self.styles_deleted
            + self.styles_modified
            + self.styles_added
            + if self.global_offset_applied { 1 } else { 0 }
    }
}

// =============================================================================
// Subtitle Edit Plan
// =============================================================================

/// Non-destructive edit plan for subtitle modifications.
///
/// Created by editor, saved to JSON, applied at job execution.
///
/// Pipeline order:
/// 1. Editor creates EditPlan (user defines changes)
/// 2. EditPlan saved to temp JSON
/// 3. Job execution:
///    a. Load SubtitleData from source
///    b. Apply EditPlan (this struct)
///    c. Sync/stepping (timing analysis)
///    d. Output final file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleEditPlan {
    /// Source identification
    #[serde(default)]
    pub source_path: String,
    #[serde(default)]
    pub source_format: String,

    /// Event modifications (by original_index)
    #[serde(default)]
    pub event_edits: Vec<EventEdit>,

    /// Events to delete (by original_index)
    #[serde(default)]
    pub deleted_events: HashSet<i32>,

    /// New events to add
    #[serde(default)]
    pub new_events: Vec<NewEventSpec>,

    /// Style modifications
    #[serde(default)]
    pub style_edits: Vec<StyleEdit>,

    /// Styles to delete
    #[serde(default)]
    pub deleted_styles: HashSet<String>,

    /// New styles to add
    #[serde(default)]
    pub new_styles: Vec<NewStyleSpec>,

    /// Group definitions (custom groups)
    #[serde(default)]
    pub group_definitions: Vec<GroupDefinition>,

    /// Global timing offset (applied to all events)
    #[serde(default)]
    pub global_timing_offset_ms: f64,

    /// Metadata
    #[serde(default = "now_iso")]
    pub created_at: String,
    #[serde(default = "now_iso")]
    pub modified_at: String,
    #[serde(default = "default_version")]
    pub version: i32,

    /// Notes/description
    #[serde(default)]
    pub notes: String,
}

fn now_iso() -> String {
    Local::now().to_rfc3339()
}

fn default_version() -> i32 {
    1
}

impl Default for SubtitleEditPlan {
    fn default() -> Self {
        Self::new()
    }
}

impl SubtitleEditPlan {
    pub fn new() -> Self {
        let now = now_iso();
        Self {
            source_path: String::new(),
            source_format: String::new(),
            event_edits: Vec::new(),
            deleted_events: HashSet::new(),
            new_events: Vec::new(),
            style_edits: Vec::new(),
            deleted_styles: HashSet::new(),
            new_styles: Vec::new(),
            group_definitions: Vec::new(),
            global_timing_offset_ms: 0.0,
            created_at: now.clone(),
            modified_at: now,
            version: 1,
            notes: String::new(),
        }
    }

    /// Check if this plan has any actual changes.
    pub fn has_changes(&self) -> bool {
        self.event_edits.iter().any(|e| e.has_changes())
            || !self.deleted_events.is_empty()
            || !self.new_events.is_empty()
            || self.style_edits.iter().any(|s| s.has_changes())
            || !self.deleted_styles.is_empty()
            || !self.new_styles.is_empty()
            || !self.group_definitions.is_empty()
            || self.global_timing_offset_ms != 0.0
    }

    /// Get the edit for a specific event, or None if no edit exists.
    pub fn get_event_edit(&self, event_index: i32) -> Option<&EventEdit> {
        self.event_edits
            .iter()
            .find(|e| e.event_index == event_index)
    }

    /// Get the mutable edit for a specific event, or None if no edit exists.
    pub fn get_event_edit_mut(&mut self, event_index: i32) -> Option<&mut EventEdit> {
        self.event_edits
            .iter_mut()
            .find(|e| e.event_index == event_index)
    }

    /// Add or update an event edit.
    pub fn set_event_edit(&mut self, edit: EventEdit) {
        self.event_edits
            .retain(|e| e.event_index != edit.event_index);
        if edit.has_changes() {
            self.event_edits.push(edit);
        }
        self.modified_at = now_iso();
    }

    /// Mark an event for deletion.
    pub fn mark_event_deleted(&mut self, event_index: i32) {
        self.deleted_events.insert(event_index);
        // Remove any edits for this event
        self.event_edits
            .retain(|e| e.event_index != event_index);
        self.modified_at = now_iso();
    }

    /// Unmark an event for deletion.
    pub fn unmark_event_deleted(&mut self, event_index: i32) {
        self.deleted_events.remove(&event_index);
        self.modified_at = now_iso();
    }

    /// Get the edit for a specific style, or None if no edit exists.
    pub fn get_style_edit(&self, style_name: &str) -> Option<&StyleEdit> {
        self.style_edits
            .iter()
            .find(|s| s.style_name == style_name)
    }

    /// Add or update a style edit.
    pub fn set_style_edit(&mut self, edit: StyleEdit) {
        self.style_edits
            .retain(|s| s.style_name != edit.style_name);
        if edit.has_changes() {
            self.style_edits.push(edit);
        }
        self.modified_at = now_iso();
    }

    /// Add a new event specification.
    pub fn add_new_event(&mut self, spec: NewEventSpec) {
        self.new_events.push(spec);
        self.modified_at = now_iso();
    }

    /// Add a new style specification.
    pub fn add_new_style(&mut self, spec: NewStyleSpec) {
        // Remove if already exists
        self.new_styles.retain(|s| s.name != spec.name);
        self.new_styles.push(spec);
        self.modified_at = now_iso();
    }

    /// Get a group definition by name.
    pub fn get_group(&self, group_name: &str) -> Option<&GroupDefinition> {
        self.group_definitions
            .iter()
            .find(|g| g.name == group_name)
    }

    /// Add or update a group definition.
    pub fn add_group(&mut self, group: GroupDefinition) {
        self.group_definitions.retain(|g| g.name != group.name);
        self.group_definitions.push(group);
        self.modified_at = now_iso();
    }

    /// Get indices of events assigned to a group.
    pub fn get_events_in_group(&self, group_name: &str) -> Vec<i32> {
        self.event_edits
            .iter()
            .filter(|edit| edit.group.as_deref() == Some(group_name))
            .map(|edit| edit.event_index)
            .collect()
    }

    /// Assign multiple events to a group.
    pub fn assign_events_to_group(&mut self, event_indices: &[i32], group_name: &str) {
        for &idx in event_indices {
            let existing = self
                .event_edits
                .iter()
                .position(|e| e.event_index == idx);
            if let Some(pos) = existing {
                self.event_edits[pos].group = Some(group_name.to_string());
            } else {
                let mut edit = EventEdit::new(idx);
                edit.group = Some(group_name.to_string());
                self.event_edits.push(edit);
            }
        }
        self.modified_at = now_iso();
    }

    /// Apply this edit plan to SubtitleData.
    ///
    /// Modifies `data` IN PLACE and returns an `ApplyResult` with statistics.
    pub fn apply(&self, data: &mut SubtitleData, log: Option<&dyn Fn(&str)>) -> ApplyResult {
        let mut result = ApplyResult::default();

        let log_msg = |msg: &str| {
            if let Some(log_fn) = log {
                log_fn(&format!("[EditPlan] {msg}"));
            }
        };

        // 1. Delete events first (before modifying indices)
        if !self.deleted_events.is_empty() {
            let original_len = data.events.len();
            data.events.retain(|event| {
                let idx = event.original_index.unwrap_or(-1);
                !self.deleted_events.contains(&idx)
            });
            result.events_deleted = (original_len - data.events.len()) as i32;
            log_msg(&format!("Deleted {} events", result.events_deleted));
        }

        // 2. Delete styles
        for style_name in &self.deleted_styles {
            let original_len = data.styles.len();
            data.styles.retain(|(n, _)| n != style_name);
            if data.styles.len() < original_len {
                result.styles_deleted += 1;
            }
        }
        if result.styles_deleted > 0 {
            log_msg(&format!("Deleted {} styles", result.styles_deleted));
        }

        // 3. Add new styles
        for spec in &self.new_styles {
            let style = SubtitleStyle {
                name: spec.name.clone(),
                fontname: spec.fontname.clone(),
                fontsize: spec.fontsize,
                primary_color: spec.primary_color.clone(),
                secondary_color: spec.secondary_color.clone(),
                outline_color: spec.outline_color.clone(),
                back_color: spec.back_color.clone(),
                bold: spec.bold,
                italic: spec.italic,
                underline: spec.underline,
                strike_out: 0,
                scale_x: spec.scale_x,
                scale_y: spec.scale_y,
                spacing: spec.spacing,
                angle: spec.angle,
                border_style: spec.border_style,
                outline: spec.outline,
                shadow: spec.shadow,
                alignment: spec.alignment,
                margin_l: spec.margin_l,
                margin_r: spec.margin_r,
                margin_v: spec.margin_v,
                encoding: spec.encoding,
            };
            data.set_style(&spec.name, style);
            result.styles_added += 1;
        }
        if result.styles_added > 0 {
            log_msg(&format!("Added {} new styles", result.styles_added));
        }

        // 4. Apply style edits
        for edit in &self.style_edits {
            if let Some(style) = data.get_style_mut(&edit.style_name) {
                if let Some(ref v) = edit.new_fontname {
                    style.fontname = v.clone();
                }
                if let Some(v) = edit.new_fontsize {
                    style.fontsize = v;
                }
                if let Some(ref v) = edit.new_primary_color {
                    style.primary_color = v.clone();
                }
                if let Some(ref v) = edit.new_secondary_color {
                    style.secondary_color = v.clone();
                }
                if let Some(ref v) = edit.new_outline_color {
                    style.outline_color = v.clone();
                }
                if let Some(ref v) = edit.new_back_color {
                    style.back_color = v.clone();
                }
                if let Some(v) = edit.new_bold {
                    style.bold = v;
                }
                if let Some(v) = edit.new_italic {
                    style.italic = v;
                }
                if let Some(v) = edit.new_underline {
                    style.underline = v;
                }
                if let Some(v) = edit.new_scale_x {
                    style.scale_x = v;
                }
                if let Some(v) = edit.new_scale_y {
                    style.scale_y = v;
                }
                if let Some(v) = edit.new_spacing {
                    style.spacing = v;
                }
                if let Some(v) = edit.new_angle {
                    style.angle = v;
                }
                if let Some(v) = edit.new_border_style {
                    style.border_style = v;
                }
                if let Some(v) = edit.new_outline {
                    style.outline = v;
                }
                if let Some(v) = edit.new_shadow {
                    style.shadow = v;
                }
                if let Some(v) = edit.new_alignment {
                    style.alignment = v;
                }
                if let Some(v) = edit.new_margin_l {
                    style.margin_l = v;
                }
                if let Some(v) = edit.new_margin_r {
                    style.margin_r = v;
                }
                if let Some(v) = edit.new_margin_v {
                    style.margin_v = v;
                }
                result.styles_modified += 1;
            }
        }
        if result.styles_modified > 0 {
            log_msg(&format!("Modified {} styles", result.styles_modified));
        }

        // 5. Apply event edits
        // Build index map for remaining events
        let mut index_to_pos: std::collections::HashMap<i32, usize> =
            std::collections::HashMap::new();
        for (i, event) in data.events.iter().enumerate() {
            let idx = event.original_index.unwrap_or(i as i32);
            index_to_pos.insert(idx, i);
        }

        for edit in &self.event_edits {
            if let Some(&pos) = index_to_pos.get(&edit.event_index) {
                let event = &mut data.events[pos];

                if let Some(ref v) = edit.new_text {
                    event.text = v.clone();
                }
                if let Some(ref v) = edit.new_style {
                    event.style = v.clone();
                }
                if let Some(v) = edit.new_layer {
                    event.layer = v;
                }
                if let Some(ref v) = edit.new_name {
                    event.name = v.clone();
                }
                if let Some(ref v) = edit.new_effect {
                    event.effect = v.clone();
                }
                if let Some(v) = edit.set_comment {
                    event.is_comment = v;
                }

                // Timing adjustments (offsets add to existing)
                if let Some(offset) = edit.start_offset_ms {
                    event.start_ms = (event.start_ms + offset).max(0.0);
                }
                if let Some(offset) = edit.end_offset_ms {
                    event.end_ms = (event.end_ms + offset).max(0.0);
                }

                // Absolute timing (replaces existing)
                if let Some(v) = edit.new_start_ms {
                    event.start_ms = v;
                }
                if let Some(v) = edit.new_end_ms {
                    event.end_ms = v;
                }

                result.events_modified += 1;
            }
        }
        if result.events_modified > 0 {
            log_msg(&format!("Modified {} events", result.events_modified));
        }

        // 6. Add new events
        for spec in &self.new_events {
            let event = SubtitleEvent {
                start_ms: spec.start_ms,
                end_ms: spec.end_ms,
                text: spec.text.clone(),
                style: spec.style.clone(),
                layer: spec.layer,
                name: spec.name.clone(),
                effect: spec.effect.clone(),
                is_comment: spec.is_comment,
                margin_l: 0,
                margin_r: 0,
                margin_v: 0,
                extradata_ids: Vec::new(),
                original_index: None,
                srt_index: None,
                ocr: None,
                sync: None,
                stepping: None,
            };

            if let Some(insert_at) = spec.insert_at {
                if insert_at <= data.events.len() {
                    data.events.insert(insert_at, event);
                } else {
                    data.events.push(event);
                }
            } else {
                data.events.push(event);
            }

            result.events_added += 1;
        }
        if result.events_added > 0 {
            log_msg(&format!("Added {} new events", result.events_added));
        }

        // 7. Apply global timing offset
        if self.global_timing_offset_ms != 0.0 {
            for event in &mut data.events {
                event.start_ms = (event.start_ms + self.global_timing_offset_ms).max(0.0);
                event.end_ms = (event.end_ms + self.global_timing_offset_ms).max(0.0);
            }
            log_msg(&format!(
                "Applied global timing offset: {}ms",
                self.global_timing_offset_ms
            ));
            result.global_offset_applied = true;
        }

        result.success = true;
        result
    }

    /// Save edit plan to JSON file.
    pub fn save(&mut self, path: &Path) -> Result<(), String> {
        self.modified_at = now_iso();
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize edit plan: {e}"))?;
        std::fs::write(path, json)
            .map_err(|e| format!("Failed to write edit plan: {e}"))
    }

    /// Load edit plan from JSON file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read edit plan: {e}"))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse edit plan: {e}"))
    }
}
