#[derive(Debug, Clone)]
pub enum TrackType {
    Video,
    Audio,
    Subtitles,
}

#[derive(Debug, Clone)]
pub struct Track {
    pub source: String,
    pub id: u32,
    pub track_type: TrackType,
    pub codec_id: String,
    pub lang: String,
    pub name: String,
}
