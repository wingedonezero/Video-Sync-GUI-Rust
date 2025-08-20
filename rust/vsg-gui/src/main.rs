use std::{
    num::NonZeroU32,
    path::{Path, PathBuf},
    process::Command,
    fs,
};

use anyhow::{anyhow, Context, Result};
use imgui::{Ui};
use imgui_glow_renderer as imgui_gl;
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use glow::HasContext as _;
use serde::Deserialize;
use serde_json::json;

use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::WindowBuilder,
    raw_window_handle::HasRawWindowHandle, // for window.raw_window_handle()
};

use glutin::{
    prelude::*,
    config::ConfigTemplateBuilder,
    context::ContextAttributesBuilder,
    surface::{SurfaceAttributesBuilder, WindowSurface},
    display::GetGlDisplay, // for gl_config.display()
};

use glutin_winit::DisplayBuilder;

// core: extraction only; (do NOT import analyze symbols)
use vsg_core::extract::run::run_mkvextract;
use vsg_core::fsutil::{default_work_dir, default_output_dir};
use vsg_core::model::{SelectionEntry, SelectionManifest, Source};

#[derive(Default)]
struct AppState {
    ref_path: String,
    sec_path: String,
    ter_path: String,
    work_dir: String,
    out_dir: String,

    // analysis options (passed to CLI for now)
    chunks: u32,
    chunk_ms: u32,
    sample_rate: String, // "s24000" | "s48000"

    log: String,
    running: bool,
}

impl AppState {
    fn new() -> Self {
        Self {
            work_dir: default_work_dir().to_string_lossy().to_string(),
            out_dir: default_output_dir().to_string_lossy().to_string(),
            chunks: 10,
            chunk_ms: 6000,
            sample_rate: "s48000".into(),
            ..Default::default()
        }
    }
    fn logln(&mut self, s: impl AsRef<str>) {
        use std::fmt::Write;
        let _ = writeln!(self.log, "{}", s.as_ref());
    }
}

/* -------- mkvmerge probing to choose tracks ---------- */

#[derive(Debug, Deserialize)]
struct ProbeFile { tracks: Vec<ProbeTrack> }

#[derive(Debug, Deserialize)]
struct ProbeTrack {
    id: u32,
    #[serde(rename="type")]
    kind: String,
    #[serde(default)]
    codec_id: Option<String>,
    #[serde(default)]
    codec: Option<String>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    properties: Option<ProbeProps>,
}

#[derive(Debug, Deserialize)]
struct ProbeProps {
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    codec_id: Option<String>,
}

fn find_tool(name: &str) -> Result<PathBuf> {
    which::which(name).with_context(|| format!("finding tool {}", name))
}

fn probe_tracks(path: &str) -> Result<ProbeFile> {
    let mkvmerge = find_tool("mkvmerge")?;
    let out = Command::new(mkvmerge)
    .arg("-J").arg(path)
    .output()
    .with_context(|| "spawn mkvmerge -J")?;
    if !out.status.success() {
        anyhow::bail!("mkvmerge -J failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    let txt = String::from_utf8(out.stdout).context("utf8 mkvmerge json")?;
    let pf: ProbeFile = serde_json::from_str(&txt).context("parse mkvmerge -J json")?;
    Ok(pf)
}

fn pick_audio_track_id(pf: &ProbeFile, prefer_lang: Option<&str>) -> Option<(u32, String, Option<String>)> {
    let audios: Vec<&ProbeTrack> = pf.tracks.iter().filter(|t| t.kind == "audio").collect();
    if audios.is_empty() { return None; }

    let pick = if let Some(lang) = prefer_lang {
        audios.iter().copied().find(|t| {
            let l = t.language.as_deref()
            .or_else(|| t.properties.as_ref().and_then(|p| p.language.as_deref()))
            .unwrap_or("und");
            l.eq_ignore_ascii_case(lang)
        }).unwrap_or(audios[0])
    } else {
        audios[0]
    };

    let codec = pick.codec_id.clone().or(pick.codec.clone()).or_else(|| pick.properties.as_ref().and_then(|p| p.codec_id.clone()));
    Some((pick.id, language_of(pick), codec))
}

fn language_of(t: &ProbeTrack) -> String {
    t.language.as_deref()
    .or_else(|| t.properties.as_ref().and_then(|p| p.language.as_deref()))
    .unwrap_or("und").to_string()
}

fn codec_ext_hint(codec: Option<&str>) -> &'static str {
    let c = codec.unwrap_or("").to_ascii_lowercase();
    match c.as_str() {
        "aac" | "a_aac" | "mp4a" | "a_aac_mpeg2lc" | "a_aac_mpeg4lc" => "aac",
        "ac3" | "a_ac3" => "ac3",
        "eac3" | "e-ac-3" | "a_eac3" | "a_e-ac-3" => "eac3",
        "dts" | "a_dts" | "a_dts_hd" | "a_dts-x" => "dts",
        "truehd" | "a_truehd" => "thd",
        "flac" | "a_flac" => "flac",
        "opus" | "a_opus" => "opus",
        "vorbis" | "a_vorbis" => "ogg",
        _ => "bin",
    }
}

fn build_manifest(ref_path:&str, sec_path:Option<&str>, ter_path:Option<&str>) -> Result<(SelectionManifest, String)> {
    // choose REF first to derive a preferred language
    let ref_pf = probe_tracks(ref_path)?;
    let (_, ref_lang, _) = pick_audio_track_id(&ref_pf, None).context("no audio in REF")?;

    // REF entry
    let (ref_id, _lang, ref_codec) = pick_audio_track_id(&ref_pf, None).unwrap();
    let mut ref_tracks = vec![SelectionEntry{
        file_path: ref_path.into(),
        track_id: ref_id,
        r#type: "audio".into(),
        language: Some(ref_lang.clone()),
        codec: ref_codec.clone(),
        container_index: Some(0),
        name: None,
        source: Source::REF
    }];

    // SEC entries
    let mut sec_tracks = Vec::new();
    if let Some(sp) = sec_path {
        let sec_pf = probe_tracks(sp)?;
        let (id, _lang, codec) = pick_audio_track_id(&sec_pf, Some(&ref_lang))
        .or_else(|| pick_audio_track_id(&sec_pf, None))
        .context("no audio in SEC")?;
        sec_tracks.push(SelectionEntry{
            file_path: sp.into(),
                        track_id: id,
                        r#type: "audio".into(),
                        language: None,
                        codec,
                        container_index: Some(0),
                        name: None,
                        source: Source::SEC
        });
    }

    // TER entries
    let mut ter_tracks = Vec::new();
    if let Some(tp) = ter_path {
        let ter_pf = probe_tracks(tp)?;
        let (id, _lang, codec) = pick_audio_track_id(&ter_pf, Some(&ref_lang))
        .or_else(|| pick_audio_track_id(&ter_pf, None))
        .context("no audio in TER")?;
        ter_tracks.push(SelectionEntry{
            file_path: tp.into(),
                        track_id: id,
                        r#type: "audio".into(),
                        language: None,
                        codec,
                        container_index: Some(0),
                        name: None,
                        source: Source::TER
        });
    }

    let manifest = SelectionManifest { ref_tracks, sec_tracks, ter_tracks };
    Ok((manifest, ref_lang))
}

fn first_audio_under(dir:&Path) -> Option<PathBuf> {
    if !dir.exists() { return None; }
    let exts = ["flac","eac3","ac3","dts","thd","opus","ogg","aac","bin"];
    for e in exts {
        if let Some(p) = std::fs::read_dir(dir).ok()
            .and_then(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| p.extension().map(|x| x==e).unwrap_or(false))) {
                return Some(p);
            }
    }
    None
}

/* ---------------- GUI + GL bootstrap ---------------- */

fn main() -> Result<()> {
    // window + GL
    let event_loop = EventLoop::new().context("EventLoop::new failed")?;
    let window_builder = WindowBuilder::new()
    .with_title("Video Sync (Rust) - Analysis")
    .with_inner_size(LogicalSize::new(1100.0, 700.0));

    let template = ConfigTemplateBuilder::new().with_alpha_size(8).with_transparency(true);
    let display_builder = DisplayBuilder::new().with_window_builder(Some(window_builder));

    // NOTE: glutin-winit build returns a Box<dyn StdError> which isn’t Send/Sync.
    // Map it to a string so anyhow can convert.
    let (maybe_window, gl_config) = display_builder
    .build(&event_loop, template, |mut configs| {
        configs.next().expect("no GL configs")
    })
    .map_err(|e| anyhow!(e.to_string()))?;

    let window = maybe_window.expect("winit window");
    let gl_display = gl_config.display();

    let ctx_attrs = ContextAttributesBuilder::new().build(Some(window.raw_window_handle()));
    let not_current = unsafe { gl_display.create_context(&gl_config, &ctx_attrs) }
    .map_err(|e| anyhow!(e.to_string()))?;

    let size = window.inner_size();
    let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        window.raw_window_handle(),
                                                                       NonZeroU32::new(size.width.max(1)).unwrap(),
                                                                       NonZeroU32::new(size.height.max(1)).unwrap(),
    );
    let surface = unsafe { gl_display.create_window_surface(&gl_config, &attrs) }
    .map_err(|e| anyhow!(e.to_string()))?;
    let gl_context = not_current.make_current(&surface)
    .map_err(|e| anyhow!(e.to_string()))?;

    let gl = unsafe {
        glow::Context::from_loader_function(|s| gl_display.get_proc_address(s) as *const _)
    };

    // imgui
    let mut imgui = imgui::Context::create();
    imgui.set_ini_filename(None);
    let mut platform = WinitPlatform::init(&mut imgui);
    platform.attach_window(imgui.io_mut(), &window, HiDpiMode::Default);

    // imgui_glow_renderer 0.12 signature: initialize(gl_ctx, &mut imgui_ctx)
    let mut renderer = imgui_gl::AutoRenderer::initialize(gl, &mut imgui)
    .expect("renderer init");

    // state
    let mut state = AppState::new();

    // UI loop (use AboutToWait to avoid missing variants on some platforms)
    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                *control_flow = winit::event_loop::ControlFlow::Exit
            }
            Event::AboutToWait => window.request_redraw(),
                   Event::RedrawRequested(_) => {
                       platform.prepare_frame(imgui.io_mut(), &window).unwrap();
                       let ui = imgui.frame();
                       draw_ui(&ui, &mut state);

                       unsafe {
                           renderer.gl_context().clear_color(0.12, 0.12, 0.13, 1.0);
                           renderer.gl_context().clear(glow::COLOR_BUFFER_BIT);
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

fn draw_ui(ui:&Ui, s:&mut AppState) {
    use imgui::*;

    ui.window("Analysis (Audio XCorr)")
    .size([1050.0, 500.0], Condition::FirstUseEver)
    .build(|| {
        ui.input_text("REF (MKV)", &mut s.ref_path).build();
        ui.input_text("SEC (MKV)", &mut s.sec_path).build();
        ui.input_text("TER (MKV)", &mut s.ter_path).build();
        ui.separator();
        ui.input_text("Work dir", &mut s.work_dir).build();
        ui.input_text("Output dir", &mut s.out_dir).build();

        ui.separator();
        ui.text("Analysis options (CLI fallback):");
        let mut chunks_i = s.chunks as i32;
        if ui.input_int("Chunks (spanned)", &mut chunks_i).build() { s.chunks = chunks_i.max(1) as u32; }
        let mut chunk_ms_i = s.chunk_ms as i32;
        if ui.input_int("Chunk ms", &mut chunk_ms_i).build() { s.chunk_ms = chunk_ms_i.max(100) as u32; }
        ui.input_text("Sample rate (s24000/s48000)", &mut s.sample_rate).build();

        if ui.button("Analyze only (extract + call CLI)") && !s.running {
            s.running = true;
            if let Err(e) = do_extract_and_cli_analyze(s) {
                s.logln(format!("ERROR: {e:#}"));
            }
            s.running = false;
        }

        ui.separator();
        ui.text("Log:");
        ui.child_window("log").size([0.0, 220.0]).build(|| {
            ui.text_wrapped(&s.log);
        });
    });
}

/* ------------- extraction + CLI analyze fallback -------------- */

fn do_extract_and_cli_analyze(s: &mut AppState) -> Result<()> {
    s.log.clear();

    fs::create_dir_all(&s.work_dir).ok();
    fs::create_dir_all(&s.out_dir).ok();

    s.logln("Probing & building selection manifest…");
    let (manifest, ref_lang) = build_manifest(&s.ref_path, nempty(&s.sec_path), nempty(&s.ter_path))?;
    s.logln(format!("REF language: {}", ref_lang));

    s.logln("Running mkvextract…");
    let work_root = PathBuf::from(&s.work_dir);
    let _summary = run_mkvextract(&manifest, &work_root).context("mkvextract")?;
    s.logln("Extraction complete.");

    let ref_audio = first_audio_under(&work_root.join("ref")).context("ref audio not found")?;
    let sec_audio = first_audio_under(&work_root.join("sec"));
    let ter_audio = first_audio_under(&work_root.join("ter"));

    s.logln(format!("ref: {}", ref_audio.display()));
    if let Some(p) = &sec_audio { s.logln(format!("sec: {}", p.display())); }
    if let Some(p) = &ter_audio { s.logln(format!("ter: {}", p.display())); }

    // try CLI analyze (keeps GUI build stable until core API is finalized)
    let cli = find_tool("vsg-cli").or_else(|_| {
        // try target/release in workspace if running locally
        let guess = PathBuf::from("target").join("release").join("vsg-cli");
        if guess.exists() { Ok(guess) } else { Err(anyhow!("vsg-cli not found in PATH")) }
    })?;

    let mut cmd = Command::new(cli);
    cmd.arg("analyze")
    .arg("--chunks").arg(s.chunks.to_string())
    .arg("--chunk-ms").arg(s.chunk_ms.to_string())
    .arg("--sample-rate").arg(&s.sample_rate)
    .arg("--ref").arg(ref_audio);

    if let Some(p) = sec_audio { cmd.arg("--sec").arg(p); }
    if let Some(p) = ter_audio { cmd.arg("--ter").arg(p); }

    s.logln("Running vsg-cli analyze …");
    let out = cmd.output().with_context(|| "spawn vsg-cli")?;
    if !out.status.success() {
        s.logln(format!("vsg-cli stderr:\n{}", String::from_utf8_lossy(&out.stderr)));
        anyhow::bail!("vsg-cli analyze failed");
    }

    let text = String::from_utf8(out.stdout).unwrap_or_default();
    s.logln("vsg-cli output:");
    s.logln(text.clone());

    // write JSON/plain text to out_dir
    let out_path = PathBuf::from(&s.out_dir).join("analysis_results.json");
    let val = json!({ "cli_raw": text });
    std::fs::write(&out_path, serde_json::to_vec_pretty(&val)?)?;
    s.logln(format!("Wrote {}", out_path.display()));
    Ok(())
}

fn nempty(s:&str) -> Option<&str> { if s.trim().is_empty() { None } else { Some(s) } }
