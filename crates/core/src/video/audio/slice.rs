use std::time::Duration;

use gst::{Buffer, BufferRef};
use gst_audio::AudioInfo;

use crate::{file::Timestamp, video::SigOperationError};

use super::buffer::rate_to_duration;

/// This is the counter part to [super::AudioBuffer], storing information such
/// that it can be seen
#[derive(Debug)]
pub struct AudioSlice {
    /// The buffers which the slice is over
    buffers: Vec<Buffer>,
    /// The information related to the audio
    pub info: AudioInfo,
    /// The index in the first buffer where the slice starts
    start: usize,
    /// The index in the last buffer where the slice ends
    end: Option<usize>,

    pts_offset: Duration,
}

impl AudioSlice {
    pub const fn new(
        buffers: Vec<Buffer>,
        info: AudioInfo,
        start: usize,
        end: Option<usize>,
        pts_offset: Duration,
    ) -> Self {
        Self {
            buffers,
            info,
            start,
            end,
            pts_offset,
        }
    }

    /// Returns the number of **bytes** this slice is for, including all the data
    pub fn len(&self) -> usize {
        let buf_len = self.per_buffer_len();

        buf_len * (self.buffers.len() - 1) + self.end.unwrap_or(buf_len) - self.start
    }

    /// Returns the expected number of bytes per channel
    #[inline]
    pub fn per_channel_bytes(&self) -> usize {
        self.len() / self.channels()
    }

    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
            || (self.buffers.len() == 1
                && self.end.unwrap_or_else(|| self.per_buffer_len()) == self.start)
    }

    #[inline]
    fn per_buffer_len(&self) -> usize {
        self.buffers[0].map_readable().unwrap().len()
    }

    /// Returns the timestamp of where this slice begins in nanoseconds, this
    /// should roughly equal [crate::video::Frame::get_timestamp] but may not
    /// be exact as it is calculated separately
    pub fn get_timestamp(&self) -> Timestamp {
        let timestamp: Timestamp = self
            .buffers
            .first()
            .map(Buffer::as_ref)
            .and_then(BufferRef::pts)
            .unwrap_or_default()
            .into();

        timestamp - self.pts_offset + self.idx_to_duration(self.start)
    }

    /// Returns the timestamp of the end of the frame in nanoseconds, this
    /// should roughly equal [crate::video::Frame::get_timestamp] but may not
    /// be exact as it is calculated separately
    pub fn get_end_timestamp(&self) -> Timestamp {
        let timestamp: Timestamp = self
            .buffers
            .last()
            .map(Buffer::as_ref)
            .and_then(BufferRef::pts)
            .unwrap_or_default()
            .into();

        let additional = match self.end {
            Some(end) => self.idx_to_duration(end),
            None => Duration::from_nanos(
                self.buffers
                    .last()
                    .map(Buffer::as_ref)
                    .and_then(BufferRef::duration)
                    .unwrap_or_default()
                    .nseconds(),
            ),
        };

        // Last timestamp + the length of time a sample lasts
        timestamp - self.pts_offset + additional
    }

    /// Conversion from the index of a buffer to the relative nanoseconds from
    /// the start of the buffer
    fn idx_to_duration(&self, idx: usize) -> Duration {
        rate_to_duration(self.info.rate())
            * (idx / (self.channels() * self.channel_byte_width())) as u32
    }

    /// Wrapper for [AudioInfo::channels]
    pub fn channels(&self) -> usize {
        self.info.channels() as usize
    }

    pub fn channel_byte_width(&self) -> usize {
        self.info.format_info().width() as usize / 8
    }

    /// Returns an iterator over the bytes relating to a specific channel
    /// within this slice
    ///
    /// This is because the audio buffer is organised as follows:
    ///
    /// ```txt
    /// [L0, R0, L1, R1, ...]
    /// ```
    ///
    /// And for signing they need to be organised as follows:
    ///
    /// ```txt
    /// [L0, L1, ..., R0, R1, ...]
    /// ```
    ///
    /// Which is done by [Self::cropped_buffer]
    ///
    fn unchecked_channel_buffer(&self, i: usize) -> impl Iterator<Item = u8> + '_ {
        let last_idx = self.buffers.len() - 1;
        self.buffers.iter().enumerate().flat_map(move |(j, b)| {
            let mem = b.map_readable().expect("Buffer should have member");
            let start = if j == 0 { self.start } else { 0 };
            let end = if j == last_idx {
                self.end.unwrap_or_else(|| mem.len())
            } else {
                mem.len()
            };
            let slice = mem.as_slice();

            let width = self.channel_byte_width();

            let channel_offset = i * width;
            (start + channel_offset..end + channel_offset)
                .step_by(self.channels() * width)
                .flat_map(|k| &slice[k..k + width])
                .cloned()
                .collect::<Vec<_>>()
        })
    }

    /// This returns an iterator over the bytes needed to sign for a given
    /// `channels` configuration.
    ///
    /// The bytes will be grouped by channel, and only channels that are
    /// requested are included, if [None] is given, we return all channels
    ///
    /// So for example, if `channels` is `vec![1, 0]`, we will return an
    /// iterator that looks as follows:
    ///
    /// ```txt
    /// [C1_0, C1_1, C1_2, ..., C0_0, C0_1, C0_2, ...]
    /// ```
    ///
    /// Note that the order of the channels is the same order as the given
    /// vector
    ///
    /// * `dest` is the destination slice for the bytes and should be of size
    ///   [Self::cropped_buffer_size]. This is done for efficiency
    pub fn cropped_buffer(
        &self,
        dest: &mut [u8],
        channels: &[usize],
    ) -> Result<usize, SigOperationError> {
        let max = self.channels();
        let mut invalid_channels = channels.iter().filter(|x| **x >= max).peekable();

        if invalid_channels.peek().is_some() {
            return Err(SigOperationError::InvalidChannels(
                invalid_channels.cloned().collect::<Vec<_>>(),
            ));
        }

        let mut offset = 0;
        for c in channels {
            for b in self.unchecked_channel_buffer(*c) {
                dest[offset] = b;
                offset += 1;
            }
        }

        Ok(offset)
    }

    /// Predicts the size of the buffer required to call [Self::cropped_buffer]
    #[inline]
    pub fn cropped_buffer_size(&self, channels: &[usize]) -> usize {
        self.per_channel_bytes() * channels.len()
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use testlibs::{test_video, videos};

    use crate::{
        SignPipeline,
        video::{Frame, audio::AudioBuffer, manager::PipeState},
    };

    use super::*;

    fn get_first_slice(video: &str) -> Result<AudioSlice, Box<dyn Error>> {
        crate::gst::init()?;

        let filepath = test_video(video);

        let init = SignPipeline::build_from_path(&filepath)
            .unwrap()
            .build_raw_pipeline()?;

        let state = PipeState::new(init, ())?;
        state.play()?;

        let mut audio_buffer = AudioBuffer::default();

        let frame = Frame::new_first(
            state
                .get_video_sink()
                .try_pull_sample(gst::ClockTime::SECOND)
                .unwrap(),
        );

        let end_timestamp = frame.get_end_timestamp();

        let audio_sink = state.get_audio_sink();
        if let Some(audio_sink) = audio_sink {
            if !audio_sink.is_eos() {
                while audio_buffer.get_end_timestamp() < end_timestamp {
                    let sample = audio_sink.try_pull_sample(gst::ClockTime::MSECOND);
                    match sample {
                        Some(sample) => audio_buffer.add_sample(sample),
                        None => break, // Reached end of video
                    }
                }
            }
        }

        // This also checks that we will always get a audio slice for all
        // frames
        Ok(audio_buffer.pop_until(end_timestamp).unwrap())
    }

    #[test]
    fn correct_crop() -> Result<(), Box<dyn Error>> {
        let slice = get_first_slice(videos::BIG_BUNNY_AUDIO)?;

        let channels = [0, 1];
        let mut full_crop = vec![0; slice.cropped_buffer_size(&channels)];

        let offset = slice
            .cropped_buffer(&mut full_crop, &channels)
            .expect("Should be able to create crop");
        assert_eq!(offset, full_crop.len(), "Predicted buffer size correct");

        let expected_length = slice.buffers[0].map_readable().unwrap().len() + slice.end.unwrap();
        assert_eq!(
            full_crop.len(),
            expected_length,
            "Full crop matches the expected length"
        );

        for c in 0..1 {
            let channels = [c];
            let mut channel_crop = vec![0; slice.cropped_buffer_size(&channels)];
            let offset = slice
                .cropped_buffer(&mut channel_crop, &[c])
                .expect("Should be able to create crop");

            assert_eq!(offset, channel_crop.len(), "Predicted buffer size correct");

            assert_eq!(
                channel_crop.len(),
                expected_length / 2, // 2 is the expected number of channels
                "Full crop matches the expected length"
            );
        }

        Ok(())
    }

    #[test]
    fn invalid_crop() -> Result<(), Box<dyn Error>> {
        let slice = get_first_slice(videos::BIG_BUNNY_AUDIO)?;

        let channels = [0, 3, 8];
        let mut crop = vec![0; slice.cropped_buffer_size(&channels)];

        let offset = slice.cropped_buffer(&mut crop, &channels);

        assert!(
            matches!(
                offset,
                Err(SigOperationError::InvalidChannels(channels))
                    if channels == vec![3, 8]),
            "Found error when trying to crop with invalid buffer"
        );

        Ok(())
    }
}
