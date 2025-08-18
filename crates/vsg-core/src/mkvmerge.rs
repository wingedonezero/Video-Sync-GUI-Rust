use anyhow::Result;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenList(pub Vec<String>);

pub fn build_tokens() -> TokenList {
    // TODO: build argv token list in the exact order expected.
    TokenList(vec![])
}

pub fn write_opts_json(path: &str, tokens: &TokenList) -> Result<()> {
    std::fs::write(path, serde_json::to_string_pretty(&tokens.0)?)?;
    Ok(())
}
