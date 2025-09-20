#[derive(Debug, Clone)]
pub struct JobSpec {
    pub sources: std::collections::HashMap<String, std::path::PathBuf>,
    pub manual_layout: Vec<TrackSelection>,
}

#[derive(Debug, Clone)]
pub struct TrackSelection {
    pub source: String,
    pub id: u32,
    pub track_type: String,
}
