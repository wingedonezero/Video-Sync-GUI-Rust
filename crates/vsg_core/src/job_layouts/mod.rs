//! Job layout management — 1:1 port of `vsg_core/job_layouts/`.
//!
//! Orchestrates persistent track layout management for batch processing jobs
//! with identical file structures. Enables reusing user-configured track orders
//! and settings across multiple files.
//!
//! Architecture:
//! - `signature::EnhancedSignatureGenerator` — creates track and structure signatures
//! - `persistence::LayoutPersistence` — handles JSON storage/loading
//! - `validation::LayoutValidator` — ensures loaded layouts are well-formed
//! - `manager::JobLayoutManager` — main API coordinating all operations

pub mod manager;
pub mod persistence;
pub mod signature;
pub mod validation;

pub use manager::JobLayoutManager;
