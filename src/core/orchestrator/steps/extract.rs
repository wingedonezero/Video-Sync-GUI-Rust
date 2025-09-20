use crate::core::orchestrator::context::Context;

pub struct ExtractStep;

impl ExtractStep {
    pub async fn run(context: Context) -> anyhow::Result<Context> {
        // Extraction logic here
        Ok(context)
    }
}
