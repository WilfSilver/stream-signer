//! This provides an easy interface to iterate over the [FrameWithAudio]s in a
//! pipeline with a state stored throughout
//!
//! This basically a heavily modified version of [vid_frame_iter](https://github.com/Farmadupe/vid_dup_finder_lib/blob/main/vid_frame_iter)

use std::{iter::FusedIterator, sync::Arc};

use gst::{CoreError, MessageView, Sample};
use gst_app::AppSink;

use crate::file::Timestamp;

use super::{
    audio::AudioBuffer, frame::FrameWithAudio, manager::PipeState, pipeline::PipeInitiator,
    utils::into_glib_error, Frame,
};

/// A simple iterator to extract every frame in a given pipeline.
///
/// The `state` can then be used to interact with the pipe itelf
///
/// Assumptions made:
/// - Frame rate is not >1000
///
/// TODO: Examples
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

    /// Caches all the audio so it create an [super::audio::AudioSlice]
    pub audio_buffer: AudioBuffer,
}

pub type SampleWithState<VC> = (Arc<PipeState<VC>>, Result<FrameWithAudio, glib::Error>);

impl<VC> FrameIter<VC> {
    pub fn new(init: PipeInitiator, context: VC) -> Result<Self, glib::Error> {
        let state = PipeState::new(init, context)?;
        Ok(Self {
            state: Arc::new(state),
            timeout: gst::ClockTime::SECOND,
            fused: false,
            audio_buffer: AudioBuffer::default(),
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
    pub fn seek_accurate(&self, time: Timestamp) -> Result<(), glib::Error> {
        self.state.seek_accurate(time)
    }

    fn try_find_error(bus: &gst::Bus) -> Option<glib::Error> {
        bus.pop_filtered(&[gst::MessageType::Error, gst::MessageType::Warning])
            .filter(|msg| matches!(msg.view(), MessageView::Error(_) | MessageView::Warning(_)))
            .map(into_glib_error)
    }

    fn get_sample_for(&mut self, sink: &AppSink) -> Option<Result<Sample, glib::Error>> {
        let bus = self.state.bus();

        let sample = sink.try_pull_sample(self.timeout);
        match sample {
            // If a frame was extracted then return it.
            Some(sample) => Some(Ok(sample)),

            None => {
                // Make sure no more frames can be drawn if next is called again
                self.fused = true;

                // If no sample was returned then we might have hit the timeout.
                // If so check for any possible error being written into the log
                // at that time
                let ret = match Self::try_find_error(&bus) {
                    Some(error) => Some(Err(error)),
                    _ => {
                        if !sink.is_eos() {
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

impl<VC> FusedIterator for FrameIter<VC> {}
impl<VC> Iterator for FrameIter<VC> {
    type Item = Result<FrameWithAudio, glib::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        // Required for FusedIterator
        if self.fused {
            return None;
        }

        // Get access to the appsink element.
        let video_sink = self.state.get_video_sink();
        let audio_sink = self.state.get_audio_sink();

        // If any error/warning occurred, then return it now.
        let bus = self.state.bus();
        if let Some(error) = Self::try_find_error(&bus) {
            return Some(Err(error));
        }

        let sample = self.get_sample_for(&video_sink);
        match sample {
            Some(Ok(sample)) => {
                let frame: Frame = sample.into();
                let end_timestamp = frame.get_end_timestamp();
                // TODO: use sink.is_eos() to pass down if last frame

                if let Some(audio_sink) = audio_sink {
                    if !audio_sink.is_eos() {
                        while self.audio_buffer.get_end_timestamp() < end_timestamp {
                            let sample = self.get_sample_for(&audio_sink);
                            match sample {
                                Some(Ok(sample)) => self.audio_buffer.add_sample(sample),
                                Some(Err(e)) => return Some(Err(e)),
                                None => return None,
                            }
                        }
                    }
                }

                let fps = frame.fps();
                Some(Ok(FrameWithAudio {
                    frame,
                    audio: self.audio_buffer.pop_next_frame(fps),
                    is_last: video_sink.is_eos(),
                }))
            }
            Some(Err(e)) => Some(Err(e)),
            None => None,
        }
    }
}
