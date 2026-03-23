//! Subtitle image parsers for bitmap-based subtitle formats.
//!
//! Provides parsers for extracting subtitle images from:
//! - VobSub (.sub/.idx) - DVD subtitle format
//! - PGS (.sup) - Blu-ray subtitle format (future)

pub mod base;
pub mod vobsub;

pub use base::{SubtitleImage, ParseResult, SubtitleImageParser};
pub use vobsub::VobSubParser;
