//! Minimal imgui + winit 0.29 + glutin 0.30 bootstrap GUI wired to vsg-core.
//! This uses the older, stable APIs to avoid the mixed crate versions you hit.
//! It opens a window, shows three file pickers (REF/SEC/TER), analysis options, and a Run button.
//! For now, it shells out to the same extraction/analysis helpers in vsg-core that the CLI uses.

use std::time::Instant;
use std::path::{Path, PathBuf};

use anyhow::{Result, Context};
use imgui::*;
use imgui_glow_renderer::AutoRenderer;
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use raw_window_handle::HasRawWindowHandle;
use winit::dpi::LogicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;

use glutin::prelude::*;
use glutin::config::ConfigTemplateBuilder;
use glutin::display::{Display, DisplayApiPreference};
use glutin::context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext};
use glutin::surface::{Surface, SurfaceAttributesBuilder, WindowSurface};
use glutin_winit::DisplayBuilder;

use vsg_core::analyze::audio_xcorr::{analyze_pair, XCorrParams, Band};
use vsg_core::extract::run::{run_mkvextract};
use vsg_core::fsutil::{ensure_dir, default_work_dir, default_output_dir};
use vsg_core::model::{SelectionEntry, SelectionManifest, Source};

// --- Small helpers mirrored from CLI ---
fn first_audio_under(dir:&Path) -> Option<String> {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_file() {
                if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
                    if ["aac","ac3","eac3","dts","truehd","flac","opus","vorbis","m4a","mp3","wav","bin"].contains(&ext) {
                        return Some(p.to_string_lossy().into_owned());
                    }
                }
            }
        }
    }
    None
}

fn pick_side(file_opt:Option<&str>, source:Source) -> Result<Vec<SelectionEntry>> {
    let mut out = Vec::new();
    if let Some(file) = file_opt {
        // simple default: pick audio track 0
        out.push(SelectionEntry {
            file_path: file.into(),
            track_id: 0,
            r#type: "audio".into(),
            language: None,
            codec: None,
            // new fields
            container_index: Some(0),
            name: None,
            source,
        });
    }
    Ok(out)
}

fn build_manifest(ref_path:&str, sec_path:Option<&str>, ter_path:Option<&str>) -> Result<SelectionManifest> {
    Ok(SelectionManifest {
        ref_entries: pick_side(Some(ref_path), Source::REF)?,
        sec_entries: pick_side(sec_path, Source::SEC)?,
        ter_entries: pick_side(ter_path, Source::TER)?,
    })
}

fn extract_if_needed(sel:&SelectionManifest, work:&Path) -> Result<()> {
    ensure_dir(work)?;
    let have_any = first_audio_under(&work.join("ref")).is_some()
        && (sel.sec_entries.is_empty() || first_audio_under(&work.join("sec")).is_some())
        && (sel.ter_entries.is_empty() || first_audio_under(&work.join("ter")).is_some());
    if have_any {
        Ok(())
    } else {
        run_mkvextract(sel, &work.to_path_buf()).context("mkvextract")?;
        Ok(())
    }
}

fn do_analyze(ref_audio:&str, other_audio:&str, ns_err:i64, params:&XCorrParams) -> Result<(i64, f64)> {
    let (nanosec, mse, _chunks) = analyze_pair(ref_audio, other_audio, ns_err, params)?;
    Ok((nanosec, mse))
}

// --- GUI state ---
#[derive(Default)]
struct UiState {
    ref_path: String,
    sec_path: String,
    ter_path: String,
    work_dir: String,
    out_dir: String,
    chunks: u32,
    chunk_dur_s: f32,
    sample_rate_sel: usize, // 0:12k,1:24k,2:48k
    band_sel: usize,        // 0:voice 1:full
    err_min_ns: i64,
    err_max_ns: i64,
    log: String,
    run_requested: bool,
}

fn current_sample_rate(idx:usize) -> u32 {
    match idx {
        0 => 12000, 1 => 24000, _ => 48000
    }
}

fn current_band(idx:usize) -> Band {
    if idx == 0 { Band::Voice } else { Band::Full }
}

fn push_log(log:&mut String, line:&str) {
    use std::fmt::Write;
    let _ = writeln!(log, "{}", line);
}

fn main() -> Result<()> {
    // ---- Window & GL (winit 0.29 + glutin 0.30 path) ----
    let event_loop = EventLoop::new();
    let window_builder = WindowBuilder::new()
        .with_title("Video Sync GUI (prototype)")
        .with_inner_size(LogicalSize::new(1100.0, 720.0));

    let template = ConfigTemplateBuilder::new();
    let display_builder = DisplayBuilder::new().with_window_builder(Some(window_builder));

    let (window, config) = display_builder
        .build(&event_loop, template, |mut configs| configs.next().expect("no GL configs"))
        .context("create window + choose GL config")?;
    let window = window.expect("failed to create winit window");

    let raw_display = unsafe {
        Display::new(
            window.raw_display_handle(),
            DisplayApiPreference::EglThenGlx(Box::new(|_cb| {})), // X11: allow GLX fallback
        ).context("create glutin Display")?
    };

    let context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::OpenGl(Some(glutin::context::Version::new(3, 3))))
        .build(Some(window.raw_window_handle()));

    let not_current = unsafe {
        raw_display.create_context(&config, &context_attributes)
    }.context("create GL context")?;

    let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        window.raw_window_handle(),
        config.compatible_surface_transform(),
        None,
    );

    let surface = unsafe { raw_display.create_window_surface(&config, &attrs) }
        .context("create window surface")?;

    let context: PossiblyCurrentContext = not_current.make_current(&surface)
        .context("make GL context current")?;

    let gl = unsafe { glow::Context::from_loader_function(|s| raw_display.get_proc_address(s) as *const _) };

    // ---- ImGui wiring ----
    let mut imgui = imgui::Context::create();
    let mut platform = WinitPlatform::with_hidpi_mode(&mut imgui, HiDpiMode::Default);
    platform.attach_window(imgui.io_mut(), &window, imgui_winit_support::HiDpiMode::Default);

    imgui.set_ini_filename(None);
    let mut renderer = AutoRenderer::initialize(&mut imgui, &gl).context("imgui renderer")?;

    // ---- App state ----
    let mut state = UiState {
        work_dir: default_work_dir().to_string_lossy().into_owned(),
        out_dir: default_output_dir().to_string_lossy().into_owned(),
        chunks: 10,
        chunk_dur_s: 6.0,
        sample_rate_sel: 2,
        band_sel: 0,
        err_min_ns: -1_000_000_000,
        err_max_ns:  1_000_000_000,
        ..Default::default()
    };

    let mut last_frame = Instant::now();

    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::NewEvents(_) => {
                let now = Instant::now();
                imgui.io_mut().update_delta_time(now - last_frame);
                last_frame = now;
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                *control_flow = winit::event_loop::ControlFlow::Exit;
            }
            Event::WindowEvent { event: WindowEvent::Resized(size), .. } => {
                surface.resize(
                    &context,
                    size.width.try_into().unwrap_or(1),
                    size.height.try_into().unwrap_or(1),
                );
            }
            Event::AboutToWait => {
                platform.prepare_frame(imgui.io_mut(), &window).unwrap();
                let ui = imgui.frame();

                ui.window("Inputs & Options")
                    .size([520.0, 360.0], Condition::FirstUseEver)
                    .build(|| {
                        ui.input_text("REF mkv", &mut state.ref_path).build();
                        ui.input_text("SEC mkv", &mut state.sec_path).build();
                        ui.input_text("TER mkv", &mut state.ter_path).build();
                        ui.separator();
                        ui.input_text("Work dir", &mut state.work_dir).build();
                        ui.input_text("Output dir", &mut state.out_dir).build();
                        ui.separator();
                        ui.text("Analysis");
                        ui.input_scalar("Chunks", &mut state.chunks).build();
                        ui.input_float("Chunk duration (s)", &mut state.chunk_dur_s).build();

                        ComboBox::new("Sample rate").build_simple_string(
                            &ui,
                            &mut state.sample_rate_sel,
                            &["12 kHz", "24 kHz", "48 kHz"],
                        );
                        ComboBox::new("Band").build_simple_string(
                            &ui,
                            &mut state.band_sel,
                            &["Voice", "Full"],
                        );
                        ui.input_scalar("Err min (ns)", &mut state.err_min_ns).build();
                        ui.input_scalar("Err max (ns)", &mut state.err_max_ns).build();

                        if ui.button("Run extract + analyze") {
                            state.run_requested = true;
                        }
                    });

                ui.window("Live Log")
                    .size([520.0, 300.0], Condition::FirstUseEver)
                    .position([540.0, 20.0], Condition::FirstUseEver)
                    .build(|| {
                        ui.text_wrapped(&state.log);
                    });

                if state.run_requested {
                    state.run_requested = false;
                    // fire-and-forget on the UI thread for now (short operations); long jobs should be threaded later
                    let mut run = || -> Result<()> {
                        let ref_path = state.ref_path.trim();
                        if ref_path.is_empty() { anyhow::bail!("REF path is empty"); }
                        let work = PathBuf::from(&state.work_dir);
                        push_log(&mut state.log, &format!("Work dir: {}", work.display()));

                        let sel = build_manifest(
                            ref_path,
                            (!state.sec_path.trim().is_empty()).then(|| state.sec_path.trim()),
                            (!state.ter_path.trim().is_empty()).then(|| state.ter_path.trim()),
                        )?;
                        push_log(&mut state.log, "Built selection manifest");

                        extract_if_needed(&sel, &work)?;
                        push_log(&mut state.log, "Extraction complete (or skipped)");

                        let ref_audio = first_audio_under(&work.join("ref")).context("no REF audio extracted")?;
                        if !state.sec_path.is_empty() {
                            let sec_audio = first_audio_under(&work.join("sec")).context("no SEC audio extracted")?;
                            let params = XCorrParams{
                                chunks: state.chunks as usize,
                                chunk_dur_s: state.chunk_dur_s as f64,
                                sample_rate: current_sample_rate(state.sample_rate_sel),
                                min_match: 0.12,
                                duration_s: None,
                                videodiff: false,
                                band: current_band(state.band_sel),
                            };
                            let (ns, mse) = do_analyze(&ref_audio, &sec_audio, state.err_max_ns.abs(), &params)?;
                            push_log(&mut state.log, &format!("SEC offset = {} ns, MSE = {:.6}", ns, mse));
                        }
                        if !state.ter_path.is_empty() {
                            let ter_audio = first_audio_under(&work.join("ter")).context("no TER audio extracted")?;
                            let params = XCorrParams{
                                chunks: state.chunks as usize,
                                chunk_dur_s: state.chunk_dur_s as f64,
                                sample_rate: current_sample_rate(state.sample_rate_sel),
                                min_match: 0.12,
                                duration_s: None,
                                videodiff: false,
                                band: current_band(state.band_sel),
                            };
                            let (ns, mse) = do_analyze(&ref_audio, &ter_audio, state.err_max_ns.abs(), &params)?;
                            push_log(&mut state.log, &format!("TER offset = {} ns, MSE = {:.6}", ns, mse));
                        }
                        Ok(())
                    };
                    if let Err(e) = run() {
                        push_log(&mut state.log, &format!("ERROR: {:#}", e));
                    }
                }

                platform.prepare_render(&ui, &window);
                let draw_data = ui.render();
                unsafe {
                    use glow::HasContext as _;
                    gl.clear_color(0.1, 0.1, 0.12, 1.0);
                    gl.clear(glow::COLOR_BUFFER_BIT);
                }
                renderer.render(draw_data).unwrap();
                surface.swap_buffers(&context).unwrap();
            }
            _ => {}
        }
    });
}