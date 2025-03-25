//! This provides an wrapper around [gst_video::VideoFrame] to make it easier
//! to just get the buffer of the frame and it's related information

use gst_video::VideoFrameExt;

pub use image::GenericImageView;

use crate::{spec::Coord, video::FrameError};

use super::{Framerate, ImageFns};

#[derive(Debug)]
pub struct Frame(gst_video::VideoFrame<gst_video::video_frame::Readable>);

impl Frame {
    pub fn cropped_buffer<'a>(
        &'a self,
        pos: Coord,
        size: Coord,
    ) -> Result<impl Iterator<Item = u8> + 'a, FrameError> {
        // TODO: I believe this to be slightly broken and so needs heavy testing!! (because it
        // doesn't take into account the number of bytes a pixel is)
        if pos.x + size.x > self.width() || pos.y + size.y > self.height() {
            return Err(FrameError::InvalidCrop(pos, size));
        }

        let buf = self.raw_buffer();
        let start_idx = (pos.x + pos.y * self.width()) as usize;
        let end_idx = (pos.x + size.x + (pos.y + size.y) * self.width()) as usize;

        let width = self.width();
        let it = buf[start_idx..end_idx]
            .iter()
            .enumerate()
            .filter(move |(i, _)| i % width as usize >= size.x as usize)
            .map(|(_, v)| v)
            .cloned();

        Ok(it)
    }

    pub fn raw_buffer(&self) -> &[u8] {
        self.0.plane_data(0).expect("rgb frames have 1 plane")
    }

    pub fn fps(&self) -> Framerate<usize> {
        self.0.info().fps().into()
    }

    pub fn width(&self) -> u32 {
        self.0.info().width()
    }

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
