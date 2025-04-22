//! This contains the different structures that we may want to store about a
//! frame.

use crate::{
    utils::TimeRange,
    video::{audio::AudioSlice, pipeline::SrcInfo, Pipeline},
};

use super::Frame;

/// An interface to make it easier interacting with the [Frame] and other
/// aspects of the video (without having the bloated contexts)
///
/// Note this is slightly different to [crate::video::manager::FrameManager], as
/// that is more focused on being a nice interface to interact with (and also
/// we don't need the specific context for this frame)
///
/// TODO: Examples
#[derive(Debug, Clone)]
pub struct FrameState {
    /// Stores information about the video itself
    pub video: SrcInfo,

    /// This allows you to directly interact and query the pipeline (and
    /// pause it if needed)
    pub pipe: Pipeline,

    /// Stores the frame itself, it is okay to clone the frame due to the fact
    /// [Frame::clone] does not clone the underlying object
    pub frame: Frame,

    pub audio: Option<AudioSlice>,

    /// The range of time that this frame is likely to be visible. For more
    /// information see [TimeRange]
    pub time: TimeRange,
    pub(crate) frame_idx: usize,
}

impl FrameState {
    /// Returns the relative index of the frame in the context of the video,
    ///
    /// However, if the video has been started with an offset this will be
    /// incorrect
    pub fn frame_idx(&self) -> usize {
        self.frame_idx
    }
}
