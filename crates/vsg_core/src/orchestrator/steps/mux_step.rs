//! Mux step — 1:1 port of `vsg_core/orchestrator/steps/mux_step.py`.

use std::path::PathBuf;

use crate::io::runner::CommandRunner;
use crate::models::jobs::MergePlan;
use crate::mux::options_builder::MkvmergeOptionsBuilder;

use super::context::Context;

/// Builds mkvmerge tokens — `MuxStep`
pub struct MuxStep;

impl MuxStep {
    /// Run the mux planning step.
    pub fn run(&self, ctx: &mut Context, _runner: &CommandRunner) -> Result<(), String> {
        let plan = MergePlan {
            items: ctx.extracted_items.clone().unwrap_or_default(),
            delays: ctx.delays.clone().unwrap_or_default(),
            chapters_xml: ctx.chapters_xml.as_ref().map(PathBuf::from),
            attachments: ctx
                .attachments
                .as_ref()
                .map(|a| a.iter().map(PathBuf::from).collect())
                .unwrap_or_default(),
            subtitle_delays_ms: ctx.subtitle_delays_ms.clone(),
        };

        let tokens = MkvmergeOptionsBuilder::build(&plan, &ctx.settings)?;

        ctx.out_file = None;
        ctx.tokens = Some(tokens);
        Ok(())
    }
}
