use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{file::Timestamp, video::Pipeline};

/// This stores the data which can be gathered once the pipeline has started
/// and is here just to provide a bit of easier access to the context of
/// the video.
#[derive(Debug, Clone)]
pub struct SrcInfo {
    /// The duration of the video (as a [Timestamp])
    pub duration: Timestamp,
}

/// The data which is needed to create a [crate::video::iter::FrameIter] and
/// [crate::video::manager::PipeState]
#[derive(Debug)]
pub struct PipeInitiator {
    /// Access to the [SrcInfo] which is expected to be set once the video
    /// starts playing/when the src pad has been linked
    pub src: Arc<Mutex<Option<SrcInfo>>>,
    /// The pipeline itself
    pub pipe: Pipeline,
    /// The starting offset position of the video
    pub offset: Timestamp,
    /// The name of the video sink so we can query it later on
    pub video_sink: String,
    /// The name of the audio sink so we can query it later on
    pub audio_sink: String,
}
