// src/types/errors.rs
#[derive(thiserror::Error, Debug)]
pub enum VsgError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Other: {0}")]
    Other(String),
}
