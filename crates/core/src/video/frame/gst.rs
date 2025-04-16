//! This provides an wrapper around [gst_video::VideoFrame] to make it easier
//! to just get the buffer of the frame and it's related information

use gst_video::VideoFrameExt;

pub use image::GenericImageView;

use crate::{audio::AudioFrame, spec::Vec2u, video::SigOperationError};

use super::{Framerate, ImageFns};

#[derive(Debug, Clone)]
pub struct FrameWithAudio {
    pub frame: Frame,
    pub audio: AudioFrame,
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
    ) -> Result<impl Iterator<Item = u8> + 'a, SigOperationError> {
        let frame = self.frame.cropped_buffer(pos, size)?;
        Ok(frame.chain(self.audio.cropped_buffer(channels)?))
    }
}

#[derive(Debug)]
pub struct Frame(gst_video::VideoFrame<gst_video::video_frame::Readable>);

impl Frame {
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

    /// Returns the number of nanoseconds this frame should appear at
    pub fn get_timestamp(&self) -> u64 {
        let timestamp = self.0.buffer().pts().unwrap_or_default();
        timestamp.nseconds()
    }

    /// Returns the number of nanoseconds this frame should stop appearing
    /// at (i.e. the next frame)
    pub fn get_end_timestamp(&self) -> u64 {
        let timestamp = self.0.buffer().pts().unwrap_or_default();
        let fps: Framerate<f64> = self.fps().into();
        let length = fps.convert_to_ms(1) * 1000000.; // Millisecond to nanosecond
        timestamp.nseconds() + length as u64
    }

    /// Returns the raw slice of bytes of the [Frame].
    ///
    /// The format is as such: each pixel is made up of 3 bytes (red, green,
    /// blue) with the rows followed by each other.
    ///
    /// For more info see [gst_video::VideoFrame::plane_data]
    pub fn raw_buffer(&self) -> &[u8] {
        self.0.plane_data(0).expect("rgb frames have 1 plane")
    }

    /// As the framerate is calculated once we start playing the video, each
    /// frame has access to the framerate, this therefore gives access to
    /// the current [Framerate] the video is expected to be running at
    pub fn fps(&self) -> Framerate<usize> {
        self.0.info().fps().into()
    }

    /// Returns the width of the frame
    pub fn width(&self) -> u32 {
        self.0.info().width()
    }

    /// Returns the height of the frame
    pub fn height(&self) -> u32 {
        self.0.info().height()
    }
}

impl Clone for Frame {
    /// Clone this video frame. This operation is cheap because it does not clone the underlying
    /// data (it actually relies on gstreamer's refcounting mechanism)
    fn clone(&self) -> Self {
        let buffer = self.0.buffer_owned();
        let frame = gst_video::VideoFrame::from_buffer_readable(buffer, self.0.info())
            .expect("Failed to map buffer readable");
        Self(frame)
    }
}

impl From<gst::Sample> for Frame {
    fn from(sample: gst::Sample) -> Self {
        let caps = sample.caps().expect("Sample without caps");
        let info = gst_video::VideoInfo::from_caps(caps).expect("Failed to parse caps");

        let buffer = sample
            .buffer_owned()
            .expect("Failed to get buffer from appsink");

        let frame = gst_video::VideoFrame::from_buffer_readable(buffer, &info)
            .expect("Failed to map buffer readable");

        Self(frame)
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
            width: self.0.width(),
            width_stride: 3,
            height: self.0.height(),
            height_stride: self.0.plane_stride()[0] as usize,
        };

        image::FlatSamples {
            samples: data_ref,
            layout,
            color_hint: Some(image::ColorType::Rgb8),
        }
    }
}
