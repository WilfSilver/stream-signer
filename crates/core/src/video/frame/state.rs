//! This contains the different structures that we may want to store about a
//! frame.

use std::sync::Arc;

use crate::{
    file::Timestamp,
    utils::TimeRange,
    video::{Pipeline, pipeline::SrcInfo},
};

use super::DecodedFrame;

/// An interface to make it easier interacting with the [Frame] and other
/// aspects of the video
///
/// Note this is slightly different to [crate::video::manager::FrameManager], as
/// that is more focused on internally used tools when signing or verifying
///
/// ## Examples
///
/// If you wanted to check if you are on a specific frame you can do:
///
/// ```
/// use futures::{future::BoxFuture, FutureExt};
/// use stream_signer::video::{sign::{ChunkSignerBuilder, Controller}, FrameState};
/// use testlibs::identity::TestIdentity;
///
/// struct MyController(());
///
/// impl Controller<TestIdentity> for MyController {
///     fn get_chunks(&self, state: FrameState) -> BoxFuture<Vec<ChunkSignerBuilder<TestIdentity>>> {
///         async move {
///             let frame = &state.info.frame;
///             let duration = state.video.duration;
///
///             // ... await ...
///
///             // Sign every 100 frames or on the last frame
///             if state.frame_idx() % 100 == 0 || state.is_last {
///                 vec![todo!()]
///             } else {
///                 vec![]
///             }
///         }.boxed()
///     }
/// }
/// ```
///
/// Or if instead you wanted to sign every X milliseconds you could do:
///
/// ```
/// use std::time::Duration;
//8
/// use futures::{future::BoxFuture, FutureExt};
/// use stream_signer::video::{sign::{ChunkSignerBuilder, Controller}, FrameState};
/// use testlibs::identity::TestIdentity;
///
/// struct MyController(());
///
/// impl Controller<TestIdentity> for MyController {
///     fn get_chunks(&self, state: FrameState) -> BoxFuture<Vec<ChunkSignerBuilder<TestIdentity>>> {
///         async move {
///             let frame = &state.info.frame;
///             let duration = state.video.duration;
///
///             // ... await ...
///
///             // Sign every 100 milliseconds or on the last frame
///             let time = state.time;
///             let cross_point = time.crosses_interval(Duration::from_millis(100));
///             if (!time.is_start() && cross_point.is_some()) || state.is_last {
///                 let chunk_end = cross_point.unwrap_or(state.time.end());
///                 vec![todo!()]
///             } else {
///                 vec![]
///             }
///         }.boxed()
///     }
/// }
/// ```
#[derive(Debug)]
pub struct FrameState {
    /// Stores information about the video itself
    pub video: SrcInfo,

    /// This allows you to directly interact and query the pipeline (and
    /// pause it if needed)
    pub pipe: Arc<Pipeline>,

    /// Stores both the frame as well as the audio, and can be downcast to
    /// [super::FrameWithAudio]
    pub info: DecodedFrame<true>,

    /// This is set to try if the current frame is the last frame of the video.
    /// It's useful for making sure that the last couple frames are always
    /// signed
    pub is_last: bool,

    /// The range of time that this frame is likely to be visible. For more
    /// information see [TimeRange]
    pub time: TimeRange,
    pub(crate) frame_idx: usize,

    pub start_offset: Timestamp,
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
