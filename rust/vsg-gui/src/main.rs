use std::{num::NonZeroU32, path::{Path, PathBuf}, process::Command};
use anyhow::{Context, Result};
use imgui::{ComboBox, Condition, Ui};
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
};
use glutin::{
    prelude::*,
    config::ConfigTemplateBuilder,
    context::ContextAttributesBuilder,
    surface::{Surface, SurfaceAttributesBuilder, WindowSurface},
};
use glutin_winit::DisplayBuilder;

// vsg-core
use vsg_core::extract::run::run_mkvextract;
use vsg_core::fsutil::{ensure_dir, default_work_dir, default_output_dir};
use vsg_core::model::{SelectionEntry, SelectionManifest, Source};
use vsg_core::analyze::audio_xcorr::{
    analyze_audio_xcorr_detailed, XCorrParams, Method, StereoMode, Band,
};

#[derive(Default)]
struct AppState {
    ref_path: String,
    sec_path: String,
    ter_path: String,
    work_dir: String,
    out_dir: String,

    // xcorr options
    chunks: u32,
    chunk_ms: u32,
    sample_rate: String, // "s24000", "s48000"
    method: usize,       // 0 = FFTPeak, 1 = CoarseRefine, 2 = RMSEdge, 3 = Hybrid
    stereo: usize,       // 0 = MixDown, 1 = Left, 2 = Right
    band: usize,         // 0 = Full, 1 = Voice

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
            method: 3,
            stereo: 0,
            band: 0,
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
    let mut audios: Vec<&ProbeTrack> = pf.tracks.iter().filter(|t| t.kind == "audio").collect();
    if let Some(lang) = prefer_lang {
        if let Some(t) = audios.iter().copied().find(|t| {
            let l = t.language.as_deref()
            .or_else(|| t.properties.as_ref().and_then(|p| p.language.as_deref()))
            .unwrap_or("und");
            l.eq_ignore_ascii_case(lang)
        }) {
            let codec = t.codec_id.clone().or(t.codec.clone()).or_else(|| t.properties.as_ref().and_then(|p| p.codec_id.clone()));
            return Some((t.id, language_of(t), codec));
        }
    }
    if let Some(t) = audios.first().copied() {
        let codec = t.codec_id.clone().or(t.codec.clone()).or_else(|| t.properties.as_ref().and_then(|p| p.codec_id.clone()));
        return Some((t.id, language_of(t), codec));
    }
    None
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

fn build_manifest(ref_path:&str, sec_path:Option<&str>, ter_path:Option<&str>) -> Result<(SelectionManifest, String, Option<String>, Option<String>)> {
    let pref_lang = {
        let ref_pf = probe_tracks(ref_path)?;
        let (_, lang, _) = pick_audio_track_id(&ref_pf, None).context("no audio in REF")?;
        lang
    };

    let mut entries: Vec<SelectionEntry> = Vec::new();
    // REF
    let ref_pf = probe_tracks(ref_path)?;
    let (ref_id, ref_lang, ref_codec) = pick_audio_track_id(&ref_pf, None).context("no audio in REF")?;
    entries.push(SelectionEntry{
        file_path: ref_path.into(),
                 track_id: ref_id,
                 r#type: "audio".into(),
                 language: Some(ref_lang.clone()),
                 codec: ref_codec.clone(),
                 container_index: Some(0),
                 name: None,
                 source: Source::REF
    });

    // SEC
    let mut sec_lang = None;
    if let Some(sp) = sec_path {
        let sec_pf = probe_tracks(sp)?;
        let (id, lang, codec) = pick_audio_track_id(&sec_pf, Some(&pref_lang)).unwrap_or_else(|| {
            pick_audio_track_id(&sec_pf, None).expect("no SEC audio")
        });
        sec_lang = Some(lang.clone());
        entries.push(SelectionEntry{
            file_path: sp.into(),
                     track_id: id,
                     r#type: "audio".into(),
                     language: Some(lang),
                     codec,
                     container_index: Some(0),
                     name: None,
                     source: Source::SEC
        });
    }

    // TER
    let mut ter_lang = None;
    if let Some(tp) = ter_path {
        let ter_pf = probe_tracks(tp)?;
        let (id, lang, codec) = pick_audio_track_id(&ter_pf, Some(&pref_lang)).unwrap_or_else(|| {
            pick_audio_track_id(&ter_pf, None).expect("no TER audio")
        });
        ter_lang = Some(lang.clone());
        entries.push(SelectionEntry{
            file_path: tp.into(),
                     track_id: id,
                     r#type: "audio".into(),
                     language: Some(lang),
                     codec,
                     container_index: Some(0),
                     name: None,
                     source: Source::TER
        });
    }

    let manifest = SelectionManifest { entries };
    Ok((manifest, ref_lang, sec_lang, ter_lang))
}

fn first_audio_under(dir:&Path) -> Option<PathBuf> {
    if !dir.exists() { return None; }
    let mut best: Option<PathBuf> = None;
    let exts = ["flac","eac3","ac3","dts","thd","opus","ogg","aac","bin"];
    for e in exts {
        if let Some(p) = std::fs::read_dir(dir).ok()
            .and_then(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| p.extension().map(|x| x==e).unwrap_or(false))) {
                best = Some(p);
                break;
            }
    }
    best
}

/* ---------------- GUI + GL bootstrap ---------------- */

fn main() -> Result<()> {
    // window + GL
    let event_loop = EventLoop::new().context("EventLoop")?;
    let window_builder = WindowBuilder::new()
    .with_title("Video Sync (Rust) - Analysis")
    .with_inner_size(LogicalSize::new(1100.0, 700.0));

    let template = ConfigTemplateBuilder::new().with_alpha_size(8).with_transparency(true);
    let display_builder = DisplayBuilder::new().with_window_builder(Some(window_builder));
    let (maybe_window, gl_config) = display_builder.build(&event_loop, template, |mut configs| {
        configs.next().expect("no GL configs")
    })?;
    let window = maybe_window.expect("winit window");

    let gl_display = gl_config.display();
    let ctx_attrs = ContextAttributesBuilder::new().build(Some(window.raw_window_handle()));
    let not_current = unsafe { gl_display.create_context(&gl_config, &ctx_attrs)? };

    let size = window.inner_size();
    let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        window.raw_window_handle(),
                                                                       NonZeroU32::new(size.width.max(1)).unwrap(),
                                                                       NonZeroU32::new(size.height.max(1)).unwrap(),
    );
    let surface = unsafe { gl_display.create_window_surface(&gl_config, &attrs)? };
    let gl_context = not_current.make_current(&surface)?;

    let gl = unsafe {
        glow::Context::from_loader_function(|s| gl_display.get_proc_address(s) as *const _)
    };

    // imgui
    let mut imgui = imgui::Context::create();
    imgui.set_ini_filename(None);
    let mut platform = WinitPlatform::init(&mut imgui);
    platform.attach_window(imgui.io_mut(), &window, HiDpiMode::Default);

    let mut renderer = imgui_gl::AutoRenderer::initialize(&mut imgui, &gl, |s| {
        gl_display.get_proc_address(s) as *const _
    }).expect("renderer");

    // state
    let mut state = AppState::new();

    // UI loop
    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                *control_flow = winit::event_loop::ControlFlow::Exit
            }
            Event::MainEventsCleared => window.request_redraw(),
                   Event::RedrawRequested(_) => {
                       platform.prepare_frame(imgui.io_mut(), &window).unwrap();
                       let ui = imgui.frame();
                       draw_ui(&ui, &mut state);

                       // render
                       unsafe {
                           gl.clear_color(0.12, 0.12, 0.13, 1.0);
                           gl.clear(glow::COLOR_BUFFER_BIT);
                       }
                       platform.prepare_render(&ui, &window);
                       let draw_data = ui.render();
                       renderer.render(draw_data).unwrap();
                       surface.swap_buffers(&gl_context).unwrap();
                   }
                   _ => {}
        }
    });
}

fn draw_ui(ui:&Ui, s:&mut AppState) {
    use imgui::*;

    Window::new("Analysis (Audio XCorr)")
    .size([1050.0, 500.0], Condition::FirstUseEver)
    .build(ui, || {
        ui.input_text("REF (MKV)", &mut s.ref_path).build();
        ui.input_text("SEC (MKV)", &mut s.sec_path).build();
        ui.input_text("TER (MKV)", &mut s.ter_path).build();
        ui.separator();
        ui.input_text("Work dir", &mut s.work_dir).build();
        ui.input_text("Output dir", &mut s.out_dir).build();

        ui.separator();
        ui.text("Cross-correlation options:");
        ui.input_int("Chunks (spanned)", &mut (s.chunks as i32)).build();
        ui.input_int("Chunk ms", &mut (s.chunk_ms as i32)).build();
        ComboBox::new("Sample rate").build_simple_string(ui, &mut s.sample_rate, &["s24000","s48000"]);
        ComboBox::new("Method").build_simple_string(ui, &mut s.method, &["FFTPeak","CoarseRefine","RMSEdge","Hybrid"]);
        ComboBox::new("Stereo").build_simple_string(ui, &mut s.stereo, &["MixDown","Left","Right"]);
        ComboBox::new("Band").build_simple_string(ui, &mut s.band, &["Full","Voice"]);

        if ui.button("Analyze only") && !s.running {
            s.running = true;
            if let Err(e) = do_analyze_only(s) {
                s.logln(format!("ERROR: {e:#}"));
            }
            s.running = false;
        }

        ui.separator();
        ui.text("Log:");
        ChildWindow::new("log").size([0.0, 220.0]).build(ui, || {
            ui.text_wrapped(&s.log);
        });
    });
}

fn do_analyze_only(s: &mut AppState) -> Result<()> {
    s.log.clear();

    ensure_dir(&PathBuf::from(&s.work_dir))?;
    ensure_dir(&PathBuf::from(&s.out_dir))?;

    s.logln("Probing & building selection manifest…");
    let (manifest, ref_lang, sec_lang, ter_lang) = build_manifest(&s.ref_path, nempty(&s.sec_path), nempty(&s.ter_path))?;
    s.logln(format!("REF language: {}", ref_lang));
    if let Some(l) = &sec_lang { s.logln(format!("SEC language: {}", l)); }
    if let Some(l) = &ter_lang { s.logln(format!("TER language: {}", l)); }

    s.logln("Running mkvextract (analysis mode)…");
    let work_root = PathBuf::from(&s.work_dir);
    let summary = run_mkvextract(&manifest, &work_root).context("mkvextract")?;
    s.logln(format!("Extracted: {:?}", summary));

    let ref_audio = first_audio_under(&work_root.join("ref")).context("ref audio not found")?;
    let sec_audio = first_audio_under(&work_root.join("sec"));
    let ter_audio = first_audio_under(&work_root.join("ter"));

    s.logln(format!("ref: {}", ref_audio.display()));
    if let Some(p) = &sec_audio { s.logln(format!("sec: {}", p.display())); }
    if let Some(p) = &ter_audio { s.logln(format!("ter: {}", p.display())); }

    let params = XCorrParams {
        chunks: s.chunks as usize,
        chunk_ms: s.chunk_ms as usize,
        sample_rate: s.sample_rate.clone(), // "s24000" | "s48000"
        method: match s.method { 0=>Method::FFTPeak, 1=>Method::CoarseRefine, 2=>Method::RMSEdge, _=>Method::Hybrid },
        stereo: match s.stereo { 1=>StereoMode::Left, 2=>StereoMode::Right, _=>StereoMode::MixDown },
        band: match s.band { 1=>Band::Voice, _=>Band::Full },
        nanosecond_output: true,
    };

    let mut results = json!({
        "meta": {
            "ref_audio_path": ref_audio,
            "ref_language": ref_lang,
            "sec_language": sec_lang.unwrap_or_else(|| "und".to_string()),
                            "ter_language": ter_lang.unwrap_or_else(|| "und".to_string()),
                            "chunks": s.chunks,
                            "chunk_ms": s.chunk_ms,
                            "sample_rate": s.sample_rate,
                            "method": s.method,
                            "stereo": s.stereo,
                            "band": s.band,
        },
        "runs": {}
    });

    if let Some(sec) = &sec_audio {
        s.logln("Analyzing REF vs SEC…");
        let detail = analyze_audio_xcorr_detailed(ref_audio.to_string_lossy().as_ref(), sec.to_string_lossy().as_ref(), &params)
        .context("xcorr SEC")?;
        s.logln(format!("SEC Δt (ns): {}", detail.global_fit.ns));
        results["runs"]["sec"] = serde_json::to_value(detail)?;
    }

    if let Some(ter) = &ter_audio {
        s.logln("Analyzing REF vs TER…");
        let detail = analyze_audio_xcorr_detailed(ref_audio.to_string_lossy().as_ref(), ter.to_string_lossy().as_ref(), &params)
        .context("xcorr TER")?;
        s.logln(format!("TER Δt (ns): {}", detail.global_fit.ns));
        results["runs"]["ter"] = serde_json::to_value(detail)?;
    }

    let out_path = PathBuf::from(&s.out_dir).join("analysis_results.json");
    std::fs::write(&out_path, serde_json::to_vec_pretty(&results)?)?;
    s.logln(format!("Wrote {}", out_path.display()));
    Ok(())
}

fn nempty(s:&str) -> Option<&str> { if s.trim().is_empty() { None } else { Some(s) } }
