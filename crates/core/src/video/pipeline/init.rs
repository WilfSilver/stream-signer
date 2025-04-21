use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{file::Timestamp, video::Pipeline};

#[derive(Debug, Clone)]
pub struct SrcInfo {
    pub duration: Timestamp,
}

#[derive(Debug)]
pub struct PipeInitiator {
    pub src: Arc<Mutex<Option<SrcInfo>>>,
    pub pipe: Pipeline,
    pub offset: f64,
    pub video_sink: String,
    pub audio_sink: String,
}
