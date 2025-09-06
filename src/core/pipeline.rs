// src/core/pipeline.rs
//
// Rust port of vsg_core/pipeline.py (JobPipeline).
// Coordinates analysis, extraction, subtitle processing, chapters, attachments, and mkvmerge.
//

use crate::core::{
    analysis,
    command_runner::CommandRunner,
    mkv_utils,
    subtitle_utils,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map as JsonMap, Value};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone)]
pub struct JobPipeline<'a> {
    pub config: &'a JsonMap<String, Value>,
    pub gui_log: Box<dyn Fn(&str) + Send + Sync + 'a>,
    pub progress: Box<dyn Fn(f32) + Send + Sync + 'a>,
    tool_paths: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub status: String,              // "Analyzed" | "Merged" | "Failed"
    pub name: String,                // ref file name
    pub delay_sec: Option<i64>,      // ms
    pub delay_ter: Option<i64>,      // ms
    pub output: Option<String>,      // merged file path (when merged)
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedTrack {
    pub id: i64,
    #[serde(rename = "type")]
    pub ttype: String,          // "video" | "audio" | "subtitles"
    pub source: String,         // "REF" | "SEC" | "TER"
    pub lang: Option<String>,
    pub codec_id: Option<String>,
    pub name: Option<String>,

    // rule flags (as in Python)
    pub is_default: Option<bool>,
    pub is_forced_display: Option<bool>,
    pub apply_track_name: Option<bool>,
    pub convert_to_ass: Option<bool>,
    pub rescale: Option<bool>,
    pub size_multiplier: Option<f64>,
}

impl<'a> JobPipeline<'a> {
    pub fn new<FLog, FProg>(
        config: &'a JsonMap<String, Value>,
        log_callback: FLog,
        progress_callback: FProg,
    ) -> Self
    where
    FLog: Fn(&str) + Send + Sync + 'a,
    FProg: Fn(f32) + Send + Sync + 'a,
    {
        Self {
            config,
            gui_log: Box::new(log_callback),
            progress: Box::new(progress_callback),
            tool_paths: HashMap::new(),
        }
    }

    fn log(&self, s: &str) {
        (self.gui_log)(s)
    }

    fn set_progress(&self, p: f32) {
        (self.progress)(p.clamp(0.0, 1.0))
    }

    fn find_required_tools(&mut self) -> Result<(), String> {
        for tool in ["ffmpeg", "ffprobe", "mkvmerge", "mkvextract"] {
            let p = which::which(tool)
            .map_err(|_| format!("Required tool '{}' not found in PATH.", tool))?;
            self.tool_paths.insert(tool.to_string(), p.to_string_lossy().to_string());
        }
        if let Ok(p) = which::which("videodiff") {
            self.tool_paths.insert("videodiff".into(), p.to_string_lossy().to_string());
        }
        Ok(())
    }

    /// Main entry: one job (ref + optional sec/ter). Parity with Python.
    pub fn run_job(
        &mut self,
        ref_file: &str,
        sec_file: Option<&str>,
        ter_file: Option<&str>,
        and_merge: bool,
        output_dir_str: &str,
        manual_layout: Option<&[SelectedTrack]>,
    ) -> JobResult {
        let output_dir = PathBuf::from(output_dir_str);
        let _ = fs::create_dir_all(&output_dir);

        let ref_name = Path::new(ref_file).file_name().unwrap_or_default().to_string_lossy().to_string();
        let log_path = output_dir.join(format!("{}.log", Path::new(ref_file).file_stem().unwrap_or_default().to_string_lossy()));

        // route logs to file + GUI
        let mut log_file = match File::create(&log_path) {
            Ok(f) => f,
            Err(e) => {
                // if we cannot write a file, still continue with GUI logs
                self.log(&format!("[WARN] Could not create log file: {}", e));
                // dummy file using in-memory sink (ignored on write errors)
                File::create("/dev/null").unwrap_or_else(|_| File::open(ref_file).unwrap())
            }
        };

        let twin_log = |line: &str, gui: &dyn Fn(&str), file: &mut File| {
            let msg = format!("{}", line.trim_end());
            let _ = writeln!(file, "{}", msg);
            gui(&msg);
        };

        let log_to_all = |s: &str, me: &mut Self, f: &mut File| twin_log(s, &*me.gui_log, f);

        let mut result = JobResult {
            status: "Failed".into(),
            name: ref_name.clone(),
            delay_sec: None,
            delay_ter: None,
            output: None,
            error: None,
        };

        // Create a command runner that uses this pipeline's config and logs through both sinks
        let runner = CommandRunner::new(self.config.clone(), |msg: &str| {
            // CommandRunner will be wrapped below to also tee into file.
            // Here we only ensure GUI receives messages; file write happens via explicit calls.
            (self.gui_log)(msg);
        });

        // Tool discovery
        if let Err(e) = self.find_required_tools() {
            log_to_all(&format!("[ERROR] {}", e), self, &mut log_file);
            result.error = Some(e);
            return result;
        }

        log_to_all(&format!("=== Starting Job: {} ===", ref_name), self, &mut log_file);
        self.set_progress(0.0);

        // Manual-only guardrail: merging requires manual layout
        if and_merge && manual_layout.is_none() {
            let e = "[ERROR] Manual layout required for merge (Manual Selection is the only merge method).";
            log_to_all(e, self, &mut log_file);
            result.error = Some("Manual layout required for merge".into());
            return result;
        }

        // Per-job temp dir
        let temp_root = self
        .config
        .get("temp_root")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "temp_work".into());
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let job_temp = PathBuf::from(&temp_root).join(format!("job_{}_{}", Path::new(ref_file).file_stem().unwrap_or_default().to_string_lossy(), ts));
        let _ = fs::create_dir_all(&job_temp);

        let mut merge_ok = false;
        let mut need_cleanup = true;

        // Recreate a runner that tees output into both GUI and file with timestamps (as CommandRunner does)
        let tee_runner = CommandRunner::new(self.config.clone(), {
            let gui = self.gui_log.clone();
            let mut file_sink = log_file.try_clone().ok();
            move |line: &str| {
                if let Some(ref mut f) = file_sink {
                    let _ = writeln!(f, "{}", line);
                }
                (gui)(line);
            }
        });

        // ---- Analysis phase ----
        log_to_all("--- Analysis Phase ---", self, &mut log_file);

        let (delay_sec, delay_ter) = self.run_analysis(ref_file, sec_file, ter_file, &tee_runner);
        result.delay_sec = delay_sec;
        result.delay_ter = delay_ter;

        if !and_merge {
            log_to_all("--- Analysis Complete (No Merge) ---", self, &mut log_file);
            self.set_progress(1.0);
            result.status = "Analyzed".into();
            // no output path for analysis-only
            log_to_all("=== Job Finished ===", self, &mut log_file);
            return result;
        }

        // ---- Merge planning ----
        log_to_all("--- Merge Planning Phase ---", self, &mut log_file);
        self.set_progress(0.25);

        let mut delays = json!({
            "secondary_ms": delay_sec.unwrap_or(0),
                               "tertiary_ms": delay_ter.unwrap_or(0)
        });

        let mut present_delays = vec![0];
        if let Some(d) = delay_sec { present_delays.push(d) }
        if let Some(d) = delay_ter { present_delays.push(d) }
        let min_delay = *present_delays.iter().min().unwrap_or(&0);
        let global_shift = if min_delay < 0 { -min_delay } else { 0 };

        delays["_global_shift"] = json!(global_shift);

        log_to_all(&format!("[Delay] Raw delays (ms): ref=0, sec={}, ter={}",
                            delays["secondary_ms"], delays["tertiary_ms"]),
                   self, &mut log_file);
        log_to_all(&format!("[Delay] Applying lossless global shift: +{} ms", global_shift),
                   self, &mut log_file);

        // ---- Extraction phase ----
        log_to_all("--- Extraction Phase ---", self, &mut log_file);
        self.set_progress(0.40);

        // Prepare IDs from manual layout
        let manual_layout = manual_layout.unwrap(); // guaranteed by earlier guard
        let ref_ids: Vec<i64> = manual_layout.iter()
        .filter(|t| t.source.eq_ignore_ascii_case("REF"))
        .map(|t| t.id).collect();
        let sec_ids: Vec<i64> = manual_layout.iter()
        .filter(|t| t.source.eq_ignore_ascii_case("SEC"))
        .map(|t| t.id).collect();
        let ter_ids: Vec<i64> = manual_layout.iter()
        .filter(|t| t.source.eq_ignore_ascii_case("TER"))
        .map(|t| t.id).collect();

        log_to_all(&format!("Manual selection: preparing to extract {} REF, {} SEC, {} TER tracks.",
                            ref_ids.len(), sec_ids.len(), ter_ids.len()),
                   self, &mut log_file);

        let mut all_tracks = vec![];

        let ref_ext = mkv_utils::extract_tracks(
            ref_file,
            &job_temp,
            &tee_runner,
            "ref",
            true,  // audio (ignored when specific_tracks provided)
        true,  // subs
        false, // all_tracks
        Some(&ref_ids),
        );
        all_tracks.extend(ref_ext.clone());

        let mut sec_ext = vec![];
        if let (Some(sec), true) = (sec_file, !sec_ids.is_empty()) {
            sec_ext = mkv_utils::extract_tracks(
                sec,
                &job_temp,
                &tee_runner,
                "sec",
                true,
                true,
                false,
                Some(&sec_ids),
            );
            all_tracks.extend(sec_ext.clone());
        }
        let mut ter_ext = vec![];
        if let (Some(ter), true) = (ter_file, !ter_ids.is_empty()) {
            ter_ext = mkv_utils::extract_tracks(
                ter,
                &job_temp,
                &tee_runner,
                "ter",
                true,
                true,
                false,
                Some(&ter_ids),
            );
            all_tracks.extend(ter_ext.clone());
        }

        // Build map source_id -> extracted track
        let mut extracted_map: HashMap<String, mkv_utils::extract::ExtractedTrack> = HashMap::new();
        for t in all_tracks.iter() {
            extracted_map.insert(format!("{}_{}", t.source.to_uppercase(), t.id), t.clone());
        }

        // Build final plan from manual layout
        let plan = self.build_plan_from_manual_layout(manual_layout, &delays, &extracted_map);

        // ---- Subtitle processing phase ----
        log_to_all("--- Subtitle Processing Phase ---", self, &mut log_file);

        for item in plan.plan.iter_mut() {
            let t = &mut item.track;
            let rule = &item.rule;

            if t.ttype == "subtitles" {
                if rule.convert_to_ass.unwrap_or(false) {
                    let new_path = subtitle_utils::convert_srt_to_ass(&t.path, &tee_runner);
                    t.path = new_path;
                }
                if rule.rescale.unwrap_or(false) {
                    let _ = subtitle_utils::rescale_subtitle(&t.path, ref_file, &tee_runner);
                }
                let mult = rule.size_multiplier.unwrap_or(1.0);
                if (mult - 1.0).abs() > f64::EPSILON {
                    let _ = subtitle_utils::multiply_font_size(&t.path, mult, &tee_runner);
                }
            }
        }

        // Attachments from TER
        let ter_attachments = if let Some(ter) = ter_file {
            mkv_utils::extract_attachments(ter, &job_temp, &tee_runner, "ter")
        } else { vec![] };

        // Chapters from REF (with global shift)
        let chapters_xml = mkv_utils::process_chapters(
            ref_file, &job_temp, &tee_runner, self.config, global_shift
        );

        // ---- Merge execution phase ----
        log_to_all("--- Merge Execution Phase ---", self, &mut log_file);
        self.set_progress(0.60);

        let out_file = PathBuf::from(output_dir_str).join(Path::new(ref_file).file_name().unwrap());
        let tokens = self.build_mkvmerge_tokens(&plan, out_file.to_string_lossy().as_ref(), chapters_xml.as_ref(), &ter_attachments);
        self.set_progress(0.80);

        match self.write_mkvmerge_opts(&tokens, &job_temp, &tee_runner) {
            Ok(opts_path) => {
                let cmd = ["mkvmerge".to_string(), format!("@{}", opts_path)];
                let refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
                merge_ok = tee_runner.run(&refs).is_some();
                if !merge_ok {
                    log_to_all("[FATAL ERROR] Job failed: mkvmerge execution failed.", self, &mut log_file);
                    result.error = Some("mkvmerge execution failed".into());
                }
            }
            Err(e) => {
                log_to_all(&format!("[FATAL ERROR] Job failed: {}", e), self, &mut log_file);
                result.error = Some(e);
            }
        }

        if merge_ok {
            log_to_all(&format!("[SUCCESS] Output file created: {}", out_file.display()), self, &mut log_file);
            self.set_progress(1.0);
            result.status = "Merged".into();
            result.output = Some(out_file.to_string_lossy().to_string());
        }

        // Cleanup
        if !and_merge || merge_ok {
            let _ = fs::remove_dir_all(&job_temp);
        }
        log_to_all("=== Job Finished ===", self, &mut log_file);

        result
    }

    fn run_analysis(
        &self,
        ref_file: &str,
        sec_file: Option<&str>,
        ter_file: Option<&str>,
        runner: &CommandRunner,
    ) -> (Option<i64>, Option<i64>) {
        let mut delay_sec = None;
        let mut delay_ter = None;

        let mode = self
        .config
        .get("analysis_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("Audio Correlation");

        let log_mode = |role: &str, me: &Self| {
            me.log(&format!("Analyzing {} file ({})...", role, mode));
        };

        if let Some(sec) = sec_file {
            log_mode("Secondary", self);
            delay_sec = analysis::analyze(ref_file, Some(sec), self.config, runner);
            self.log(&format!("Secondary delay determined: {} ms", delay_sec.unwrap_or(0)));
        }
        if let Some(ter) = ter_file {
            log_mode("Tertiary", self);
            delay_ter = analysis::analyze(ref_file, Some(ter), self.config, runner);
            self.log(&format!("Tertiary delay determined: {} ms", delay_ter.unwrap_or(0)));
        }

        (delay_sec, delay_ter)
    }

    // -------- Plan building (from manual layout) --------

    #[derive(Debug, Clone)]
    struct PlanItem {
        track: mkv_utils::extract::ExtractedTrack,
        rule: SelectedTrack,
    }

    #[derive(Debug, Clone)]
    struct MergePlan {
        plan: Vec<PlanItem>,
        delays: Value, // { secondary_ms, tertiary_ms, _global_shift }
    }

    fn build_plan_from_manual_layout(
        &self,
        manual_layout: &[SelectedTrack],
        delays: &Value,
        extracted_map: &HashMap<String, mkv_utils::extract::ExtractedTrack>,
    ) -> MergePlan {
        self.log(&format!("--- Building merge plan from {} manual selections ---", manual_layout.len()));
        let mut final_plan: Vec<PlanItem> = vec![];

        for sel in manual_layout {
            let key = format!("{}_{}", sel.source.to_uppercase(), sel.id);
            if let Some(ext) = extracted_map.get(&key) {
                final_plan.push(PlanItem {
                    track: ext.clone(),
                                rule: sel.clone(),
                });
            } else {
                self.log(&format!("[WARNING] Could not find extracted file for {}. Skipping.", key));
            }
        }

        MergePlan {
            plan: final_plan,
            delays: delays.clone(),
        }
    }

    // -------- mkvmerge tokens --------

    fn build_mkvmerge_tokens(
        &self,
        plan: &MergePlan,
        output_file: &str,
        chapters_xml: Option<&PathBuf>,
        attachments: &[String],
    ) -> Vec<String> {
        // harmless guardrails for video
        let video_items: Vec<&PlanItem> = plan.plan.iter().filter(|it| it.track.ttype == "video").collect();
        if !video_items.is_empty() {
            if !video_items.iter().any(|it| it.track.source.to_uppercase() == "REF") {
                self.log("[WARN] No REF video present in final plan. If this was intended (audio-only), ignore this warning.");
            }
            if video_items.iter().any(|it| it.track.source.to_uppercase() != "REF") {
                self.log("[WARN] Non-REF video detected in final plan (SEC/TER). The UI should prevent this; proceeding anyway.");
            }
        }

        let mut tokens: Vec<String> = vec!["--output".into(), output_file.into()];

        if let Some(ch) = chapters_xml {
            tokens.push("--chapters".into());
            tokens.push(ch.to_string_lossy().to_string());
        }

        if self
            .config
            .get("disable_track_statistics_tags")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            {
                tokens.push("--disable-track-statistics-tags".into());
            }

            let delays = &plan.delays;
        let global_shift = delays
        .get("_global_shift")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

        // Determine first video index and default indices
        let mut default_audio = -1isize;
        let mut default_subs = -1isize;
        let mut forced_subs = -1isize;
        let mut first_video_idx = -1isize;

        for (i, item) in plan.plan.iter().enumerate() {
            if item.track.ttype == "video" && first_video_idx < 0 {
                first_video_idx = i as isize;
            }
            if item.rule.is_default.unwrap_or(false) {
                match item.track.ttype.as_str() {
                    "audio" if default_audio < 0 => default_audio = i as isize,
                    "subtitles" if default_subs < 0 => default_subs = i as isize,
                    _ => {}
                }
            }
            if item.rule.is_forced_display.unwrap_or(false) && item.track.ttype == "subtitles" && forced_subs < 0 {
                forced_subs = i as isize;
            }
        }

        let mut track_order_indices: Vec<String> = vec![];

        for (i, item) in plan.plan.iter().enumerate() {
            let t = &item.track;
            let r = &item.rule;

            // Compute per-track delay (global + sec/ter)
            let mut delay = global_shift;
            match t.source.to_lowercase().as_str() {
                "sec" => {
                    delay += delays.get("secondary_ms").and_then(|v| v.as_i64()).unwrap_or(0);
                }
                "ter" => {
                    delay += delays.get("tertiary_ms").and_then(|v| v.as_i64()).unwrap_or(0);
                }
                _ => {}
            }

            let is_default = (i as isize == first_video_idx)
            || (i as isize == default_audio)
            || (i as isize == default_subs);

            // --language
            tokens.push("--language".into());
            tokens.push(format!("0:{}", t.lang));

            // --track-name (optional)
            if r.apply_track_name.unwrap_or(false) && !t.name.is_empty() {
                tokens.push("--track-name".into());
                tokens.push(format!("0:{}", t.name));
            }

            // --sync (delay)
            tokens.push("--sync".into());
            tokens.push(format!("0:{}", delay));

            // --default-track-flag
            tokens.push("--default-track-flag".into());
            tokens.push(format!("0:{}", if is_default { "yes" } else { "no" }));

            // --forced-display-flag for subtitles when selected
            if (i as isize) == forced_subs && t.ttype == "subtitles" {
                tokens.push("--forced-display-flag".into());
                tokens.push("0:yes".into());
            }

            // --compression 0:none
            tokens.push("--compression".into());
            tokens.push("0:none".into());

            // remove dialog normalization gain (AC3/E-AC3)
            if self
                .config
                .get("apply_dialog_norm_gain")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                && t.ttype == "audio"
                {
                    let cid = t.codec_id.to_uppercase();
                    if cid.contains("AC3") || cid.contains("EAC3") {
                        tokens.push("--remove-dialog-normalization-gain".into());
                        tokens.push("0".into());
                    }
                }

                // ( path )
                tokens.push("(".into());
                tokens.push(t.path.clone());
                tokens.push(")".into());

                // Track order: i:0
                track_order_indices.push(format!("{}:0", i));
        }

        for a in attachments {
            tokens.push("--attach-file".into());
            tokens.push(a.clone());
        }

        if !track_order_indices.is_empty() {
            tokens.push("--track-order".into());
            tokens.push(track_order_indices.join(","));
        }

        tokens
    }

    fn write_mkvmerge_opts(
        &self,
        tokens: &[String],
        temp_dir: &Path,
        runner: &CommandRunner,
    ) -> Result<String, String> {
        let opts_path = temp_dir.join("opts.json");
        let json_str = serde_json::to_string(tokens).map_err(|e| format!("Failed to serialize tokens: {}", e))?;
        fs::write(&opts_path, &json_str).map_err(|e| format!("Failed to write mkvmerge options file: {}", e))?;
        runner.log(&format!("mkvmerge options file written to: {}", opts_path.display()));

        if self
            .config
            .get("log_show_options_pretty")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            {
                let pretty_path = temp_dir.join("opts.pretty.txt");
                let pretty = tokens.join(" \\\n  ");
                if let Err(e) = fs::write(&pretty_path, pretty.as_bytes()) {
                    return Err(format!("Failed to write pretty mkvmerge options: {}", e));
                }
                runner.log(&format!(
                    "--- mkvmerge options (pretty) ---\n{}\n-------------------------------",
                                    fs::read_to_string(pretty_path).unwrap_or_default()
                ));
            }

            Ok(opts_path.to_string_lossy().to_string())
    }
}
