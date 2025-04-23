use std::{collections::VecDeque, time::Duration};

use gst::{Buffer, BufferRef};
use gst_audio::AudioInfo;

use crate::{
    file::Timestamp,
    time::{ONE_MILLI_NANOS, ONE_SECOND_MILLIS},
    video::Framerate,
};

use super::AudioSlice;

/// This is built to cache the audio [gst::Sample]s, extracting their
/// information.
///
/// This then can be used to produce an [AudioSlice] which can be seen as a
/// slice over the different audio buffers which relate to the set frame.
///
/// ```no_run
/// # use std::error::Error;
/// # fn main() -> Result<(), Box<dyn Error>> {
/// use stream_signer::{
///     video::{
///         audio::{AudioBuffer, AudioSlice},
///         manager::PipeState,
///         Frame,
///     },
///     SignPipeline,
/// };
/// # use testlibs::{test_video, videos};
/// # let my_video_with_audio = test_video(videos::BIG_BUNNY_LONG);
///
/// let init = SignPipeline::build_from_path(&my_video_with_audio)
///     .unwrap()
///     .build_raw_pipeline()?;
///
/// let state = PipeState::new(init, ())?;
/// let mut audio_buffer = AudioBuffer::default();
///
/// let sample = state.get_video_sink().try_pull_sample(gst::ClockTime::SECOND).unwrap();
/// let frame: Frame = sample.into();
/// let end_timestamp = frame.get_end_timestamp();
///
/// let audio_sink = state.get_audio_sink();
/// if let Some(audio_sink) = audio_sink {
///     if !audio_sink.is_eos() {
///         while audio_buffer.get_end_timestamp() < end_timestamp {
///             let sample = audio_sink.try_pull_sample(gst::ClockTime::SECOND);
///             match sample {
///                 Some(sample) => audio_buffer.add_sample(sample),
///                 None => break, // Reached end of video
///             }
///         }
///     }
/// }
///
/// let audio_slice: Option<AudioSlice> = audio_buffer.pop_next_frame(frame.fps());
///
/// assert!(audio_slice.is_some());
///
/// let audio_slice = audio_slice.unwrap();
///
/// // The timestamp range is equal to the framerate (in this case it is 30fps)
/// assert_eq!(audio_slice.get_timestamp().as_millis(), 0);
/// assert_eq!(audio_slice.get_end_timestamp().as_millis(), 33);
///
/// // ...
/// #
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Default, Clone)]
pub struct AudioBuffer {
    /// The raw vector of buffers
    pub buffer: VecDeque<Buffer>,
    /// The information about the audio collected from the last sample.
    ///
    /// NOTE: This can be assumed to be [Some] if [Self::buffer] length is > 0
    info: Option<AudioInfo>,

    /// The index of the next frame's starting position (also can be seen
    /// as where the last frame extracted with [Self::pop_next_frame]
    /// ended)
    start_idx: usize,
    /// Cache of the [Self::start_idx] in nanoseconds
    rel_start: Duration,
}

impl AudioBuffer {
    /// Adds the given [gst::Sample] to the buffers.
    ///
    /// NOTE: This assumes that the given Sample is an audio sample (and so
    /// [AudioInfo] can be extracted from it)
    pub fn add_sample(&mut self, sample: gst::Sample) {
        let buffer = sample
            .buffer_owned()
            .expect("Failed to get buffer from appsink");

        self.buffer.push_back(buffer);

        if self.info.is_none() {
            let caps = sample.caps().expect("Sample without caps");
            self.info = Some(AudioInfo::from_caps(caps).expect("Failed to parse caps"));
        }
    }

    /// Returns the timestamp of the start of the buffer in nanoseconds
    pub fn get_timestamp(&self) -> Timestamp {
        let timestamp = self
            .buffer
            .front()
            .map(Buffer::as_ref)
            .and_then(BufferRef::pts)
            .unwrap_or_default();
        timestamp.into()
    }

    /// Returns the timestamp in nanoseconds that the next [gst::Sample] is
    /// expected to be at (or when this object ends)
    pub fn get_end_timestamp(&self) -> Timestamp {
        match self.buffer.back() {
            Some(buf) => {
                let timestamp: Timestamp = buf.pts().unwrap_or_default().into();
                timestamp + self.buffer_duration(buf)
            }
            None => Timestamp::ZERO,
        }
    }

    /// Returns the number of channels in the audio, so wrapper for
    /// [AudioInfo::channels]
    ///
    /// If we haven't had a sample yet, it will return [`None`]
    pub fn channels(&self) -> Option<usize> {
        self.info.as_ref().map(|i| i.channels() as usize)
    }

    /// This creates a new [AudioSlice] from the currently stored information.
    ///
    /// If there isn't enough audio data (e.g. [Self::get_end_timestamp] < [crate::video::Frame::get_end_timestamp]),
    /// [`None`] will be returned.
    pub fn pop_next_frame(&mut self, rate: Framerate<usize>) -> Option<AudioSlice> {
        if self.buffer.is_empty() {
            return None;
        }

        let frame_length = rate.frame_time() * 1_000_000;

        let buffer_length = self.buffer_duration(&self.buffer[0]);
        let required_length = (self.rel_start + frame_length)
            .as_nanos()
            .div_ceil(buffer_length.as_nanos()) as usize;
        let rel_end_ns = Duration::from_nanos(
            ((self.rel_start + frame_length).as_nanos() % buffer_length.as_nanos()) as u64,
        );

        if self.buffer.len() < required_length {
            return None;
        }

        let pop_length = required_length - 1;
        let mut buffers = Vec::new();

        for _ in 0..pop_length {
            buffers.push(self.buffer.pop_front().unwrap());
        }

        buffers.push(self.buffer[0].clone());

        let end = self.duration_to_idx(rel_end_ns);

        let res = Some(AudioSlice::new(
            buffers,
            self.info.clone().unwrap(),
            self.start_idx,
            end,
        ));

        self.start_idx = end;
        self.rel_start = rel_end_ns;

        res
    }

    /// Converts relative nanoseconds to an index within the [Buffer]'s memory'
    ///
    /// NOTE: Assumes that [Self::info] is fine to unwrap
    #[inline]
    fn duration_to_idx(&self, time: Duration) -> usize {
        self.channels().unwrap()
            * (time.as_nanos() as f64 * self.info.as_ref().unwrap().rate() as f64
                / (ONE_MILLI_NANOS * ONE_SECOND_MILLIS) as f64) as usize
    }

    /// Returns the nanosecond length for a Buffer, taking into account the
    /// number of channels, rate and size of the buffer.
    ///
    /// NOTE: Assumes that [Self::info] is fine to unwrap
    #[inline]
    fn buffer_duration(&self, buf: &Buffer) -> Duration {
        rate_to_duration(self.info.as_ref().unwrap().rate())
            * (buf.map_readable().unwrap().len() / self.channels().unwrap()) as u32
    }
}

impl From<gst::Sample> for AudioBuffer {
    fn from(sample: gst::Sample) -> Self {
        let caps = sample.caps().expect("Sample without caps");
        let info = Some(AudioInfo::from_caps(caps).expect("Failed to parse caps"));

        let buffer = sample
            .buffer_owned()
            .expect("Failed to get buffer from appsink");

        Self {
            start_idx: 0,
            rel_start: Duration::ZERO,
            buffer: VecDeque::from_iter([buffer]),
            info,
        }
    }
}

/// Converts the given rate to nanoseconds per level
#[inline]
pub(crate) fn rate_to_duration(rate: u32) -> Duration {
    Duration::from_secs_f64(1. / rate as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        video::{manager::PipeState, Frame},
        SignPipeline,
    };

    use std::error::Error;

    use testlibs::{test_video, videos};

    #[test]
    fn timestamps_match() -> Result<(), Box<dyn Error>> {
        let filepath = test_video(videos::BIG_BUNNY_LONG);

        let init = SignPipeline::build_from_path(&filepath)
            .unwrap()
            .build_raw_pipeline()?;

        let state = PipeState::new(init, ())?;
        let mut audio_buffer = AudioBuffer::default();

        while let Some(sample) = state
            .get_video_sink()
            .try_pull_sample(gst::ClockTime::SECOND)
        {
            let frame: Frame = sample.into();
            let end_timestamp = frame.get_end_timestamp();

            let audio_sink = state.get_audio_sink();
            if let Some(audio_sink) = audio_sink {
                if !audio_sink.is_eos() {
                    while audio_buffer.get_end_timestamp() < end_timestamp {
                        let sample = audio_sink.try_pull_sample(gst::ClockTime::SECOND);
                        match sample {
                            Some(sample) => audio_buffer.add_sample(sample),
                            None => break, // Reached end of video
                        }
                    }
                }
            }

            // This also checks that we will always get a audio slice for all
            // frames
            let audio_slice = audio_buffer.pop_next_frame(frame.fps()).unwrap();

            let expected_start = frame.get_timestamp();
            let expected_end = frame.get_end_timestamp();
            assert_eq!(
                audio_slice.get_timestamp(),
                expected_start,
                "Starting timestamp is {expected_start:.2?}",
            );
            assert_eq!(
                audio_slice.get_end_timestamp(),
                expected_end,
                "Ending timestamp is {expected_end:.2?}",
            );
        }

        Ok(())
    }

    #[test]
    fn channels_detected() -> Result<(), Box<dyn Error>> {
        let filepath = test_video(videos::BIG_BUNNY_LONG);

        let init = SignPipeline::build_from_path(&filepath)
            .unwrap()
            .build_raw_pipeline()?;

        let state = PipeState::new(init, ())?;
        let mut audio_buffer = AudioBuffer::default();

        let sample = state
            .get_video_sink()
            .try_pull_sample(gst::ClockTime::SECOND)
            .unwrap();

        let frame: Frame = sample.into();
        let end_timestamp = frame.get_end_timestamp();

        let audio_sink = state.get_audio_sink();
        if let Some(audio_sink) = audio_sink {
            if !audio_sink.is_eos() {
                while audio_buffer.get_end_timestamp() < end_timestamp {
                    let sample = audio_sink.try_pull_sample(gst::ClockTime::SECOND);
                    match sample {
                        Some(sample) => audio_buffer.add_sample(sample),
                        None => break, // Reached end of video
                    }
                }
            }
        }

        let audio_slice = audio_buffer.pop_next_frame(frame.fps()).unwrap();

        assert_eq!(audio_slice.channels(), 2, "Detected 2 channels");

        Ok(())
    }
}
