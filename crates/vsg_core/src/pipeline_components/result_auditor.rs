//! Result auditor — 1:1 port of `vsg_core/pipeline_components/result_auditor.py`.
//!
//! Wraps the FinalAuditor for post-merge validation.

use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::orchestrator::steps::context::Context;
use crate::postprocess::final_auditor::FinalAuditor;

/// Audits merged output files for quality and correctness — `ResultAuditor`
pub struct ResultAuditor;

impl ResultAuditor {
    /// Audits the merged output file — `audit_output`
    ///
    /// Returns the number of issues found (0 = no issues).
    pub fn audit_output(
        output_file: &Path,
        ctx: &Context,
        runner: &CommandRunner,
        log_callback: &dyn Fn(&str),
    ) -> i32 {
        log_callback("--- Post-Merge: Running Final Audit ---");

        let issues = FinalAuditor::run(ctx, runner, output_file);

        if issues > 0 {
            log_callback(&format!("[Audit] Found {issues} issue(s) in final output."));
        }

        issues
    }
}
