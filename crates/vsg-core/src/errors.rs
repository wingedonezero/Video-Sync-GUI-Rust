use thiserror::Error;

#[derive(Debug, Error)]
pub enum VsgError {
    #[error("Tool not found: {0}")]
    ToolMissing(String),

    #[error("Process failed: {tool} (code {code:?})")]
    ProcessFailed { tool: String, code: Option<i32> },

    #[error("Invalid data: {0}")]
    InvalidData(String),
}
