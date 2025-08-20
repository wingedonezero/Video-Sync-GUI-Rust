//! Minimal, **known-compatible** ImGui + winit(0.29) + glutin(0.30) bootstrap
//! Goal: get a window up reliably. Wire vsg-core analysis after this runs.

use std::time::Instant;
use std::ffi::{CString};

use anyhow::Result;
use imgui::{Context as ImContext, Io, Window, ChildWindow};
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use imgui_glow_renderer as imgui_gl;
use glow::HasContext as _;

use log::info;

use winit::dpi::LogicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;

use glutin::config::{ConfigTemplateBuilder, GlConfig};
use glutin::prelude::*;
use glutin::display::{Display, DisplayApiPreference};
use glutin::surface::{Surface, SurfaceAttributesBuilder, WindowSurface};
use glutin_winit::DisplayBuilder;

// raw-window-handle 0.5 (matches winit 0.29 and glutin 0.30)
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};

fn main() -> Result<()> {
    // ----------------- winit event loop + window -----------------
    let event_loop = EventLoop::new()?;

    let window_builder = WindowBuilder::new()
        .with_title("VSG – Analysis (alpha, stable stack)")
        .with_inner_size(LogicalSize::new(960.0, 600.0));

    // Minimal template; MSAA etc can be tuned later
    let template = ConfigTemplateBuilder::new();

    let (maybe_window, gl_config) = DisplayBuilder::new()
        .with_window_builder(Some(window_builder))
        .build(&event_loop, template, |mut confs| {
            confs.next().expect("no GL configs")
        })?;

    let window = maybe_window.expect("failed to create window");

    // ----------------- GL display/context/surface -----------------
    // glutin 0.30 uses raw-window-handle 0.5 (matching winit 0.29)
    let raw_display = unsafe {
        Display::new(
            window.raw_display_handle(),
            DisplayApiPreference::EglThenGlx(Box::new(|_| true)),
        )?
    };

    let ctx_attrs = glutin::context::ContextAttributesBuilder::new().build(Some(window.raw_window_handle()));
    let not_current = unsafe { raw_display.create_context(&gl_config, &ctx_attrs)? };

    let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        window.raw_window_handle(),
        // width/height will be set by resize events; 1x1 is fine here
        (1, 1).into(),
    );
    let surface = unsafe { raw_display.create_window_surface(&gl_config, &attrs)? };
    let context = not_current.make_current(&surface)?;

    // ----------------- glow + imgui setup -----------------
    let gl_display = gl_config.display();
    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            let c = CString::new(s).unwrap();
            gl_display.get_proc_address(&c)
        })
    };

    let mut imgui = ImContext::create();
    let mut platform = WinitPlatform::init(&mut imgui);
    platform.attach_window(imgui.io_mut(), &window, HiDpiMode::Default);

    let mut renderer = imgui_gl::AutoRenderer::initialize(gl, &mut imgui)
        .expect("failed to create imgui glow renderer");

    // style
    imgui.style_mut().use_dark_colors();

    let mut state = UiState::default();
    let mut last_frame = Instant::now();

    // request first frame
    window.request_redraw();

    // ----------------- event loop -----------------
    event_loop.run(move |event, _elwt| {
        match event {
            Event::NewEvents(_) => {
                // frame delta for imgui
                let now = Instant::now();
                imgui.io_mut().update_delta_time(now - last_frame);
                last_frame = now;
            }
            Event::WindowEvent { event, window_id } if window_id == window.id() => {
                // feed imgui first
                platform.handle_event(imgui.io_mut(), &window, &event);

                match event {
                    WindowEvent::CloseRequested => {
                        // 0.29 still has Exit
                        * _elwt.exit() = true;
                    }
                    WindowEvent::Resized(size) => {
                        surface.resize(
                            &context,
                            size.width.try_into().unwrap_or(1),
                            size.height.try_into().unwrap_or(1),
                        );
                        window.request_redraw();
                    }
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        surface.resize(
                            &context,
                            new_inner_size.width.try_into().unwrap_or(1),
                            new_inner_size.height.try_into().unwrap_or(1),
                        );
                        window.request_redraw();
                    }
                    _ => {}
                }
            }
            Event::RedrawRequested(_) => {
                let ui = platform.frame(imgui.io_mut(), &window).expect("imgui frame");

                draw_ui(&ui, &mut state);

                // Render
                platform.prepare_render(&ui, &window);
                let draw_data = imgui.render();

                // GL clear + draw
                unsafe {
                    let (w, h) = surface.size();
                    renderer.gl_context().viewport(0, 0, w as i32, h as i32);
                    renderer.gl_context().clear_color(0.1, 0.12, 0.15, 1.0);
                    renderer.gl_context().clear(glow::COLOR_BUFFER_BIT);
                }
                renderer.render(draw_data).expect("render failed");
                surface.swap_buffers(&context).expect("swap");
            }
            Event::MainEventsCleared => {
                window.request_redraw();
            }
            _ => {}
        }
    })?;

    // Unreachable (run() is !Return in 0.29), but keep signature happy on newer platforms.
    #[allow(unreachable_code)]
    Ok(())
}

#[derive(Default)]
struct UiState {
    ref_path: String,
    sec_path: String,
    ter_path: String,
    work_dir: String,
    out_dir: String,
    log: String,
    running: bool,
}

impl UiState {
    fn logln<S: Into<String>>(&mut self, s: S) {
        self.log.push_str(&s.into());
        self.log.push('\n');
    }
}

fn draw_ui(ui: &imgui::Ui, s: &mut UiState) {
    Window::new(ui, "Analysis (Audio XCorr)")
        .always_auto_resize(true)
        .build(|| {
            ui.input_text("REF (MKV)", &mut s.ref_path).build();
            ui.input_text("SEC (MKV)", &mut s.sec_path).build();
            ui.input_text("TER (MKV)", &mut s.ter_path).build();
            ui.separator();
            ui.input_text("Work dir", &mut s.work_dir).build();
            ui.input_text("Output dir", &mut s.out_dir).build();
            ui.separator();
            if ui.button("Analyze only") && !s.running {
                s.running = true;
                s.logln("✅ GUI is running on stable stack. Wire analysis next.");
                s.running = false;
            }
            ui.separator();
            ui.text("Log:");
            ChildWindow::new(ui, "log").size([0.0, 220.0]).build(|| {
                ui.text_wrapped(&s.log);
            });
        });
}
