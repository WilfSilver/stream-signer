//! This provides an easy interface to iterate over the [Frame]s in a pipeline
//! with a state stored throughout
//!
//! This basically a cut down version of [vid_frame_iter](https://github.com/Farmadupe/vid_dup_finder_lib/blob/main/vid_frame_iter)

use std::{iter::FusedIterator, sync::Arc};

use gst::{CoreError, MessageView};

use super::{
    manager::{into_glib_error, PipeState},
    Frame,
};

/// A simple iterator to extract every frame in a given pipeline.
///
/// The `state` can then be used to interact with the pipe itelf
#[derive(Debug)]
pub struct FrameIter<VC> {
    /// The state of the iterator, storing key information about the
    /// pipeline
    pub state: Arc<PipeState<VC>>,

    // The amount of time to wait for a frame before assuming there are
    // none left.
    pub timeout: gst::ClockTime,

    /// Whether the last frame has been returned
    pub fused: bool,
}

pub type SampleWithState<VC> = (Arc<PipeState<VC>>, Result<Frame, glib::Error>);

impl<VC> FrameIter<VC> {
    pub fn new<S: ToString>(
        pipeline: gst::Pipeline,
        sink: S,
        offset: Option<f64>,
        context: VC,
    ) -> Result<Self, glib::Error> {
        Ok(Self {
            state: Arc::new(PipeState::new(pipeline, sink, offset, context)?),
            timeout: 30 * gst::ClockTime::SECOND,
            fused: false,
        })
    }

    /// This returns an iterator with the current state information,
    /// usually this is for making it easier to create a managed stream from
    pub fn zip_state(self) -> impl Iterator<Item = SampleWithState<VC>> {
        let state = self.state.clone();
        std::iter::repeat(state).zip(self)
    }

    /// Sets the pipeline to the [gst::State::Paused] state
    pub fn pause(&self) -> Result<(), glib::Error> {
        self.state.pause()
    }

    /// Sets the pipeline to the [gst::State::Playing] state
    pub fn play(&self) -> Result<(), glib::Error> {
        self.state.play()
    }

    /// Seek to the given position in the file, passing the 'accurate' flag to gstreamer.
    /// If you want to make large jumps in a video file this may be faster than setting a
    /// very low framerate (because with a low framerate, gstreamer still decodes every frame).
    pub fn seek_accurate(&self, time: f64) -> Result<(), glib::Error> {
        self.state.seek_accurate(time)
    }

    fn try_find_error(bus: &gst::Bus) -> Option<glib::Error> {
        bus.pop_filtered(&[gst::MessageType::Error, gst::MessageType::Warning])
            .filter(|msg| matches!(msg.view(), MessageView::Error(_) | MessageView::Warning(_)))
            .map(into_glib_error)
    }
}

impl<VC> FusedIterator for FrameIter<VC> {}
impl<VC> Iterator for FrameIter<VC> {
    type Item = Result<Frame, glib::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        // Required for FusedIterator
        if self.fused {
            return None;
        }

        let bus = self.state.bus();

        // Get access to the appsink element.
        let appsink = self.state.get_sink();

        // If any error/warning occurred, then return it now.
        if let Some(error) = Self::try_find_error(&bus) {
            return Some(Err(error));
        }

        let sample = appsink.try_pull_sample(self.timeout);
        match sample {
            //If a frame was extracted then return it.
            Some(sample) => Some(Ok(sample.into())),

            None => {
                // Make sure no more frames can be drawn if next is called again
                self.fused = true;

                //if no sample was returned then we might have hit the timeout.
                //If so check for any possible error being written into the log
                //at that time
                let ret = match Self::try_find_error(&bus) {
                    Some(error) => Some(Err(error)),
                    _ => {
                        if !appsink.is_eos() {
                            Some(Err(glib::Error::new(
                                CoreError::TooLazy,
                                "Gstreamer timed out",
                            )))

                        // Otherwise we hit EOS and nothing else suspicious happened
                        } else {
                            None
                        }
                    }
                };

                if let Err(e) = self.state.close() {
                    panic!("{e:?}");
                }

                ret
            }
        }
    }
}
