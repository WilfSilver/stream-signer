use gst::{Buffer, BufferRef};
use gst_audio::AudioInfo;

use crate::video::SigOperationError;

use super::buffer::rate_to_ns;

#[derive(Debug, Clone)]
pub struct AudioFrame {
    buffers: Vec<Buffer>,
    info: AudioInfo,
    start: usize,
    end: usize,
}

impl AudioFrame {
    pub(crate) fn new(buffers: Vec<Buffer>, info: AudioInfo, start: usize, end: usize) -> Self {
        Self {
            buffers,
            info,
            start,
            end,
        }
    }

    /// Returns the timestamp of the first object in milliseconds
    pub fn get_timestamp(&self) -> u64 {
        let timestamp = self
            .buffers
            .first()
            .map(Buffer::as_ref)
            .and_then(BufferRef::pts)
            .unwrap_or_default();
        timestamp.nseconds() + self.idx_to_ns(self.start)
    }

    pub fn get_end_timestamp(&self) -> u64 {
        let timestamp = self
            .buffers
            .last()
            .map(Buffer::as_ref)
            .and_then(BufferRef::pts)
            .unwrap_or_default();
        // Last timestamp + the length of time a sample lasts
        timestamp.nseconds() + self.idx_to_ns(self.end)
    }

    fn idx_to_ns(&self, idx: usize) -> u64 {
        (idx as f64 * rate_to_ns(self.info.rate())) as u64
    }

    pub fn channels(&self) -> usize {
        self.info.channels() as usize
    }

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

impl Default for AudioFrame {
    fn default() -> Self {
        Self {
            buffers: Vec::default(),
            info: AudioInfo::builder(gst_audio::AudioFormat::F32le, 1, 2)
                .build()
                .expect("Failed to build audio info"),
            start: 0,
            end: 0,
        }
    }
}
