use serde::{Serialize,Deserialize};

#[derive(Debug,Serialize,Deserialize,Clone)]
pub enum Source { REF, SEC, TER }

#[derive(Debug,Serialize,Deserialize,Clone)]
pub struct SelectionEntry{
  pub source: Source,
  pub file_path: String,
  pub track_id: u32,
  pub r#type: String,
  pub codec: Option<String>,
  pub language: Option<String>,
  pub name: Option<String>,
  pub container_index: Option<usize>,
}

#[derive(Debug,Serialize,Deserialize,Clone,Default)]
pub struct SelectionManifest{
  pub ref_tracks: Vec<SelectionEntry>,
  pub sec_tracks: Vec<SelectionEntry>,
  pub ter_tracks: Vec<SelectionEntry>,
}
