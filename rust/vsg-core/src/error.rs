use thiserror::Error;

#[derive(Error, Debug)]
pub enum VsgError {
  #[error("IO error: {0}")]
  Io(#[from] std::io::Error),
  #[error("Serde JSON error: {0}")]
  SerdeJson(#[from] serde_json::Error),
  #[error("Process error: {0}")]
  Process(String),
  #[error("Other: {0}")]
  Other(String),
}
