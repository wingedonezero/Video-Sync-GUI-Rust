use crate::error::VsgError;
use serde::{Deserialize,Serialize};
use std::fs;

#[derive(Debug,Serialize,Deserialize,Clone)]
pub struct ProbeTrack { pub id:u32, pub r#type:String, pub codec:String, pub language:Option<String>, pub name:Option<String> }

#[derive(Debug,Serialize,Deserialize,Clone)]
pub struct ProbeFile { pub tracks:Vec<ProbeTrack> }

pub fn load_probe(path:&str)->Result<ProbeFile,VsgError>{
  let data=fs::read_to_string(path)?;
  let pf:ProbeFile=serde_json::from_str(&data)?;
  Ok(pf)
}
