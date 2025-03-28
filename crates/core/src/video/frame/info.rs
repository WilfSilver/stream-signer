//! This contains the different structures that we may want to store about a
//! frame.

use std::ops::{Bound, Range, RangeBounds, Rem};

use crate::{file::Timestamp, utils::TimeRange};

use super::{Frame, Framerate};

/// An interface to make it easier interacting with the [Frame]
///
/// Note this is slightly different to [crate::video::manager::FrameState], as
/// that is more focused on being a nice interface to interact with (and also
/// we don't need the specific context for this frame)
#[derive(Debug, Clone)]
pub struct FrameInfo {
    /// Stores the frame itself, it is okay to clone the frame due to the fact
    /// [Frame::clone] does not clone the underlying object
    pub frame: Frame,
    /// The range of time that this frame is likely to be visible. For more
    /// information see [TimeRange]
    pub time: TimeRange,
    frame_idx: usize,
}

impl FrameInfo {
    pub(crate) fn new(
        frame: Frame,
        timestamp: Timestamp,
        frame_idx: usize,
        fps: Framerate<usize>,
    ) -> Self {
        Self {
            frame,
            frame_idx,
            time: TimeRange::new(timestamp, fps.seconds() as f64 / fps.frames() as f64),
        }
    }

    /// Returns the relative index of the frame in the context of the video,
    ///
    /// However, if the video has been started with an offset this will be
    /// incorrect
    pub fn idx(&self) -> usize {
        self.frame_idx
    }
}
