//! Minimal IMGUI shell on winit 0.30 + glutin 0.32 + glow.
//! - Uses ApplicationHandler / run_app (winit 0.30).
//! - Creates GL context via glutin 0.32 (EGL/GLX chosen by platform).
//! - Integrates imgui-winit-support 0.13 + imgui-glow-renderer 0.13.
//! - Wires "Analysis mode" button to call into vsg-core paths (stubs call sites, real work stays in core).

use anyhow::{Context, Result};
use imgui::{Condition, Ui};
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use log::*;
use std::path::{Path, PathBuf};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes};

use glutin::config::ConfigTemplateBuilder;
use glutin::display::{Display, DisplayApiPreference};
use glutin::prelude::*;
use glutin::surface::{Surface, SurfaceAttributesBuilder, WindowSurface};

use glow::HasContext as _;

use vsg_core::model::{SelectionEntry, SelectionManifest, Source};
use vsg_core::extract::run::run_mkvextract; // returns ExtractSummary
use vsg_core::fsutil::{default_output_dir, default_work_dir};
use vsg_core::analyze::audio_xcorr::{analyze_pair, XCorrParams}; // assume exists

// ---------------- GUI State ----------------

#[derive(Default)]
struct GuiState {
    window: Option<Window>,
    gl_display: Option<Display>,
    gl_surface: Option<Surface<WindowSurface>>,
    gl_context: Option<glutin::context::PossiblyCurrentContext>,
    gl: Option<glow::Context>,

    imgui: Option<imgui::Context>,
    platform: Option<WinitPlatform>,
    renderer: Option<imgui_glow_renderer::AutoRenderer>,

    // Inputs
    ref_path: String,
    sec_path: String,
    ter_path: String,
    work_dir: String,
    out_dir: String,

    // Analysis params (defaults)
    chunks: u32,
    chunk_ms: u32,
    sample_rate: String, // "s48000" etc
    min_match: f32,
    use_videodiff: bool,

    // Log text (live)
    log_lines: Vec<String>,
}

impl GuiState {
    fn log(&mut self, s: impl Into<String>) {
        let line = s.into();
        info!("{line}");
        self.log_lines.push(line);
        if self.log_lines.len() > 2000 {
            self.log_lines.drain(..1000);
        }
    }
}

// ---------------- Utility (selection manifest builders) ----------------

fn make_entry(file: &str, id: u32, lang: Option<&str>, codec: Option<&str>, source: Source) -> SelectionEntry {
    SelectionEntry {
        file_path: file.into(),
        track_id: id,
        r#type: "audio".into(),
        language: lang.map(|s| s.to_string()),
        codec: codec.map(|s| s.to_string()),
        // keep alignment with core model
        container_index: Some(0),
        name: None,
        source,
    }
}

fn build_manifest(ref_file: &str, sec_file: Option<&str>, ter_file: Option<&str>) -> SelectionManifest {
    // We default to picking track id 0 for now — the CLI path already does probing & selection.
    // GUI will later call the same probe to populate real choices.
    let mut ref_entries = vec![make_entry(ref_file, 0, None, None, Source::REF)];
    let mut sec_entries = vec![];
    let mut ter_entries = vec![];
    if let Some(sf) = sec_file {
        sec_entries.push(make_entry(sf, 0, None, None, Source::SEC));
    }
    if let Some(tf) = ter_file {
        ter_entries.push(make_entry(tf, 0, None, None, Source::TER));
    }
    SelectionManifest { ref_entries, sec_entries, ter_entries }
}

fn to_path(s: &str) -> PathBuf { PathBuf::from(s) }

// ---------------- Application Handler ----------------

struct App {
    state: GuiState,
}

impl App {
    fn new() -> Self {
        let mut st = GuiState::default();
        st.work_dir = default_work_dir().to_string_lossy().to_string();
        st.out_dir = default_output_dir().to_string_lossy().to_string();
        st.chunks = 10;
        st.chunk_ms = 12000;
        st.sample_rate = "s48000".into();
        st.min_match = 0.4;
        st.use_videodiff = false;
        App { state: st }
    }

    fn init_gl(
        &mut self,
        el: &dyn ActiveEventLoop,
    ) -> Result<()> {
        // Create window
        let attrs = WindowAttributes::default()
        .with_title("Video Sync GUI (winit 0.30 + imgui)")
        .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0));
        let window = el.create_window(attrs).context("create window")?;

        // Choose config
        let template = ConfigTemplateBuilder::new();
        let display = unsafe {
            // Prefer EGL then GLX on Linux (glutin picks appropriate)
            Display::new(window.display_handle().as_raw(), DisplayApiPreference::EglThenGlx)?
        };

        let config = display
        .find_configs(template)
        .context("find GL configs")?
        .next()
        .context("no GL configs")?;

        // Build GL context and surface
        let window_handle = window.window_handle().as_raw();
        let surface_attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(window_handle, 0, 0);
        let surface = unsafe { display.create_window_surface(&config, &surface_attrs)? };

        let ctx_attrs = glutin::context::ContextAttributesBuilder::new().build(Some(window_handle));
        let not_current = unsafe { display.create_context(&config, &ctx_attrs)? };
        let context = not_current.make_current(&surface).context("make_current")?;

        // Load GL
        let gl = unsafe {
            glow::Context::from_loader_function(|s| display.get_proc_address(&std::ffi::CString::new(s).unwrap()) as *const _)
        };

        self.state.window = Some(window);
        self.state.gl_display = Some(display);
        self.state.gl_surface = Some(surface);
        self.state.gl_context = Some(context);
        self.state.gl = Some(gl);
        Ok(())
    }

    fn init_imgui(&mut self) -> Result<()> {
        let mut imgui = imgui::Context::create();
        imgui.set_ini_filename(None);
        let mut platform = WinitPlatform::new(&mut imgui);
        platform.set_high_dpi_mode(HiDpiMode::Default);

        let window = self.state.window.as_ref().unwrap();
        platform.attach_window(imgui.io_mut(), window, imgui_winit_support::HiDpiMode::Default);

        let gl = self.state.gl.as_ref().unwrap();
        let renderer = unsafe { imgui_glow_renderer::AutoRenderer::initialize(&mut imgui, gl)? };

        self.state.imgui = Some(imgui);
        self.state.platform = Some(platform);
        self.state.renderer = Some(renderer);
        Ok(())
    }

    fn draw_ui(&mut self, ui: &Ui) {
        ui.window("Analysis (extract+xcorr)")
        .size([560.0, 380.0], Condition::FirstUseEver)
        .build(|| {
            ui.input_text("REF path", &mut self.state.ref_path).build();
            ui.input_text("SEC path", &mut self.state.sec_path).build();
            ui.input_text("TER path", &mut self.state.ter_path).build();
            ui.input_text("Work dir", &mut self.state.work_dir).build();
            ui.input_text("Output dir", &mut self.state.out_dir).build();

            ui.separator();
            ui.text("Correlation params");
            ui.input_int("chunks (10 == full spread)", &mut (self.state.chunks as i32))
            .build();
            ui.input_int("chunk ms", &mut (self.state.chunk_ms as i32)).build();
            ui.input_text("sample rate (s48000/s24000/s12000)", &mut self.state.sample_rate).build();
            ui.slider_config("min match", 0.0, 1.0).build(&mut self.state.min_match);
            ui.checkbox("videodiff (optional second pass)", &mut self.state.use_videodiff);

            if ui.button("Run analysis") {
                self.state.log("Starting analysis…");
                if let Err(e) = self.run_analysis() {
                    self.state.log(format!("ERROR: {e:#}"));
                }
            }
        });

        ui.window("Live Log")
        .size([680.0, 380.0], Condition::FirstUseEver)
        .position([580.0, 20.0], Condition::FirstUseEver)
        .build(|| {
            for line in &self.state.log_lines {
                ui.text_wrapped(line);
            }
        });
    }

    fn run_analysis(&mut self) -> Result<()> {
        let work = to_path(&self.state.work_dir);
        let out = to_path(&self.state.out_dir);
        std::fs::create_dir_all(&work).ok();
        std::fs::create_dir_all(&out).ok();

        let have_ref = !self.state.ref_path.is_empty();
        if !have_ref {
            anyhow::bail!("REF path is empty");
        }
        let sel = build_manifest(
            &self.state.ref_path,
            (!self.state.sec_path.is_empty()).then(|| self.state.sec_path.as_str()),
                                 (!self.state.ter_path.is_empty()).then(|| self.state.ter_path.as_str()),
        );

        // Extract (respect our rule to use mkvextract)
        self.state.log("Extracting tracks via mkvextract…");
        let summary = run_mkvextract(&sel, &work).context("mkvextract")?;
        self.state.log(format!("Extracted: {:?}", summary.outputs));

        // Pick first audio under each dir (core helper already does this in CLI; we mirror simply here)
        let ref_audio = first_audio_under(&work.join("ref")).context("ref audio not found")?;
        let sec_audio = first_audio_under(&work.join("sec")).ok();
        let ter_audio = first_audio_under(&work.join("ter")).ok();

        // Correlation params
        let params = XCorrParams {
            chunks: self.state.chunks as usize,
            chunk_ms: self.state.chunk_ms as usize,
            sample_rate_flag: self.state.sample_rate.clone(),
            min_match: self.state.min_match,
        };

        // SEC pass
        if let Some(sec) = &sec_audio {
            self.state.log(format!("Analyzing REF vs SEC: {:?} vs {:?}", ref_audio, sec));
            let result = analyze_pair(&ref_audio, sec, &params)
            .context("xcorr REF vs SEC")?;
            self.state.log(format!("SEC result: global_offset_ns={} ns, matches={}", result.global_offset_ns, result.matches.len()));
            // TODO save JSON to binary dir like CLI
        } else {
            self.state.log("No SEC audio extracted; skipping.");
        }

        // TER pass
        if let Some(ter) = &ter_audio {
            self.state.log(format!("Analyzing REF vs TER: {:?} vs {:?}", ref_audio, ter));
            let result = analyze_pair(&ref_audio, ter, &params)
            .context("xcorr REF vs TER")?;
            self.state.log(format!("TER result: global_offset_ns={} ns, matches={}", result.global_offset_ns, result.matches.len()));
        } else {
            self.state.log("No TER audio extracted; skipping.");
        }

        Ok(())
    }
}

// Small helper: mirror CLI’s “first audio” pick
fn first_audio_under(dir: &Path) -> Option<PathBuf> {
    let mut candidates = std::fs::read_dir(dir).ok()?
    .filter_map(|e| e.ok())
    .map(|e| e.path())
    .filter(|p| {
        if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
            matches!(ext, "aac" | "ac3" | "eac3" | "dts" | "thd" | "flac" | "opus" | "ogg" | "mka" | "wav")
        } else { false }
    })
    .collect::<Vec<_>>();
    candidates.sort();
    candidates.into_iter().next()
}

// ---- ApplicationHandler impl

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.state.window.is_none() {
            if let Err(e) = self.init_gl(event_loop)
                .and_then(|_| self.init_imgui()) {
                    eprintln!("Failed to init GUI: {e:#}");
                    event_loop.exit();
                    return;
                }
        }
    }

    fn window_event(&mut self, _el: &dyn ActiveEventLoop, _id: winit::window::WindowId, event: WindowEvent) {
        if let (Some(window), Some(imgui), Some(platform), Some(renderer), Some(gl), Some(surface), Some(ctx)) =
            (self.state.window.as_ref(), self.state.imgui.as_mut(), self.state.platform.as_mut(),
             self.state.renderer.as_mut(), self.state.gl.as_ref(), self.state.gl_surface.as_ref(), self.state.gl_context.as_ref())
            {
                platform.handle_event(imgui.io_mut(), window, &event);

                match event {
                    WindowEvent::CloseRequested => {
                        _el.exit();
                    }
                    WindowEvent::RedrawRequested => {
                        // New frame
                        platform.prepare_frame(imgui.io_mut(), window).unwrap();
                        let ui = imgui.frame();

                        // UI
                        self.draw_ui(&ui);

                        // Render
                        unsafe {
                            gl.clear_color(0.10, 0.12, 0.15, 1.0);
                            gl.clear(glow::COLOR_BUFFER_BIT);
                        }
                        platform.prepare_render(&ui, window);
                        let draw_data = ui.render();
                        renderer.render(draw_data).unwrap();
                        surface.swap_buffers(ctx).ok();
                    }
                    _ => {}
                }
            }
    }

    fn device_event(&mut self, _el: &dyn ActiveEventLoop, _id: DeviceId, _event: DeviceEvent) {}
}

// ---------------- main ----------------

fn main() -> Result<()> {
    env_logger::init();
    let event_loop = EventLoop::new().context("event loop")?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::new();
    event_loop.run_app(&mut app).context("run_app")?;
    Ok(())
}
