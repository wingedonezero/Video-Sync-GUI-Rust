
mod config;
mod logger;

use anyhow::Result;
use config::{GuiConfig, load as load_cfg, save as save_cfg};
use imgui::*;
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use logger::Logger;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;
use vsg_core::analyze::audio_xcorr::{analyze_audio_xcorr_detailed, Band, Method, StereoMode, XCorrParams};

fn default_dirs() -> (PathBuf, PathBuf) {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    let base = exe.parent().unwrap_or(&PathBuf::from(".")).to_path_buf();
    (base.join("tmp_work"), base.join("out"))
}

fn ffprobe_duration_s(p: &str) -> Result<f64> {
    let out = Command::new("ffprobe")
        .args(["-v","error","-show_entries","format=duration","-of","csv=p=0", p])
        .output()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let v: f64 = s.parse().unwrap_or(0.0);
    Ok(v)
}

fn run_analysis(cfg: &GuiConfig, log: &mut Logger) -> Result<()> {
    let (def_work, def_out) = default_dirs();
    let work_dir = if cfg.work_dir.trim().is_empty() { def_work } else { PathBuf::from(&cfg.work_dir) };
    let _out_dir = if cfg.out_dir.trim().is_empty() { def_out } else { PathBuf::from(&cfg.out_dir) };
    std::fs::create_dir_all(&work_dir)?;
    std::fs::create_dir_all(work_dir.join("manifest"))?;

    // resolve sample rate
    let sr = match cfg.sample_rate.as_str() {
        "s12000" => 12000,
        "s48000" => 48000,
        _ => 24000,
    };
    let stereo = match cfg.stereo_mode.as_str() {
        "mono" => StereoMode::Mono,
        "left" => StereoMode::Left,
        "right" => StereoMode::Right,
        "mid" => StereoMode::Mid,
        _ => StereoMode::Best,
    };
    let method = match cfg.method.as_str() { "compat" => Method::Compat, _ => Method::Fft };
    let band = match cfg.band.as_str() { "voice" => Band::Voice, _ => Band::None };

    // determine duration from REF
    log.log("Probing duration via ffprobe...");
    let dur = ffprobe_duration_s(&cfg.ref_path)?;
    log.log(&format!("Duration: {:.3}s", dur));

    let params = XCorrParams {
        chunks: cfg.chunks,
        chunk_dur_s: cfg.chunk_dur,
        sample_rate: sr,
        min_match: 0.8,
        stereo_mode: stereo,
        method,
        band,
    };

    // Run SEC / TER if provided
    let mut json = serde_json::json!({
        "meta": {
            "ref_audio_path": cfg.ref_path,
            "sec_audio_path": if cfg.sec_path.is_empty(){ serde_json::Value::Null } else { serde_json::Value::String(cfg.sec_path.clone()) },
            "ter_audio_path": if cfg.ter_path.is_empty(){ serde_json::Value::Null } else { serde_json::Value::String(cfg.ter_path.clone()) },
            "ref_language": serde_json::Value::Null, // unknown at GUI level for now
            "sec_language": serde_json::Value::Null,
            "ter_language": serde_json::Value::Null
        },
        "params": {
            "chunks": cfg.chunks, "chunk_dur_s": cfg.chunk_dur, "sample_rate": sr,
            "stereo_mode": cfg.stereo_mode, "method": cfg.method, "band": cfg.band
        },
        "runs": {},
        "final": {}
    });

    let mut delays_ms = serde_json::Map::new();
    let mut delays_ns = serde_json::Map::new();
    let mut peaks = serde_json::Map::new();

    let mut do_one = |label: &str, other: &str| -> Result<serde_json::Value> {
        log.log(&format!("Analyzing {} vs REF...", label.to_uppercase()));
        let (res, chunks) = analyze_audio_xcorr_detailed(&cfg.ref_path, other, dur, &params)?;
        let chunks_j: Vec<_> = chunks.iter().map(|c| serde_json::json!({
            "center_s": c.center_s, "window_samples": c.window_samples,
            "lag_ns": c.lag_ns, "lag_ms": c.lag_ms, "peak": c.peak
        })).collect();
        Ok(serde_json::json!({
            "language": serde_json::Value::Null,
            "language_matched": serde_json::Value::Null,
            "chunks": chunks_j,
            "summary": {
                "median_delay_ns": res.delay_ns,
                "median_delay_ms": res.delay_ms,
                "peak_max": res.peak_score
            }
        }))
    };

    if !cfg.sec_path.trim().is_empty() {
        let entry = do_one("sec", &cfg.sec_path)?;
        delays_ms.insert("sec".into(), entry["summary"]["median_delay_ms"].clone());
        delays_ns.insert("sec".into(), entry["summary"]["median_delay_ns"].clone());
        peaks.insert("sec".into(), entry["summary"]["peak_max"].clone());
        json["runs"]["sec"] = entry;
    }
    if !cfg.ter_path.trim().is_empty() {
        let entry = do_one("ter", &cfg.ter_path)?;
        delays_ms.insert("ter".into(), entry["summary"]["median_delay_ms"].clone());
        delays_ns.insert("ter".into(), entry["summary"]["median_delay_ns"].clone());
        peaks.insert("ter".into(), entry["summary"]["peak_max"].clone());
        json["runs"]["ter"] = entry;
    }

    // global shift to make non-negative ms
    let mut present = vec![0i64];
    if let Some(v) = delays_ms.get("sec").and_then(|x| x.as_i64()) { present.push(v); }
    if let Some(v) = delays_ms.get("ter").and_then(|x| x.as_i64()) { present.push(v); }
    let minv = *present.iter().min().unwrap_or(&0);
    let global_shift_ms = if minv < 0 { -minv } else { 0 };

    let mut delays_ms_positive = serde_json::Map::new();
    if let Some(v) = delays_ms.get("sec").and_then(|x| x.as_i64()) { delays_ms_positive.insert("sec".into(), serde_json::json!(v + global_shift_ms)); }
    if let Some(v) = delays_ms.get("ter").and_then(|x| x.as_i64()) { delays_ms_positive.insert("ter".into(), serde_json::json!(v + global_shift_ms)); }

    json["final"] = serde_json::json!({
        "delays_ms_signed": delays_ms,
        "delays_ns_signed": delays_ns,
        "global_shift_ms": global_shift_ms,
        "delays_ms_positive": delays_ms_positive,
        "peaks": peaks,
        "notes": [
            "GUI Analyze-only mode output; languages unknown at GUI level",
            "Use CLI analyze --from-manifest for language-aware selection"
        ]
    });

    let outp = work_dir.join("manifest").join("analysis.json");
    std::fs::write(&outp, serde_json::to_string_pretty(&json).unwrap())?;
    log.log(&format!("Wrote {}", outp.display()));
    Ok(())
}

fn main() -> Result<()> {
    // Create event loop and context
    let event_loop = winit::event_loop::EventLoop::new().unwrap();
    let window = winit::window::WindowBuilder::new()
        .with_title("VSG GUI (Analyze mode)")
        .with_inner_size(winit::dpi::LogicalSize::new(1024.0, 720.0))
        .build(&event_loop)
        .unwrap();

    let mut imgui = imgui::Context::create();
    imgui.set_ini_filename(None);
    let mut platform = WinitPlatform::new(&mut imgui);
    platform.attach_window(imgui.io_mut(), &window, HiDpiMode::Default);

    // GL context (glow)
    use glow::HasContext as _;
    let (gl, shader_version, surface, gl_context) = {
        let wb = window;
        let display_builder = glutin::display::Display::new(
            glutin::platform::windows::DisplayApiPreference::EglThenWgl(Some(wb.raw_window_handle())),
            glutin::display::DisplayApi::Egl,
        );
        // The above is complex; fallback to glow headless with Winit is non-trivial across platforms.
        // Simpler route: create a glow context via glutin::ContextBuilder (not available in this minimal example).
        // To keep cross-platform issues down, we use a dummy context-less renderer-less loop
        // and render nothing custom; ImGui draws via software in this stripped skeleton.
        // NOTE: If this fails to build on your platform, we can swap to eframe/egui quickly.
        let gl = glow::Context::from_loader_function(|_| std::ptr::null());
        (gl, "#version 130".to_string(), (), ())
    };

    let mut cfg = load_cfg();
    let mut logger = Logger::new();
    let mut running = false;

    event_loop.run(move |event, target| {
        match event {
            winit::event::Event::WindowEvent { event, .. } => {
                match event {
                    winit::event::WindowEvent::CloseRequested => target.exit(),
                    _ => {}
                }
            }
            winit::event::Event::AboutToWait => {
                platform.prepare_frame(imgui.io_mut(), &window).unwrap();
                window.request_redraw();
            }
            winit::event::Event::RedrawRequested(_) => {
                let ui = imgui.frame();

                ui.window("Inputs")
                    .size([480.0, 220.0], Condition::FirstUseEver)
                    .build(|| {
                        ui.input_text("REF", &mut cfg.ref_path).build();
                        ui.input_text("SEC", &mut cfg.sec_path).build();
                        ui.input_text("TER", &mut cfg.ter_path).build();
                        ui.separator();
                        ui.input_text("Work dir (blank = ./tmp_work)", &mut cfg.work_dir).build();
                        ui.input_text("Out dir (blank = ./out)", &mut cfg.out_dir).build();
                        if ui.button("Save Settings") { let _ = save_cfg(&cfg); }
                    });

                ui.window("Options")
                    .size([480.0, 260.0], Condition::FirstUseEver)
                    .build(|| {
                        ui.text("Correlation");
                        ui.slider_config("Chunks", 1, 30).display_format("%d").build(&mut cfg.chunks);
                        ui.slider_config("Chunk dur (s)", 1.0, 30.0).display_format("%.1f").build(&mut cfg.chunk_dur);
                        ui.separator();
                        ui.text("Audio Decode");
                        ui.combo_simple_string("Sample Rate", &mut cfg.sample_rate, &["s12000","s24000","s48000"]);
                        ui.combo_simple_string("Stereo", &mut cfg.stereo_mode, &["mono","left","right","mid","best"]);
                        ui.combo_simple_string("Method", &mut cfg.method, &["fft","compat"]);
                        ui.combo_simple_string("Band", &mut cfg.band, &["none","voice"]);
                        if ui.button("Analyze") && !running {
                            running = true;
                            let mut cfg_clone = cfg.clone();
                            // spawn worker
                            let (tx, rx) = std::sync::mpsc::channel::<String>();
                            thread::spawn(move || {
                                let mut lg = Logger::new();
                                lg.log("Starting analysis...");
                                let res = run_analysis(&cfg_clone, &mut lg);
                                if let Err(e) = res { let _ = tx.send(format!("ERROR: {}", e)); }
                                else { let _ = tx.send("DONE".into()); }
                            });
                            // simple polling
                            thread::spawn(move || {
                                // This second thread is a placeholder to join the worker if needed.
                            });
                        }
                    });

                ui.window("Live Log")
                    .size([520.0, 480.0], Condition::FirstUseEver)
                    .build(|| {
                        ui.text_wrapped(logger.contents());
                    });

                // end frame (no GL rendering in this skeleton)

            }
            _ => {}
        }
    }).unwrap();

    // unreachable
    //Ok(())
}
