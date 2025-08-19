use crate::model::{SelectionEntry,SelectionManifest,Source};
use crate::probe::ProbeFile;

pub struct Defaults;

impl Defaults {
  // Simple rule: ref video 0 + audio 0; sec audio 0; ter all subtitles.
  pub fn select(ref_file:&str, ref_probe:&ProbeFile, sec_file:Option<&str>, sec_probe:Option<&ProbeFile>, ter_file:Option<&str>, ter_probe:Option<&ProbeFile>) -> SelectionManifest {
    let mut sel = SelectionManifest::default();
    // REF
    let mut seen_v = false;
    let mut seen_a = false;
    for (idx,t) in ref_probe.tracks.iter().enumerate() {
      if t.r#type=="video" && !seen_v {
        sel.ref_tracks.push(SelectionEntry{source:Source::REF,file_path:ref_file.to_string(),track_id:t.id,r#type:t.r#type.clone(),codec:Some(t.codec.clone()),language:t.language.clone(),name:t.name.clone(),container_index:Some(idx)});
        seen_v = true;
      }
      if t.r#type=="audio" && !seen_a {
        sel.ref_tracks.push(SelectionEntry{source:Source::REF,file_path:ref_file.to_string(),track_id:t.id,r#type:t.r#type.clone(),codec:Some(t.codec.clone()),language:t.language.clone(),name:t.name.clone(),container_index:Some(idx)});
        seen_a = true;
      }
    }
    // SEC
    if let (Some(secp), Some(secpath)) = (sec_probe, sec_file) {
      for (idx,t) in secp.tracks.iter().enumerate() {
        if t.r#type=="audio" {
          sel.sec_tracks.push(SelectionEntry{source:Source::SEC,file_path:secpath.to_string(),track_id:t.id,r#type:t.r#type.clone(),codec:Some(t.codec.clone()),language:t.language.clone(),name:t.name.clone(),container_index:Some(idx)});
          break;
        }
      }
    }
    // TER
    if let (Some(terp), Some(terpath)) = (ter_probe, ter_file) {
      for (idx,t) in terp.tracks.iter().enumerate() {
        if t.r#type=="subtitles" {
          sel.ter_tracks.push(SelectionEntry{source:Source::TER,file_path:terpath.to_string(),track_id:t.id,r#type:t.r#type.clone(),codec:Some(t.codec.clone()),language:t.language.clone(),name:t.name.clone(),container_index:Some(idx)});
        }
      }
    }
    sel
  }
}
