use iced::widget::{button, column, container, row, scrollable, space, text, toggler, slider, Space};
use iced::{Element, Length};
use iced_aw::Card;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::gui::theme::card_container_style;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackRow {
    pub source: String, pub id: i64, pub ttype: String, pub codec_id: String,
    pub lang: String, pub name: String,
    pub is_default: bool, pub is_forced_display: bool, pub apply_track_name: bool,
    pub convert_to_ass: bool, pub rescale: bool, pub size_multiplier: f64,
}
impl TrackRow {
    pub fn is_subs(&self) -> bool { self.ttype == "subtitles" }
    pub fn is_srt(&self) -> bool { self.codec_id.to_uppercase().contains("S_TEXT/UTF8") }
}

#[derive(Debug, Clone)]
pub enum Msg {
    CloseManual(bool),
    FinalAdd(TrackRow),
    FinalRemove(usize),
    FinalMoveUp(usize),
    FinalMoveDown(usize),
    FinalToggleDefault(usize),
    FinalToggleForced(usize),
    FinalToggleName(usize),
    FinalToggleRescale(usize),
    FinalToggleConvert(usize),
    FinalSizeChanged(usize, f64),
}

#[derive(Debug, Clone)]
pub struct JobPreview {
    pub ref_path: String,
    pub sec_path: Option<String>,
    pub ter_path: Option<String>,
    pub tracks_ref: Vec<TrackRow>,
    pub tracks_sec: Vec<TrackRow>,
    pub tracks_ter: Vec<TrackRow>,
    pub prepopulated: bool,
}

pub struct ManualView<'a> {
    pub preview: &'a JobPreview,
    pub final_list: &'a [TrackRow],
}

fn source_row(t: &TrackRow, add: impl Fn(TrackRow) -> Msg + Copy) -> Element<'static, Msg> {
    row![
        text(format!(
            "[{}-{}] {} ({}){}",
                     t.ttype.chars().next().unwrap_or('?').to_ascii_uppercase(),
                     t.id, t.codec_id, t.lang,
                     if t.name.is_empty(){ "".into() } else { format!(" '{}'", t.name) }
        )).size(14),
        Space::with_width(Length::Fill),
        iced::widget::button("Add").on_press(add(t.clone())),
    ].spacing(8).into()
}

fn final_row(i: usize, t: &TrackRow) -> Element<'static, Msg> {
    row![
        text(format!(
            "[{}] [{}-{}] {} ({}){}",
                     t.source,
                     t.ttype.chars().next().unwrap_or('?').to_ascii_uppercase(),
                     t.id, t.codec_id, t.lang,
                     if t.name.is_empty(){ "".into() } else { format!(" '{}'", t.name) }
        )),
        Space::with_width(Length::Fill),
        button("↑").on_press(Msg::FinalMoveUp(i)),
        button("↓").on_press(Msg::FinalMoveDown(i)),
        toggler("Default", t.is_default, move |_| Msg::FinalToggleDefault(i)).text_size(12),
        if t.is_subs() { toggler("Forced", t.is_forced_display, move |_| Msg::FinalToggleForced(i)).text_size(12).into() } else { Space::with_width(Length::Shrink).into() },
            toggler("Keep Name", t.apply_track_name, move |_| Msg::FinalToggleName(i)).text_size(12),
            if t.is_subs() { toggler("Rescale", t.rescale, move |_| Msg::FinalToggleRescale(i)).text_size(12).into() } else { Space::with_width(Length::Shrink).into() },
                if t.is_subs() && t.is_srt() { toggler("Convert SRT→ASS", t.convert_to_ass, move |_| Msg::FinalToggleConvert(i)).text_size(12).into() } else { Space::with_width(Length::Shrink).into() },
                    if t.is_subs() { slider(0.1..=5.0, t.size_multiplier, move |v| Msg::FinalSizeChanged(i,v)).width(Length::Fixed(120.0)).into() } else { Space::with_width(Length::Fixed(120.0)).into() },
                        button("Delete").on_press(Msg::FinalRemove(i)),
    ].spacing(6).into()
}

impl<'a> ManualView<'a> {
    pub fn view(self) -> Element<'static, Msg> {
        let mut ref_col = column![];
        for t in &self.preview.tracks_ref { ref_col = ref_col.push(source_row(t, Msg::FinalAdd)); }
        let mut sec_col = column![];
        for t in &self.preview.tracks_sec { sec_col = sec_col.push(source_row(t, Msg::FinalAdd)); }
        let mut ter_col = column![];
        for t in &self.preview.tracks_ter { ter_col = ter_col.push(source_row(t, Msg::FinalAdd)); }

        let mut final_col = column![];
        for (i, t) in self.final_list.iter().enumerate() {
            final_col = final_col.push(final_row(i, t));
        }

        let left = container(column![
            if self.preview.prepopulated {
                text("✅ Pre-populated with the layout from the previous file.").size(14)
            } else { text("") },
                container(text("Reference Tracks")).padding(4),
                             scrollable(ref_col).height(Length::Fixed(220.0)),
                             space::Space::new(Length::Shrink, 10.0),
                             container(text("Secondary Tracks")).padding(4),
                             scrollable(sec_col).height(Length::Fixed(150.0)),
                             space::Space::new(Length::Shrink, 10.0),
                             container(text("Tertiary Tracks")).padding(4),
                             scrollable(ter_col).height(Length::Fixed(150.0)),
        ]).width(Length::FillPortion(1)).style(card_container_style());

        let right = container(column![
            container(text("Final Output (Reorder with buttons)")).padding(4),
                              scrollable(final_col).height(Length::Fixed(520.0)),
        ]).width(Length::FillPortion(1)).style(card_container_style());

        let body = row![left, right].spacing(12);

        Card::new(text("Manual Track Selection"), body)
        .foot(row![
            iced::widget::button("OK").on_press(Msg::CloseManual(true)),
              iced::widget::button("Cancel").on_press(Msg::CloseManual(false)),
        ].spacing(10))
        .max_width(1100.0)
        .into()
    }
}

// Signatures
pub fn signature_loose(p: &JobPreview) -> std::collections::HashMap<String, usize> {
    let mut c = std::collections::HashMap::new();
    for (src, v) in [("REF",&p.tracks_ref),("SEC",&p.tracks_sec),("TER",&p.tracks_ter)] {
        for t in v { *c.entry(format!("{}_{}",src,t.ttype)).or_insert(0)+=1; }
    } c
}
pub fn signature_strict(p: &JobPreview) -> std::collections::HashMap<String, usize> {
    let mut c = std::collections::HashMap::new();
    for (src, v) in [("REF",&p.tracks_ref),("SEC",&p.tracks_sec),("TER",&p.tracks_ter)] {
        for t in v { *c.entry(format!("{}_{}_{}_{}",src,t.ttype,t.lang.to_lowercase(),t.codec_id.to_lowercase())).or_insert(0)+=1; }
    } c
}

// Template from final list (abstract, no IDs)
#[derive(Debug, Clone)]
pub struct TemplateRow {
    pub source: String, pub ttype: String,
    pub is_default: bool, pub is_forced_display: bool, pub apply_track_name: bool,
    pub convert_to_ass: bool, pub rescale: bool, pub size_multiplier: f64,
}
pub fn template_from_final(list: &[TrackRow]) -> Vec<TemplateRow> {
    list.iter().map(|t| TemplateRow {
        source: t.source.clone(), ttype: t.ttype.clone(),
                    is_default: t.is_default, is_forced_display: t.is_forced_display,
                    apply_track_name: t.apply_track_name, convert_to_ass: t.convert_to_ass,
                    rescale: t.rescale, size_multiplier: if t.is_subs(){t.size_multiplier}else{1.0},
    }).collect()
}
pub fn materialize_template(tpl: &[TemplateRow], p: &JobPreview) -> Vec<TrackRow> {
    use std::collections::HashMap;
    let mut pools: HashMap<(String,String), Vec<TrackRow>> = HashMap::new();
    for t in &p.tracks_ref { pools.entry(("REF".into(), t.ttype.clone())).or_default().push(t.clone()); }
    for t in &p.tracks_sec { pools.entry(("SEC".into(), t.ttype.clone())).or_default().push(t.clone()); }
    for t in &p.tracks_ter { pools.entry(("TER".into(), t.ttype.clone())).or_default().push(t.clone()); }

    let mut counters: HashMap<(String,String), usize> = HashMap::new();
    let mut out = Vec::new();
    for row in tpl {
        let k = (row.source.clone(), row.ttype.clone());
        let idx = *counters.get(&k).unwrap_or(&0);
        if let Some(vec) = pools.get_mut(&k) {
            if idx < vec.len() {
                let mut base = vec[idx].clone();
                // block SEC/TER video
                if base.ttype=="video" && (base.source=="SEC"||base.source=="TER") { counters.insert(k, idx+1); continue; }
                base.is_default = row.is_default;
                base.is_forced_display = row.is_forced_display && base.is_subs();
                base.apply_track_name = row.apply_track_name;
                base.convert_to_ass = row.convert_to_ass && base.is_srt();
                base.rescale = row.rescale && base.is_subs();
                base.size_multiplier = if base.is_subs(){ row.size_multiplier } else { 1.0 };
                out.push(base);
            }
        }
        counters.insert(k, idx+1);
    }
    normalize_defaults(&mut out);
    normalize_forced(&mut out);
    out
}
fn normalize_defaults(list: &mut [TrackRow]) {
    for kind in ["audio","subtitles"] {
        let mut first: Option<usize> = None;
        for (i,t) in list.iter_mut().enumerate() { if t.ttype==kind { if first.is_none() && t.is_default { first=Some(i); } } }
        if let Some(idx)=first {
            for (i,t) in list.iter_mut().enumerate(){ if t.ttype==kind { t.is_default = i==idx; } }
        } else {
            for (_i,t) in list.iter_mut().enumerate(){ if t.ttype==kind { t.is_default = true; break; } }
        }
    }
}
fn normalize_forced(list: &mut [TrackRow]) {
    let mut seen=false;
    for t in list.iter_mut().filter(|t| t.is_subs()) {
        if t.is_forced_display { if seen { t.is_forced_display=false; } else { seen=true; } }
    }
}

// Conversion helper: core track info -> TrackRow
pub fn from_core_list(list: Vec<serde_json::Map<String, Value>>) -> Vec<TrackRow> {
    list.into_iter().map(|t| TrackRow{
        source: t.get("source").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                         id: t.get("id").and_then(|v| v.as_i64()).unwrap_or(0),
                         ttype: t.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                         codec_id: t.get("codec_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                         lang: t.get("lang").and_then(|v| v.as_str()).unwrap_or("und").to_string(),
                         name: t.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                         is_default:false,is_forced_display:false,apply_track_name:false,
                         convert_to_ass:false,rescale:false,size_multiplier:1.0,
    }).collect()
}
