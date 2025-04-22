use std::time::Duration;

use gst::{Buffer, BufferRef};
use gst_audio::AudioInfo;

use crate::{file::Timestamp, video::SigOperationError};

use super::buffer::rate_to_duration;

/// This is the counter part to [super::AudioBuffer], storing information such
/// that it can be seen
#[derive(Debug, Clone)]
pub struct AudioSlice {
    /// The buffers which the slice is over
    buffers: Vec<Buffer>,
    /// The information related to the audio
    pub info: AudioInfo,
    /// The index in the first buffer where the slice starts
    start: usize,
    /// The index in the last buffer where the slice ends
    end: usize,
}

impl AudioSlice {
    pub const fn new(buffers: Vec<Buffer>, info: AudioInfo, start: usize, end: usize) -> Self {
        Self {
            buffers,
            info,
            start,
            end,
        }
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
        timestamp + self.idx_to_duration(self.start)
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

        // Last timestamp + the length of time a sample lasts
        timestamp + self.idx_to_duration(self.end)
    }

    /// Conversion from the index of a buffer to the relative nanoseconds from
    /// the start of the buffer
    fn idx_to_duration(&self, idx: usize) -> Duration {
        rate_to_duration(self.info.rate()) * idx as u32
    }

    /// Wrapper for [AudioInfo::channels]
    pub fn channels(&self) -> usize {
        self.info.channels() as usize
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
            let end = if j == last_idx { self.end } else { mem.len() };
            let slice = mem.as_slice();

            (start..end)
                .step_by(self.channels())
                .map(|k| slice[k + i])
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
    pub fn cropped_buffer<'a>(
        &'a self,
        channels: &'a Option<Vec<usize>>,
    ) -> Result<Box<dyn Iterator<Item = u8> + 'a>, SigOperationError> {
        Ok(match channels {
            Some(limit) => {
                let max = self.channels();
                let mut invalid_channels = limit.iter().filter(|x| **x >= max).peekable();

                if invalid_channels.peek().is_some() {
                    return Err(SigOperationError::InvalidChannels(
                        invalid_channels.cloned().collect::<Vec<_>>(),
                    ));
                }

                Box::new(limit.iter().flat_map(|i| self.unchecked_channel_buffer(*i)))
            }
            None => Box::new((0..self.channels()).flat_map(|i| self.unchecked_channel_buffer(i))),
        })
    }
}
