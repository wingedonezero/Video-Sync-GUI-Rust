use anyhow::{anyhow, Context, Result};
use imgui::*;
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use raw_window_handle::HasRawWindowHandle;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use winit::dpi::LogicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;

use vsg_core::analyze::audio_xcorr::{analyze_audio_xcorr_detailed, Band, Method, StereoMode, XCorrParams};
use vsg_core::extract::run::run_mkvextract;
use vsg_core::fsutil::{default_output_dir, default_work_dir};
use vsg_core::model::{SelectionEntry, SelectionManifest};

mod config;
use config::{ExtractionStrategy, Settings};

fn exe_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_else(|| std::env::current_dir().unwrap())
}

fn ensure_dir(p: &Path) -> Result<()> {
    fs::create_dir_all(p).with_context(|| format!("create dir {}", p.display()))
}

fn probe_tracks_with_mkvmerge(input: &str) -> Result<Vec<(u32, String, String)>> {
    // returns (id, codec/codec_id lowercased, language or "und")
    let out = std::process::Command::new("mkvmerge").arg("-J").arg(input).output().context("spawn mkvmerge -J")?;
    if !out.status.success() {
        return Err(anyhow!("mkvmerge -J failed: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let mut res = vec![];
    if let Some(arr) = v.get("tracks").and_then(|t| t.as_array()) {
        for t in arr {
            if t.get("type").and_then(|x| x.as_str()) == Some("audio") {
                let id = t.get("id").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                let codec = t.get("codec_id").and_then(|x| x.as_str())
                    .or_else(|| t.get("codec").and_then(|x| x.as_str()))
                    .unwrap_or("unknown").to_lowercase();
                let lang = t.get("properties")
                    .and_then(|p| p.get("language").or_else(|| p.get("language_ietf")))
                    .and_then(|x| x.as_str())
                    .unwrap_or("und").to_lowercase();
                res.push((id, codec, lang));
            }
        }
    }
    Ok(res)
}

fn pick_ext_from_codec(codec: &str) -> &'static str {
    match codec {
        "aac" | "a_aac" | "mp4a" | "a_aac_mpeg2lc" | "a_aac_mpeg4lc" => "aac",
        "ac3" | "a_ac3" => "ac3",
        "eac3" | "e-ac-3" | "a_eac3" | "a_e-ac-3" => "eac3",
        "dts" | "a_dts" | "a_dts_hd" | "a_dts-x" => "dts",
        "truehd" | "a_truehd" => "thd",
        "flac" | "a_flac" => "flac",
        "opus" | "a_opus" => "opus",
        "vorbis" | "a_vorbis" => "ogg",
        other => {
            if other.contains("opus") { "opus" }
            else if other.contains("flac") { "flac" }
            else if other.contains("ac-3") { "ac3" }
            else { "bin" }
        }
    }
}

fn build_manifest(ref_file:&str, sec_file:Option<&str>, ter_file:Option<&str>) -> Result<SelectionManifest> {
    let ref_tracks = probe_tracks_with_mkvmerge(ref_file)?;
    let mut ref_first_lang = "und".to_string();
    let mut ref_entries: Vec<SelectionEntry> = vec![];
    for (i,(id, codec, lang)) in ref_tracks.iter().enumerate() {
        if i == 0 { ref_first_lang = lang.clone(); }
        ref_entries.push(SelectionEntry{ file_path: ref_file.into(), track_id: *id, r#type: "audio".into(), language: Some(lang.clone()), codec: Some(codec.clone()) });
    }
    let target_lang = ref_first_lang;

    let mut pick_side = |file_opt:Option<&str>| -> Result<Vec<SelectionEntry>> {
        if let Some(file) = file_opt {
            let tracks = probe_tracks_with_mkvmerge(file)?;
            let mut first: Option<(u32,String,String)> = None;
            let mut best: Option<(u32,String,String)> = None;
            for (id, codec, lang) in tracks {
                if first.is_none() { first = Some((id, codec.clone(), lang.clone())); }
                if lang == target_lang { best = Some((id, codec.clone(), lang.clone())); break; }
            }
            let (id, codec, lang) = best.or(first).ok_or_else(|| anyhow!("no audio tracks in {}", file))?;
            Ok(vec![ SelectionEntry{ file_path: file.into(), track_id:id, r#type:"audio".into(), language: Some(lang), codec: Some(codec) } ])
        } else {
            Ok(vec![])
        }
    };

    let sec_entries = pick_side(sec_file)?;
    let ter_entries = pick_side(ter_file)?;

    Ok(SelectionManifest{ ref_tracks: ref_entries, sec_tracks: sec_entries, ter_tracks: ter_entries })
}

fn extract_if_needed(sel:&SelectionManifest, work:&Path, strategy:ExtractionStrategy) -> Result<()> {
    let have_any = work.join("ref").exists() || work.join("sec").exists() || work.join("ter").exists();
    match strategy {
        ExtractionStrategy::DecodeDirect => Ok(()), // skip extraction entirely
        ExtractionStrategy::ReuseOnly => {
            if have_any { Ok(()) } else { Err(anyhow!("No extracted audio present under {}", work.display())) }
        }
        ExtractionStrategy::Auto => {
            if have_any { Ok(()) } else { run_mkvextract(sel, work).context("mkvextract") }
        }
        ExtractionStrategy::ForceExtract => run_mkvextract(sel, work).context("mkvextract"),
    }
}

fn do_analyze(ref_audio:&str, other_audio:&str, duration_s:f64, params:&XCorrParams) -> Result<(i128, i64, Vec<serde_json::Value>)> {
    let (res, chunks) = analyze_audio_xcorr_detailed(ref_audio, other_audio, duration_s, params)?;
    let chunks_json: Vec<_> = chunks.into_iter().map(|c| json!({
        "center_s": c.center_s, "window_samples": c.window_samples,
        "lag_ns": c.lag_ns, "lag_ms": c.lag_ms, "peak": c.peak
    })).collect();
    Ok((res.delay_ns, res.delay_ms, chunks_json))
}

fn first_audio_under(dir:&Path) -> Option<String> {
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.contains("_audio.") {
                return Some(e.path().to_string_lossy().to_string());
            }
        }
    }
    None
}

fn main() -> Result<()> {
    // --- imgui + winit setup ---
    let event_loop = EventLoop::new()?;
    let window = WindowBuilder::new()
        .with_title("VSG GUI - Analyze")
        .with_inner_size(LogicalSize::new(1100.0, 700.0))
        .build(&event_loop)?;

    let mut imgui = imgui::Context::create();
    imgui.set_ini_filename(None);
    let mut platform = WinitPlatform::new(&mut imgui);
    platform.attach_window(imgui.io_mut(), &window, HiDpiMode::Default);

    // simple glow GL context
    let (gl, mut renderer) = {
        use glow::HasContext as _;
        let raw = unsafe {
            glow::Context::from_loader_function(|s| window.get_proc_address(s) as *const _)
        };
        let renderer = imgui_glow_renderer::AutoRenderer::initialize(&mut imgui, &raw, |s| window.get_proc_address(s) as *const _)
            .map_err(|e| anyhow!("renderer init: {:?}", e))?;
        (raw, renderer)
    };

    // settings
    let settings_path = exe_dir().join("vsg_settings.json");
    let mut settings: Settings = fs::read_to_string(&settings_path).ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();

    let mut log_lines: Vec<String> = Vec::new();
    let mut busy = false;
    let mut last_result: Option<String> = None;

    let mut last_frame = Instant::now();
    event_loop.run(move |event, elwt| {
        platform.handle_event(imgui.io_mut(), &window, &event);
        match event {
            Event::NewEvents(_) => {}
            Event::MainEventsCleared => {
                window.request_redraw();
            }
            Event::RedrawRequested(_) => {
                let io = imgui.io_mut();
                platform.prepare_frame(io, &window).unwrap();
                let delta = last_frame.elapsed();
                io.update_delta_time(delta);
                last_frame = Instant::now();

                let mut ui = imgui.frame();

                ui.window("Analysis")
                    .size([1060.0, 300.0], Condition::FirstUseEver)
                    .build(|| {
                        ui.input_text("REF", &mut settings.ref_path).build();
                        ui.input_text("SEC", &mut settings.sec_path).build();
                        ui.input_text("TER", &mut settings.ter_path).build();
                        ui.separator();
                        ui.input_text("Work dir (blank = exe/tmp_work)", &mut settings.work_dir).build();
                        ui.input_text("Out dir (blank = exe/out)", &mut settings.out_dir).build();
                        ui.separator();

                        ui.text("Options:");
                        ui.input_scalar("Chunks", &mut settings.chunks).build();
                        ui.input_scalar("Chunk dur (s)", &mut settings.chunk_dur_s).build();
                        ui.input_text("Sample rate (s12000/s24000/s48000)", &mut settings.sample_rate).build();
                        ui.input_text("Stereo (mono/left/right/mid/best)", &mut settings.stereo_mode).build();
                        ui.input_text("Method (fft/compat)", &mut settings.method).build();
                        ui.input_text("Band (none/voice)", &mut settings.band).build();

                        ui.separator();
                        let mut strategy_idx = match settings.strategy {
                            ExtractionStrategy::Auto => 0,
                            ExtractionStrategy::ForceExtract => 1,
                            ExtractionStrategy::ReuseOnly => 2,
                            ExtractionStrategy::DecodeDirect => 3,
                        };
                        let items = ["Auto", "ForceExtract", "ReuseOnly", "DecodeDirect"];
                        if ComboBox::new("Extraction Strategy").build_simple_string(&ui, &mut strategy_idx, &items, |s| s) {
                            settings.strategy = match strategy_idx {
                                1 => ExtractionStrategy::ForceExtract,
                                2 => ExtractionStrategy::ReuseOnly,
                                3 => ExtractionStrategy::DecodeDirect,
                                _ => ExtractionStrategy::Auto,
                            };
                        }

                        if ui.button("Save Settings") {
                            let _ = fs::write(&settings_path, serde_json::to_string_pretty(&settings).unwrap());
                            log_lines.push("Settings saved.".into());
                        }
                        ui.same_line();
                        if ui.button("Analyze") && !busy {
                            busy = true;
                            log_lines.push("Starting analysis...".into());
                            let s = settings.clone();
                            let win = window.clone();
                            // Spawn worker thread
                            std::thread::spawn(move || {
                                let run = || -> Result<String> {
                                    // Resolve dirs
                                    let exe = exe_dir();
                                    let work = if s.work_dir.trim().is_empty() { exe.join("tmp_work") } else { PathBuf::from(&s.work_dir) };
                                    let out = if s.out_dir.trim().is_empty() { exe.join("out") } else { PathBuf::from(&s.out_dir) };
                                    ensure_dir(&work)?; ensure_dir(&out)?;
                                    let manifest_dir = work.join("manifest");
                                    ensure_dir(&manifest_dir)?;

                                    // Build or load manifest
                                    let sel_path = manifest_dir.join("selection.json");
                                    let sel = if sel_path.exists() {
                                        let t = fs::read_to_string(&sel_path)?;
                                        serde_json::from_str::<SelectionManifest>(&t)?
                                    } else {
                                        let sel = build_manifest(&s.ref_path, (!s.sec_path.is_empty()).then(|| s.sec_path.as_str()), (!s.ter_path.is_empty()).then(|| s.ter_path.as_str()))?;
                                        fs::write(&sel_path, serde_json::to_string_pretty(&sel)?)?;
                                        sel
                                    };

                                    // Extract per strategy
                                    extract_if_needed(&sel, &work, s.strategy)?;

                                    // Resolve audio files for analysis
                                    let ref_audio = first_audio_under(&work.join("ref"))
                                        .or_else(|| (!s.ref_path.is_empty()).then(|| s.ref_path.clone()))
                                        .ok_or_else(|| anyhow!("no REF audio"))?;
                                    let mut results = serde_json::json!({
                                        "meta":{
                                            "ref_audio_path": ref_audio,
                                            "sec_audio_path": first_audio_under(&work.join("sec")),
                                            "ter_audio_path": first_audio_under(&work.join("ter"))
                                        },
                                        "params":{
                                            "chunks": s.chunks, "chunk_dur_s": s.chunk_dur_s, "sample_rate": &s.sample_rate,
                                            "stereo_mode": &s.stereo_mode, "method": &s.method, "band": &s.band
                                        },
                                        "runs":{}, "final":{}
                                    });

                                    let sr = match s.sample_rate.as_str() {
                                        "s12000" => 12_000u32,
                                        "s48000" => 48_000u32,
                                        _ => 24_000u32,
                                    };
                                    let stereo = match s.stereo_mode.as_str() {
                                        "mono" => StereoMode::Mono, "left" => StereoMode::Left, "right" => StereoMode::Right,
                                        "mid" => StereoMode::Mid, _ => StereoMode::Best
                                    };
                                    let method = match s.method.as_str() {
                                        "compat" => Method::Compat, _ => Method::Fft
                                    };
                                    let band = match s.band.as_str() {
                                        "voice" => Band::Voice, _ => Band::None
                                    };
                                    let params = XCorrParams{
                                        chunks: s.chunks, chunk_dur_s: s.chunk_dur_s, sample_rate: sr,
                                        min_match: 0.8, stereo_mode: stereo, method, band
                                    };

                                    // Duration: use ffprobe
                                    let dur = {
                                        let out = std::process::Command::new("ffprobe")
                                            .args(["-v","error","-show_entries","format=duration","-of","default=noprint_wrappers=1:nokey=1"])
                                            .arg(&s.ref_path)
                                            .output()
                                            .context("ffprobe")?;
                                        let txt = String::from_utf8_lossy(&out.stdout);
                                        txt.trim().parse::<f64>().unwrap_or(600.0)
                                    };

                                    // SEC
                                    if let Some(sec_audio) = results["meta"]["sec_audio_path"].as_str().map(|s| s.to_string()) {
                                        let (ns, ms, chunks) = do_analyze(&results["meta"]["ref_audio_path"].as_str().unwrap(), &sec_audio, dur, &params)?;
                                        let summary = json!({"median_delay_ns": ns, "median_delay_ms": ms, "peak_max": chunks.iter().map(|c| c["peak"].as_f64().unwrap_or(0.0)).fold(0.0, f64::max)});
                                        results["runs"]["sec"] = json!({"language": null, "language_matched": null, "chunks": chunks, "summary": summary});
                                    }
                                    // TER
                                    if let Some(ter_audio) = results["meta"]["ter_audio_path"].as_str().map(|s| s.to_string()) {
                                        let (ns, ms, chunks) = do_analyze(&results["meta"]["ref_audio_path"].as_str().unwrap(), &ter_audio, dur, &params)?;
                                        let summary = json!({"median_delay_ns": ns, "median_delay_ms": ms, "peak_max": chunks.iter().map(|c| c["peak"].as_f64().unwrap_or(0.0)).fold(0.0, f64::max)});
                                        results["runs"]["ter"] = json!({"language": null, "language_matched": null, "chunks": chunks, "summary": summary});
                                    }

                                    // Final block (compute global_shift_ms)
                                    let mut delays = vec![];
                                    if let Some(ms) = results["runs"]["sec"]["summary"]["median_delay_ms"].as_i64() { delays.push(ms); }
                                    if let Some(ms) = results["runs"]["ter"]["summary"]["median_delay_ms"].as_i64() { delays.push(ms); }
                                    let min_ms = delays.iter().cloned().min().unwrap_or(0);
                                    let mut pos = serde_json::Map::new();
                                    if let Some(ms) = results["runs"]["sec"]["summary"]["median_delay_ms"].as_i64() { pos.insert("sec".into(), serde_json::Value::from(ms - min_ms)); }
                                    if let Some(ms) = results["runs"]["ter"]["summary"]["median_delay_ms"].as_i64() { pos.insert("ter".into(), serde_json::Value::from(ms - min_ms)); }
                                    results["final"] = json!({
                                        "delays_ms_signed": {
                                            "sec": results["runs"]["sec"]["summary"]["median_delay_ms"],
                                            "ter": results["runs"]["ter"]["summary"]["median_delay_ms"],
                                        },
                                        "global_shift_ms": -min_ms,
                                        "delays_ms_positive": serde_json::Value::Object(pos),
                                    });

                                    // Write analysis.json
                                    let out_path = work.join("manifest").join("analysis.json");
                                    fs::write(&out_path, serde_json::to_string_pretty(&results)?)?;

                                    Ok(format!("Analysis done → {}", out_path.display()))
                                };
                                let msg = match run() {
                                    Ok(ok) => ok,
                                    Err(e) => format!("ERROR: {e:?}"),
                                };
                                // ping window to redraw by emitting a resize (hacky but fine)
                                let _ = win.request_redraw();
                                println!("{msg}");
                            });
                        }
                    });

                ui.window("Log")
                    .size([1060.0, 320.0], Condition::FirstUseEver)
                    .build(|| {
                        for line in &log_lines { ui.text_wrapped(line); }
                    });

                platform.prepare_render(&ui, &window);
                unsafe { gl.clear_color(0.1,0.1,0.1,1.0); gl.clear(glow::COLOR_BUFFER_BIT); }
                renderer.render(ui.render()).unwrap();
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                // Save settings on exit
                let _ = std::fs::write(exe_dir().join("vsg_settings.json"), serde_json::to_string_pretty(&settings).unwrap());
                elwt.exit();
            }
            _ => {}
        }
    })?;

    // never reached
    #[allow(unreachable_code)]
    Ok(())
}
