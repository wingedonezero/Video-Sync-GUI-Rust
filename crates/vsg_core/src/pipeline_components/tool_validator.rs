//! Tool validation — 1:1 port of `vsg_core/pipeline_components/tool_validator.py`.

use std::collections::HashMap;

/// Required external tools.
const REQUIRED_TOOLS: &[&str] = &["ffmpeg", "ffprobe", "mkvmerge", "mkvextract", "mkvpropedit"];
/// Optional external tools.
const OPTIONAL_TOOLS: &[&str] = &["videodiff"];

/// Validates and locates required external tools — `ToolValidator`
pub struct ToolValidator;

impl ToolValidator {
    /// Validates that all required tools are available in PATH — `validate_tools`
    ///
    /// Returns a HashMap mapping tool names to their paths.
    /// Returns Err if any required tool is not found.
    pub fn validate_tools() -> Result<HashMap<String, String>, String> {
        let mut tool_paths: HashMap<String, String> = HashMap::new();

        // Validate required tools
        for tool in REQUIRED_TOOLS {
            match which::which(tool) {
                Ok(path) => {
                    tool_paths.insert(tool.to_string(), path.to_string_lossy().to_string());
                }
                Err(_) => {
                    return Err(format!("Required tool '{tool}' not found in PATH."));
                }
            }
        }

        // Optional tools (don't fail if missing)
        for tool in OPTIONAL_TOOLS {
            if let Ok(path) = which::which(tool) {
                tool_paths.insert(tool.to_string(), path.to_string_lossy().to_string());
            }
        }

        Ok(tool_paths)
    }
}
