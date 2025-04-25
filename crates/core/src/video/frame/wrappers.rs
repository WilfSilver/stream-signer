//! This provides an wrapper around [gst_video::VideoFrame] to make it easier
//! to just get the buffer of the frame and it's related information

use std::time::Duration;

use gst_video::VideoFrameExt;

pub use image::GenericImageView;

use crate::{
    file::Timestamp,
    spec::Vec2u,
    video::{audio::AudioSlice, SigOperationError},
};

use super::{Framerate, ImageFns};

/// This is a basic structure used internally to store both the frame and audio
/// in one place as well as some additional context.
#[derive(Debug, Clone)]
pub struct FrameWithAudio {
    /// The wrapper for the [gst_video::VideoFrame] to get access it its
    /// information
    pub frame: Frame,
    /// If there is audio in the video, this will store all the data for the
    /// audio which is played during the frame.
    pub audio: Option<AudioSlice>,
    /// Is set if this is the last Frame in the video
    pub is_last: bool,
}

impl FrameWithAudio {
    /// This returns the bytes that should be signed, including the audio
    /// channels
    ///
    /// It will also do the checking if the given crop is within the bounds of
    /// the image, if it is not it will return a [SigOperationError::InvalidCrop] or
    /// [SigOperationError::InvalidChannels]
    pub fn cropped_buffer<'a>(
        &'a self,
        pos: Vec2u,
        size: Vec2u,
        channels: &'a Option<Vec<usize>>,
    ) -> Result<Box<dyn Iterator<Item = u8> + 'a>, SigOperationError> {
        let frame = self.frame.cropped_buffer(pos, size)?;
        Ok(match &self.audio {
            Some(audio) => Box::new(frame.chain(audio.cropped_buffer(channels)?)),
            None => Box::new(frame),
        })
    }
}

/// A simple wrapper around [gst_video::VideoFrame] to provide some additional
/// functions used within signing and some nice utilities
#[derive(Debug)]
pub struct Frame {
    /// Stores the raw frame information from gstreamer
    raw: gst_video::VideoFrame<gst_video::video_frame::Readable>,
    /// Stores a cache of the actual pts, removing the start offset
    pts: Timestamp,
}

impl Frame {
    pub fn new(sample: gst::Sample, pts_offset: Duration) -> Self {
        let caps = sample.caps().expect("Sample without caps");
        let info = gst_video::VideoInfo::from_caps(caps).expect("Failed to parse caps");

        let buffer = sample
            .buffer_owned()
            .expect("Failed to get buffer from appsink");

        let pts: Timestamp = buffer.pts().unwrap_or_default().into();

        let frame = gst_video::VideoFrame::from_buffer_readable(buffer, &info)
            .expect("Failed to map buffer readable");

        Self {
            raw: frame,
            pts: pts - pts_offset,
        }
    }

    /// This can be used when it is known this is the first frame and therefore
    /// the timestamp should be 0
    pub fn new_first(sample: gst::Sample) -> Self {
        let caps = sample.caps().expect("Sample without caps");
        let info = gst_video::VideoInfo::from_caps(caps).expect("Failed to parse caps");

        let buffer = sample
            .buffer_owned()
            .expect("Failed to get buffer from appsink");

        let frame = gst_video::VideoFrame::from_buffer_readable(buffer, &info)
            .expect("Failed to map buffer readable");

        Self {
            raw: frame,
            pts: Timestamp::ZERO,
        }
    }

    /// This returns the bytes which are within the given bounds determined by
    /// `pos` and `size`
    ///
    /// It will also do the checking if the given crop is within the bounds of
    /// the image, if it is not it will return a [SigOperationError::InvalidCrop]
    pub fn cropped_buffer(
        &self,
        pos: Vec2u,
        size: Vec2u,
    ) -> Result<impl Iterator<Item = u8> + '_, SigOperationError> {
        if pos.x + size.x > self.width()
            || pos.y + size.y > self.height()
            || size.x == 0 // Means we are signing nothing and there's no point in that
            || size.y == 0
        {
            return Err(SigOperationError::InvalidCrop(pos, size));
        }

        let buf = self.raw_buffer();
        const DEPTH: usize = 3;
        let start_idx = DEPTH * (pos.x + pos.y * self.width()) as usize;
        // This ends on the first pixel outside the value (which is why we don't add `size.x`)
        let end_idx = DEPTH * (pos.x + (pos.y + size.y) * self.width()) as usize;

        let row_size = size.x as usize * 3;

        let width = self.width() as usize;
        let it = buf[start_idx..end_idx]
            .iter()
            .enumerate()
            .filter(move |(i, _)| i % width >= row_size)
            .map(|(_, v)| v)
            .cloned();

        Ok(it)
    }

    /// Returns the number of nanoseconds this frame should appear at, also
    /// known as the presentation timestamp
    #[inline]
    pub const fn get_timestamp(&self) -> Timestamp {
        self.pts
    }

    #[inline]
    pub fn get_duration(&self) -> Duration {
        let timestamp = self.raw.buffer().duration().unwrap_or_default();
        Duration::from_nanos(timestamp.nseconds())
    }

    /// Returns the number of nanoseconds this frame should stop appearing
    /// at (i.e. the next frame)
    #[inline]
    pub fn get_end_timestamp(&self) -> Timestamp {
        self.get_timestamp() + self.fps().frame_time()
    }

    /// Returns the raw slice of bytes of the [Frame].
    ///
    /// The format is as such: each pixel is made up of 3 bytes (red, green,
    /// blue) with the rows followed by each other.
    ///
    /// For more info see [gst_video::VideoFrame::plane_data]
    #[inline]
    pub fn raw_buffer(&self) -> &[u8] {
        self.raw.plane_data(0).expect("rgb frames have 1 plane")
    }

    /// As the framerate is calculated once we start playing the video, each
    /// frame has access to the framerate, this therefore gives access to
    /// the current [Framerate] the video is expected to be running at
    #[inline]
    pub fn fps(&self) -> Framerate<usize> {
        self.raw.info().fps().into()
    }

    /// Returns the width of the frame
    #[inline]
    pub fn width(&self) -> u32 {
        self.raw.info().width()
    }

    /// Returns the height of the frame
    #[inline]
    pub fn height(&self) -> u32 {
        self.raw.info().height()
    }

    /// Returns the information associated with the video
    #[inline]
    pub fn info(&self) -> &gst_video::VideoInfo {
        self.raw.info()
    }
}

impl Clone for Frame {
    /// Clone this video frame. This operation is cheap because it does not clone the underlying
    /// data (it actually relies on gstreamer's refcounting mechanism)
    fn clone(&self) -> Self {
        let buffer = self.raw.buffer_owned();
        let frame = gst_video::VideoFrame::from_buffer_readable(buffer, self.raw.info())
            .expect("Failed to map buffer readable");
        Self {
            raw: frame,
            pts: self.pts,
        }
    }
}

impl GenericImageView for Frame {
    type Pixel = image::Rgb<u8>;

    fn dimensions(&self) -> (u32, u32) {
        (self.width(), self.height())
    }

    fn get_pixel(&self, x: u32, y: u32) -> Self::Pixel {
        self.as_flat()
            .as_view::<image::Rgb<u8>>()
            .expect("unreachable")
            .get_pixel(x, y)
    }
}

impl ImageFns for Frame {
    type IB = image::RgbImage;

    fn as_flat(&self) -> image::FlatSamples<&[u8]> {
        // Safety: gstreamer guarantees that this pointer exists and does not move for the life of self)
        let data_ref: &[u8] = self.raw_buffer();
        let layout = image::flat::SampleLayout {
            channels: 3,
            channel_stride: 1,
            width: self.raw.width(),
            width_stride: 3,
            height: self.raw.height(),
            height_stride: self.raw.plane_stride()[0] as usize,
        };

        image::FlatSamples {
            samples: data_ref,
            layout,
            color_hint: Some(image::ColorType::Rgb8),
        }
    }
}
