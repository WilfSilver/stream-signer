use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{file::Timestamp, video::Pipeline};

// TODO: Documentation

#[derive(Debug, Clone)]
pub struct SrcInfo {
    pub duration: Timestamp,
}

#[derive(Debug)]
pub struct PipeInitiator {
    pub src: Arc<Mutex<Option<SrcInfo>>>,
    pub pipe: Pipeline,
    pub offset: Timestamp,
    pub video_sink: String,
    pub audio_sink: String,
}
