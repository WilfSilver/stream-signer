use std::collections::VecDeque;

use gst::{Buffer, BufferRef};
use gst_audio::AudioInfo;

use crate::video::Framerate;

use super::AudioSlice;

/// This is built to cache the audio [gst::Sample]s, extracting their
/// information.
///
/// This then can be used to produce an [AudioSlice] which can be seen as a
/// slice over the different audio buffers which relate to the set frame.
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
    rel_start_ns: u64,
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
    pub fn get_timestamp(&self) -> u64 {
        let timestamp = self
            .buffer
            .front()
            .map(Buffer::as_ref)
            .and_then(BufferRef::pts)
            .unwrap_or_default();
        timestamp.nseconds()
    }

    /// Returns the timestamp in nanoseconds that the next [gst::Sample] is
    /// expected to be at (or when this object ends)
    pub fn get_end_timestamp(&self) -> u64 {
        match self.buffer.back() {
            Some(buf) => {
                let timestamp = buf.pts().unwrap_or_default();
                timestamp.nseconds() + self.buffer_ns_length(buf)
            }
            None => 0,
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

        let fps: Framerate<f64> = rate.into();
        let frame_length = (fps.convert_to_ms(1) * 1_000_000.) as u64;

        let buffer_length = self.buffer_ns_length(&self.buffer[0]);
        let required_length = (self.rel_start_ns + frame_length).div_ceil(buffer_length) as usize;
        let rel_end_ns = (self.rel_start_ns + frame_length) % buffer_length;

        if self.buffer.len() < required_length {
            return None;
        }

        let pop_length = required_length - 1;
        let mut buffers = Vec::new();

        for _ in 0..pop_length {
            buffers.push(self.buffer.pop_front().unwrap());
        }

        buffers.push(self.buffer[0].clone());

        let end = self.ns_to_idx(rel_end_ns);

        let res = Some(AudioSlice::new(
            buffers,
            self.info.clone().unwrap(),
            self.start_idx,
            end,
        ));

        self.start_idx = end;
        self.rel_start_ns = rel_end_ns;

        res
    }

    /// Converts relative nanoseconds to an index within the [Buffer]'s memory'
    ///
    /// NOTE: Assumes that [Self::info] is fine to unwrap
    #[inline]
    fn ns_to_idx(&self, time: u64) -> usize {
        self.channels().unwrap()
            * (time as f64 * self.info.as_ref().unwrap().rate() as f64 / 1_000_000_000.) as usize
    }

    /// Returns the nanosecond length for a Buffer, taking into account the
    /// number of channels, rate and size of the buffer.
    ///
    /// NOTE: Assumes that [Self::info] is fine to unwrap
    #[inline]
    fn buffer_ns_length(&self, buf: &Buffer) -> u64 {
        ((buf.map_readable().unwrap().len() / self.channels().unwrap()) as f64
            * rate_to_ns(self.info.as_ref().unwrap().rate())) as u64
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
            rel_start_ns: 0,
            buffer: VecDeque::from_iter([buffer]),
            info,
        }
    }
}

/// Converts the given rate to nanoseconds per level
#[inline]
pub(crate) const fn rate_to_ns(rate: u32) -> f64 {
    1_000_000_000. / rate as f64
}
