// src/main.rs

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use vsg_core::{config::Config, job_discovery, pipeline::{self, ManualTrack}};

/// Command-line arguments for the Video Sync & Merge tool.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to the reference file or directory.
    #[arg(short, long)]
    ref_file: PathBuf,

    /// Path to the secondary file or directory.
    #[arg(short, long)]
    sec_file: Option<PathBuf>,

    /// Path to the tertiary file or directory.
    #[arg(short, long)]
    ter_file: Option<PathBuf>,

    /// Path to the output directory. Defaults to 'sync_output' in the app folder.
    #[arg(short, long)]
    output_dir: Option<PathBuf>,

    /// Perform analysis only; do not merge files.
    #[arg(long, default_value_t = false)]
    analyze_only: bool,

    /// A string defining the tracks to include in the merge, e.g., "REF:v0,a1;SEC:a0"
    #[arg(long, help = "Define track layout, e.g., 'REF:v0,s1;SEC:a0'")]
    layout: Option<String>,
}

fn main() -> Result<()> {
    // 1. Load configuration from settings.json
    let config = Config::load();

    // 2. Parse command-line arguments
    let cli = Cli::parse();

    // 3. Set up a thread-safe logger that prints to the console
    let log_callback = Arc::new(Mutex::new(|message: String| {
        println!("{}", message);
    }));

    // 4. Discover jobs based on input paths
    log_callback.lock().unwrap()("Discovering jobs...".to_string());
    let jobs = job_discovery::discover_jobs(
        cli.ref_file.to_str().unwrap(),
                                            cli.sec_file.as_deref().and_then(|p| p.to_str()),
                                            cli.ter_file.as_deref().and_then(|p| p.to_str()),
    )?;
    log_callback.lock().unwrap()(format!("Found {} job(s) to process.", jobs.len()));

    if jobs.is_empty() {
        return Ok(());
    }

    // 5. Determine the output directory
    let output_dir = cli.output_dir.unwrap_or_else(|| config.output_folder.clone());
    std::fs::create_dir_all(&output_dir)?;

    // 6. Parse the manual layout from the command line (this is a simplified parser)
    // In a real GUI, this would come from the selection dialog.
    let manual_layout = parse_layout_string(cli.layout.as_deref().unwrap_or(""))?;

    // 7. Initialize and run the pipeline for each job
    let pipeline = pipeline::JobPipeline::new(config);

    for (i, job) in jobs.iter().enumerate() {
        log_callback.lock().unwrap()(format!(
            "\n--- Starting Job {}/{} ({}) ---",
                                             i + 1,
                                             jobs.len(),
                                             job.ref_file.display()
        ));

        pipeline.run_job(
            &job.ref_file,
            job.sec_file.as_deref(),
                         job.ter_file.as_deref(),
                         !cli.analyze_only,
                         &manual_layout,
                         &output_dir,
                         Arc::clone(&log_callback),
        )?;
    }

    log_callback.lock().unwrap()("\nAll jobs completed successfully.".to_string());
    Ok(())
}

/// A simple parser for the --layout command-line argument.
/// Format: "SOURCE:tID,tID;SOURCE:tID..." e.g., "REF:v0,a1;SEC:a0"
fn parse_layout_string(layout_str: &str) -> Result<Vec<ManualTrack>> {
    if layout_str.is_empty() {
        // In a real CLI, we might want to probe and select all tracks by default.
        // For now, we require an explicit layout for merging.
        return Ok(Vec::new());
    }

    let mut layout = Vec::new();
    for source_part in layout_str.split(';') {
        let parts: Vec<&str> = source_part.split(':').collect();
        if parts.len() != 2 { continue; }

        let source = parts[0].to_uppercase();
        let track_specs = parts[1];

        for track_spec in track_specs.split(',') {
            if track_spec.len() < 2 { continue; }
            let track_type_char = track_spec.chars().next().unwrap();
            let track_id_str = &track_spec[1..];

            let track_type = match track_type_char {
                'v' => "video",
                'a' => "audio",
                's' => "subtitles",
                _ => continue,
            }.to_string();

            let id = track_id_str.parse::<u64>()?;

            // CLI parsing can be expanded to include flags like --default, --forced, etc.
            // For now, we use sensible defaults.
            layout.push(ManualTrack {
                source: source.clone(),
                        id,
                        track_type,
                        is_default: true, // Simplified for CLI
                        is_forced_display: false,
                        apply_track_name: true,
                        convert_to_ass: true,
                        rescale: true,
                        size_multiplier: 1.0,
            });
        }
    }
    Ok(layout)
}
