\
use std::time::Instant;
use std::ffi::CString;

use anyhow::Result;
use log::*;

use imgui::{Ui, Condition, Window, ChildWindow};
use imgui_winit_support::{HiDpiMode, WinitPlatform};

use glow::HasContext as _;

use winit::dpi::LogicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;

use glutin::config::{ConfigTemplateBuilder, GlConfig};
use glutin::context::{ContextAttributesBuilder, NotCurrentContext, PossiblyCurrentContext};
use glutin::display::{GetGlDisplay};
use glutin::prelude::*;
use glutin::surface::{Surface, SurfaceAttributesBuilder, WindowSurface, GlSurface};
use glutin_winit::DisplayBuilder;

use raw_window_handle::{HasRawWindowHandle, HasRawDisplayHandle};

fn init_logger() {
    let _ = env_logger::builder().is_test(false).try_init();
}

struct GuiState {
    ref_path: String,
    sec_path: String,
    ter_path: String,
    work_dir: String,
    out_dir: String,
    log: String,
    running: bool,
}

impl GuiState {
    fn new() -> Self {
        Self {
            ref_path: String::new(),
            sec_path: String::new(),
            ter_path: String::new(),
            work_dir: String::new(),
            out_dir: String::new(),
            log: String::new(),
            running: false,
        }
    }
    fn logln(&mut self, s: impl AsRef<str>) {
        self.log.push_str(s.as_ref());
        self.log.push('\n');
    }
}

fn main() -> Result<()> {
    init_logger();
    info!("vsg-gui starting (stable stack: winit 0.29 / glutin 0.31 / imgui 0.12)");

    // ---- Winit event loop & window ----
    let event_loop: EventLoop<()> = EventLoop::new()?; // winit 0.29 returns Result

    let window_attrs = WindowBuilder::new()
        .with_title("VSG – Analysis (alpha, stable stack)")
        .with_inner_size(LogicalSize::new(1100.0, 700.0));

    // ---- Choose a GL config via glutin DisplayBuilder ----
    let template = ConfigTemplateBuilder::new().with_alpha_size(8).with_depth_size(24).with_stencil_size(8).build();
    let display_builder = DisplayBuilder::new().with_window_attributes(Some(window_attrs));

    let (maybe_window, gl_config) = display_builder.build(&event_loop, template, |mut configs| {
        configs.next().expect("No GL configs found")
    })?;

    let window = maybe_window.expect("Failed to create winit window");
    let gl_display = gl_config.display();

    // ---- Create GL context ----
    let raw_window = window.raw_window_handle();
    let _raw_display = window.raw_display_handle();

    let context_attributes = ContextAttributesBuilder::new().build(Some(raw_window));
    let not_current: NotCurrentContext = unsafe { gl_display.create_context(&gl_config, &context_attributes) }?;

    let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        window.raw_window_handle(),
        window.inner_size().width,
        window.inner_size().height,
    );
    let surface: Surface<WindowSurface> = unsafe { gl_display.create_window_surface(&gl_config, &attrs) }?;

    let mut gl_context: PossiblyCurrentContext = not_current.make_current(&surface)?;

    // ---- Glow (OpenGL function loader) ----
    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            let cs = CString::new(s).unwrap();
            gl_display.get_proc_address(&cs) as *const _
        })
    };

    // ---- ImGui ----
    let mut imgui = imgui::Context::create();
    imgui.set_ini_filename(None);
    let mut platform = WinitPlatform::init(&mut imgui);
    platform.attach_window(imgui.io_mut(), &window, HiDpiMode::Default);

    let mut renderer = imgui_glow_renderer::AutoRenderer::initialize(gl.clone(), &mut imgui)
        .expect("failed to init renderer");

    let mut last_frame = Instant::now();
    let mut state = GuiState::new();

    // ---- Event loop (winit 0.29: 3-arg closure) ----
    event_loop.run(move |event, _target, control_flow| {
        *control_flow = winit::event_loop::ControlFlow::Wait;

        match event {
            Event::NewEvents(_) => {
                let now = Instant::now();
                imgui.io_mut().update_delta_time(now - last_frame);
                last_frame = now;
            }
            Event::WindowEvent { event, .. } => {
                platform.handle_event(imgui.io_mut(), &window, &event);
                match event {
                    WindowEvent::CloseRequested => {
                        *control_flow = winit::event_loop::ControlFlow::Exit;
                    }
                    WindowEvent::Resized(size) => {
                        surface.resize(
                            &gl_context,
                            size.width.max(1),
                            size.height.max(1),
                        );
                    }
                    _ => {}
                }
            }
            Event::MainEventsCleared => {
                window.request_redraw();
            }
            Event::RedrawRequested(_) => {
                let ui = imgui.frame();
                // --- UI ---
                Window::new("Analysis (Audio XCorr)")
                    .position([15.0, 15.0], Condition::FirstUseEver)
                    .size([600.0, 360.0], Condition::FirstUseEver)
                    .build(&ui, || {
                        ui.input_text("REF (MKV)", &mut state.ref_path).build();
                        ui.input_text("SEC (MKV)", &mut state.sec_path).build();
                        ui.input_text("TER (MKV)", &mut state.ter_path).build();
                        ui.separator();
                        ui.input_text("Work dir", &mut state.work_dir).build();
                        ui.input_text("Output dir", &mut state.out_dir).build();
                        ui.separator();
                        if ui.button("Analyze only") && !state.running {
                            state.logln("TODO: wire to vsg-core analyze; GUI verified running.");
                        }
                        ui.separator();
                        ui.text("Log:");
                        ChildWindow::new(&ui, "log").size([0.0, 220.0]).build(|| {
                            ui.text_wrapped(&state.log);
                        });
                    });

                // --- Render ---
                unsafe {
                    gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
                }
                platform.prepare_render(&ui, &window);
                let draw_data = imgui.render();
                renderer.render(draw_data).unwrap();
                surface.swap_buffers(&gl_context).unwrap();
            }
            _ => {}
        }
    });
}
