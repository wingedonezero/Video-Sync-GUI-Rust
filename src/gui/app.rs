use iced::{Application, Command, Element, Theme};
use iced_aw::Modal;
use crossbeam_channel::{unbounded, Sender, Receiver};
use std::path::{Path, PathBuf};
use std::fs;

use crate::gui::{
    theme::{self, UiTheme},
    view_inputs,
    view_status,
    manual::{self, JobPreview},
    settings_modal,
};

pub enum WorkerMsg {
    Log(String), Progress(f32), Status(String),
    FinishedJob(serde_json::Map<String, serde_json::Value>),
    FinishedAll(Vec<serde_json::Map<String, serde_json::Value>>),
}

#[derive(Debug, Clone)]
pub enum Msg {
    // inputs
    Inputs(view_inputs::Msg),
    // settings
    OpenSettings, CloseSettings(bool), SettingsChanged,
    // manual
    Manual(manual::Msg), OpenManual(JobPreview),
    // worker
    Worker(WorkerMsg),
}

pub struct App {
    cfg: vsg::core::config::AppConfig,
    // inputs
    ref_path: String, sec_path: String, ter_path: String,
    auto_apply: bool, auto_apply_strict: bool, archive_logs: bool,
    // ui
    status: String, progress: f32,
    sec_delay: Option<i64>, ter_delay: Option<i64>,
    log_text: String, show_settings: bool, show_manual: bool,
    theme_choice: UiTheme,
    // manual state
    current_preview: Option<JobPreview>,
    final_list: Vec<manual::TrackRow>,
    // auto-apply memory
    last_template: Option<Vec<manual::TemplateRow>>,
    last_signature_loose: Option<std::collections::HashMap<String,usize>>,
    last_signature_strict: Option<std::collections::HashMap<String,usize>>,
    // worker comms
    rx: Option<Receiver<WorkerMsg>>, tx: Option<Sender<WorkerMsg>>,
    last_results: Vec<serde_json::Map<String, serde_json::Value>>,
}

impl Default for App {
    fn default() -> Self {
        let cfg = vsg::core::config::AppConfig::new("settings.json");
        Self {
            ref_path: cfg.get_string("last_ref_path"),
            sec_path: cfg.get_string("last_sec_path"),
            ter_path: cfg.get_string("last_ter_path"),
            auto_apply: false,
            auto_apply_strict: cfg.get_bool("auto_apply_strict"),
            archive_logs: cfg.get_bool("archive_logs"),
            status: "Ready".into(), progress: 0.0,
            sec_delay: None, ter_delay: None, log_text: String::new(),
            show_settings: false, show_manual: false,
            theme_choice: UiTheme::Oxocarbon,
            current_preview: None, final_list: vec![],
            last_template: None, last_signature_loose: None, last_signature_strict: None,
            rx: None, tx: None, last_results: vec![],
            cfg,
        }
    }
}

impl Application for App {
    type Executor = iced::executor::Default;
    type Message = Msg;
    type Theme = Theme;
    type Flags = ();

    fn new(_: ()) -> (Self, Command<Msg>) { (Self::default(), Command::none()) }
    fn title(&self) -> String { "Video/Audio Sync & Merge — Iced".into() }
    fn theme(&self) -> Theme { theme::theme_of(self.theme_choice) }

    fn update(&mut self, message: Msg) -> Command<Msg> {
        match message {
            Msg::Inputs(m) => match m {
                view_inputs::Msg::BrowseRef => {
                    if let Some(p)=rfd::FileDialog::new()
                        .set_directory(Path::new(&self.ref_path).parent().unwrap_or(Path::new(".")))
                        .pick_file()
                        { self.ref_path = p.to_string_lossy().to_string(); }
                }
                view_inputs::Msg::BrowseSec => {
                    if let Some(p)=rfd::FileDialog::new()
                        .set_directory(Path::new(&self.sec_path).parent().unwrap_or(Path::new(".")))
                        .pick_file()
                        { self.sec_path = p.to_string_lossy().to_string(); }
                }
                view_inputs::Msg::BrowseTer => {
                    if let Some(p)=rfd::FileDialog::new()
                        .set_directory(Path::new(&self.ter_path).parent().unwrap_or(Path::new(".")))
                        .pick_file()
                        { self.ter_path = p.to_string_lossy().to_string(); }
                }
                view_inputs::Msg::RefChanged(s)=>self.ref_path=s,
                view_inputs::Msg::SecChanged(s)=>self.sec_path=s,
                view_inputs::Msg::TerChanged(s)=>self.ter_path=s,
                view_inputs::Msg::AutoApply(v)=>self.auto_apply=v,
                view_inputs::Msg::AutoApplyStrict(v)=>self.auto_apply_strict=v,
                view_inputs::Msg::ArchiveLogs(v)=>self.archive_logs=v,
                view_inputs::Msg::AnalyzeOnly => return self.start_batch(false),
                view_inputs::Msg::AnalyzeMerge => return self.pre_scan_then_manual(),
                view_inputs::Msg::ThemeChanged(t)=>self.theme_choice=t,
                view_inputs::Msg::OpenSettings => self.show_settings=true,
            },
            Msg::OpenSettings => self.show_settings=true,
            Msg::CloseSettings(save) => { if save { self.persist_config(); } self.show_settings=false; }
            Msg::SettingsChanged => {},
            Msg::OpenManual(preview) => { self.current_preview=Some(preview); self.show_manual=true; }
            Msg::Manual(m) => match m {
                manual::Msg::CloseManual(ok) => {
                    if ok {
                        self.last_template = Some(manual::template_from_final(&self.final_list));
                        self.show_manual=false;
                        return self.start_batch(true);
                    } else { self.show_manual=false; }
                }
                manual::Msg::FinalAdd(row) => {
                    if row.ttype=="video"&&(row.source=="SEC"||row.source=="TER") { /* blocked */ }
                    else { self.final_list.push(row); }
                }
                manual::Msg::FinalRemove(i)=>{ if i<self.final_list.len(){ self.final_list.remove(i); } }
                manual::Msg::FinalMoveUp(i)=>{ if i>0 && i<self.final_list.len(){ self.final_list.swap(i,i-1);} }
                manual::Msg::FinalMoveDown(i)=>{ if i+1<self.final_list.len(){ self.final_list.swap(i,i+1);} }
                manual::Msg::FinalToggleDefault(i)=>{
                    if i<self.final_list.len(){
                        let kind=self.final_list[i].ttype.clone();
                        for (j,t) in self.final_list.iter_mut().enumerate(){ if t.ttype==kind{ t.is_default = j==i; } }
                    }
                }
                manual::Msg::FinalToggleForced(i)=>{
                    if i<self.final_list.len() && self.final_list[i].is_subs(){
                        let new=!self.final_list[i].is_forced_display;
                        for (j,t) in self.final_list.iter_mut().enumerate(){ if t.is_subs(){ t.is_forced_display = j==i && new; } }
                    }
                }
                manual::Msg::FinalToggleName(i)=>{ if i<self.final_list.len(){ self.final_list[i].apply_track_name = !self.final_list[i].apply_track_name; } }
                manual::Msg::FinalToggleRescale(i)=>{ if i<self.final_list.len() && self.final_list[i].is_subs(){ self.final_list[i].rescale = !self.final_list[i].rescale; } }
                manual::Msg::FinalToggleConvert(i)=>{
                    if i<self.final_list.len() && self.final_list[i].is_subs() && self.final_list[i].is_srt(){
                        self.final_list[i].convert_to_ass = !self.final_list[i].convert_to_ass;
                    }
                }
                manual::Msg::FinalSizeChanged(i,v)=>{ if i<self.final_list.len() && self.final_list[i].is_subs(){ self.final_list[i].size_multiplier=v; } }
            },
            Msg::Worker(w) => match w {
                WorkerMsg::Log(s)=> { self.log_text.push_str(&s); self.log_text.push('\n'); }
                WorkerMsg::Progress(p)=> self.progress=p,
                WorkerMsg::Status(s)=> self.status=s,
                WorkerMsg::FinishedJob(map)=>{
                    if let Some(v)=map.get("delay_sec").and_then(|v| v.as_i64()) { self.sec_delay=Some(v); }
                    if let Some(v)=map.get("delay_ter").and_then(|v| v.as_i64()) { self.ter_delay=Some(v); }
                    self.last_results.push(map);
                }
                WorkerMsg::FinishedAll(all)=>{
                    self.status=format!("All {} jobs finished.", all.len());
                    self.progress=1.0;
                    if self.archive_logs { self.archive_logs_zip(); }
                }
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<Msg> {
        let inputs = view_inputs::Inputs{
            ref_path: &self.ref_path, sec_path: &self.sec_path, ter_path: &self.ter_path,
            auto_apply: self.auto_apply, auto_apply_strict: self.auto_apply_strict,
            archive_logs: self.archive_logs, theme_choice: self.theme_choice,
        }.view().map(Msg::Inputs);

        let status = view_status::Status{
            status: &self.status, progress: self.progress,
            sec_delay: self.sec_delay, ter_delay: self.ter_delay, log_text: &self.log_text,
        }.view();

        let base = iced::widget::column![inputs, status].spacing(12);

        let with_settings = Modal::new(self.show_settings, base, || settings_modal::modal())
        .on_close(Msg::CloseSettings(false));

        if self.show_manual {
            let m = crate::gui::manual::ManualView{
                preview: self.current_preview.as_ref().unwrap(),
                final_list: &self.final_list,
            }.view().map(Msg::Manual);
            Modal::new(true, with_settings, || m)
            .on_close(Msg::Manual(manual::Msg::CloseManual(false)))
            .into()
        } else {
            with_settings.into()
        }
    }
}

/* ---------------- helpers ---------------- */

impl App {
    fn persist_config(&mut self) {
        self.cfg.set_string("last_ref_path", &self.ref_path);
        self.cfg.set_string("last_sec_path", &self.sec_path);
        self.cfg.set_string("last_ter_path", &self.ter_path);
        self.cfg.set_bool("archive_logs", self.archive_logs);
        self.cfg.set_bool("auto_apply_strict", self.auto_apply_strict);
        self.cfg.save();
    }

    fn archive_logs_zip(&self) {
        let mut out_dir: Option<PathBuf> = None;
        for m in &self.last_results {
            if let Some(p) = m.get("output").and_then(|v| v.as_str()) {
                out_dir = Some(PathBuf::from(p).parent().unwrap_or(Path::new("."))).cloned();
                break
            }
        }
        let Some(dir)=out_dir else { return };
        let logs: Vec<_> = match fs::read_dir(&dir) {
            Ok(it)=>it.filter_map(|e|e.ok()).map(|e|e.path()).filter(|p|p.extension().map(|e|e=="log").unwrap_or(false)).collect(),
            Err(_)=>return,
        };
        if logs.is_empty() { return; }
        let zip_path = dir.join(format!("{}.zip", dir.file_name().unwrap_or_default().to_string_lossy()));
        let file = match fs::File::create(&zip_path) { Ok(f)=>f, Err(_)=>return };
        let mut zipw = zip::ZipWriter::new(file);
        let opts = zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for p in &logs {
            if let Ok(mut f) = fs::File::open(p) {
                let name = p.file_name().unwrap().to_string_lossy().to_string();
                let _ = zipw.start_file(name, opts);
                use std::io::{Read, Write};
                let mut buf = Vec::new(); let _ = f.read_to_end(&mut buf); let _ = zipw.write_all(&buf);
            }
        }
        let _ = zipw.finish();
        for p in logs { let _ = fs::remove_file(p); }
    }

    fn runner_with(&self, tx: Sender<WorkerMsg>) -> vsg::core::process::CommandRunner {
        vsg::core::process::CommandRunner::new(self.cfg.map().clone(), move |line: String| {
            let _ = tx.send(WorkerMsg::Log(line));
        })
    }

    // Pre-scan first job, compute signature, maybe pre-populate manual
    pub fn pre_scan_then_manual(&mut self) -> Command<Msg> {
        self.persist_config();
        self.log_text.clear();
        self.status = "Pre-scanning…".into();

        let ref_path = self.ref_path.clone();
        let sec_path = if self.sec_path.trim().is_empty(){ None } else { Some(self.sec_path.clone()) };
        let ter_path = if self.ter_path.trim().is_empty(){ None } else { Some(self.ter_path.clone()) };

        let auto_apply = self.auto_apply;
        let strict = self.auto_apply_strict;
        let last_tpl = self.last_template.clone();
        let last_sig_loose = self.last_signature_loose.clone();
        let last_sig_strict = self.last_signature_strict.clone();

        let cfg_map = self.cfg.map().clone();

        Command::perform(async move {
            // discover jobs
            let jobs = match vsg::core::job_discovery::discover_jobs(&ref_path, sec_path.as_deref(), ter_path.as_deref()) {
                Ok(j)=>j, Err(e)=> return Msg::Worker(WorkerMsg::Log(format!("[Pre-Scan] {e}")))
            };
            if jobs.is_empty() { return Msg::Worker(WorkerMsg::Status("No jobs found.".into())); }
            let job0 = &jobs[0];

            // lightweight runner for mkvmerge -J only
            let (tx,_rx) = unbounded::<WorkerMsg>();
            let runner = vsg::core::process::CommandRunner::new(cfg_map.clone(), move |s| { let _=tx.send(WorkerMsg::Log(s)); });
            let tool_paths = serde_json::Map::new();

            let info = vsg::core::mkv_utils::get_track_info_for_dialog(
                &job0.ref_path, job0.sec.as_deref(), job0.ter.as_deref(), &runner, &tool_paths
            );

            let to_rows = |k:&str| manual::from_core_list(info.get(k).cloned().unwrap_or_default());

            let mut preview = JobPreview{
                ref_path: job0.ref_path.clone(),
                         sec_path: job0.sec.clone(),
                         ter_path: job0.ter.clone(),
                         tracks_ref: to_rows("REF"), tracks_sec: to_rows("SEC"), tracks_ter: to_rows("TER"),
                         prepopulated: false,
            };

            // compute signatures
            let sig_loose = manual::signature_loose(&preview);
            let sig_strict = manual::signature_strict(&preview);

            // pre-populate?
            let mut prepop = false;
            if auto_apply {
                let matches = if strict {
                    if let Some(prev)=last_sig_strict { prev == sig_strict } else { false }
                } else {
                    if let Some(prev)=last_sig_loose { prev == sig_loose } else { false }
                };
                if matches {
                    if let Some(tpl)=last_tpl.as_ref() {
                        let _materialized = manual::materialize_template(tpl, &preview);
                        prepop = true; // banner; App will let the user confirm/adjust
                    }
                }
            }
            preview.prepopulated = prepop;

            Msg::OpenManual(preview)
        }, |m| m)
    }

    pub fn start_batch(&mut self, and_merge: bool) -> Command<Msg> {
        self.persist_config();
        self.progress=0.0; self.sec_delay=None; self.ter_delay=None;
        self.log_text.clear(); self.status="Starting…".into();
        self.last_results.clear();

        let jobs = match vsg::core::job_discovery::discover_jobs(
            &self.ref_path,
            if self.sec_path.trim().is_empty(){None}else{Some(self.sec_path.as_str())},
                if self.ter_path.trim().is_empty(){None}else{Some(self.ter_path.as_str())}
        ) {
            Ok(v)=>v, Err(e)=>{ self.log_text.push_str(&format!("[Job Discovery Error] {e}\n")); return Command::none(); }
        };
        if jobs.is_empty() { self.status="No jobs to run.".into(); return Command::none(); }

        // convert jobs to maps and attach manual layout to first job (if merge)
        let mut jobs_map: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();
        for (i,j) in jobs.iter().enumerate() {
            let mut m=serde_json::Map::new();
            m.insert("ref".into(), serde_json::Value::String(j.ref_path.clone()));
            if let Some(s)=&j.sec { m.insert("sec".into(), serde_json::Value::String(s.clone())); }
            if let Some(t)=&j.ter { m.insert("ter".into(), serde_json::Value::String(t.clone())); }
            if and_merge && i==0 {
                let arr: Vec<_> = self.final_list.iter().map(|t|{
                    let mut o=serde_json::Map::new();
                    o.insert("source".into(), t.source.clone().into());
                    o.insert("id".into(), t.id.into());
                    o.insert("type".into(), t.ttype.clone().into());
                    o.insert("codec_id".into(), t.codec_id.clone().into());
                    o.insert("lang".into(), t.lang.clone().into());
                    o.insert("name".into(), t.name.clone().into());
                    o.insert("is_default".into(), t.is_default.into());
                    o.insert("is_forced_display".into(), t.is_forced_display.into());
                    o.insert("apply_track_name".into(), t.apply_track_name.into());
                    o.insert("convert_to_ass".into(), t.convert_to_ass.into());
                    o.insert("rescale".into(), t.rescale.into());
                    o.insert("size_multiplier".into(), serde_json::Number::from_f64(t.size_multiplier).unwrap_or_else(||serde_json::Number::from(1)).into());
                    serde_json::Value::Object(o)
                }).collect();
                m.insert("manual_layout".into(), serde_json::Value::Array(arr));
            }
            jobs_map.push(m);
        }

        let cfg_map = self.cfg.map().clone();
        let (tx, rx) = unbounded::<WorkerMsg>();
        self.tx=Some(tx.clone()); self.rx=Some(rx.clone());

        Command::perform(async move {
            let log_cb = move |s:String| { let _=tx.send(WorkerMsg::Log(s)); };
            let prog_cb = move |p:f32| { let _=tx.send(WorkerMsg::Progress(p)); };
            let mut pipeline = vsg::core::pipeline::JobPipeline::new(cfg_map.clone(), log_cb, prog_cb);
            let mut all = Vec::new(); let total = jobs_map.len();

            for (i, j) in jobs_map.into_iter().enumerate() {
                let ref_file = j.get("ref").and_then(|v|v.as_str()).unwrap().to_string();
                let sec_file = j.get("sec").and_then(|v|v.as_str()).map(|s|s.to_string());
                let ter_file = j.get("ter").and_then(|v|v.as_str()).map(|s|s.to_string());
                let manual_layout = j.get("manual_layout").cloned();

                let _ = tx.send(WorkerMsg::Status(format!("Processing {}/{}: {}", i+1, total, Path::new(&ref_file).file_name().unwrap().to_string_lossy())));
                let res = pipeline.run_job(
                    &ref_file, sec_file.as_deref(), ter_file.as_deref(),
                                           and_merge,
                                           &cfg_map.get("output_folder").and_then(|v|v.as_str()).unwrap_or("sync_output").to_string(),
                                           manual_layout
                );
                let _=tx.send(WorkerMsg::FinishedJob(res.clone())); all.push(res);
            }
            let _=tx.send(WorkerMsg::FinishedAll(all));
            ()
        }, |_| Command::none())
    }
}
