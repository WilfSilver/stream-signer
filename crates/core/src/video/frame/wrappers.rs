//! This provides an wrapper around [gst_video::VideoFrame] to make it easier
//! to just get the buffer of the frame and it's related information

use std::{ops::Deref, sync::Arc, time::Duration};

use gst_video::VideoFrameExt;

pub use image::GenericImageView;

use crate::{
    file::Timestamp,
    spec::Vec2u,
    video::{SigOperationError, StreamError, audio::AudioSlice},
};

use super::{Framerate, ImageFns};

/// A utility struct to help dealing with Arced FrameWithAudio to allow us to
/// not to clone it while also decoding it without moving out of the [Arc]
#[derive(Debug, Clone)]
pub struct DecodedFrame<const OK: bool>(Arc<Result<FrameWithAudio, StreamError>>);

impl From<Result<FrameWithAudio, StreamError>> for DecodedFrame<false> {
    fn from(value: Result<FrameWithAudio, StreamError>) -> Self {
        Self(Arc::new(value))
    }
}

impl From<Arc<Result<FrameWithAudio, StreamError>>> for DecodedFrame<false> {
    fn from(value: Arc<Result<FrameWithAudio, StreamError>>) -> Self {
        Self(value)
    }
}

impl DecodedFrame<false> {
    pub fn check(self) -> Result<DecodedFrame<true>, StreamError> {
        if let Err(e) = &self.0.as_ref() {
            Err(e.clone())
        } else {
            Ok(DecodedFrame::<true>(self.0))
        }
    }

    pub fn check_ref(&self) -> Result<&DecodedFrame<true>, StreamError> {
        if let Err(e) = &self.0.as_ref() {
            Err(e.clone())
        } else {
            Ok(unsafe { &*(self as *const DecodedFrame<false> as *const DecodedFrame<true>) })
        }
    }
}

impl Deref for DecodedFrame<false> {
    type Target = Result<FrameWithAudio, StreamError>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for DecodedFrame<true> {
    type Target = FrameWithAudio;

    fn deref(&self) -> &Self::Target {
        unsafe { self.0.as_ref().as_ref().unwrap_unchecked() }
    }
}

/// This is a basic structure used internally to store both the frame and audio
/// in one place as well as some additional context.
#[derive(Debug)]
pub struct FrameWithAudio {
    /// The index in which the frame appeared, please note that this may be
    /// inaccurate to the video itself due to having a start offset,
    /// [Frame::get_timestamp] should be used instead
    pub idx: usize,
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
    pub fn cropped_buffer(
        &self,
        dest: &mut [u8],
        pos: Vec2u,
        size: Vec2u,
        channels: &[usize],
    ) -> Result<usize, SigOperationError> {
        let mut offset = self.frame.cropped_buffer(dest, pos, size)?;

        if let Some(audio) = &self.audio {
            offset += audio.cropped_buffer(dest, channels)?;
        }

        Ok(offset)
    }

    /// Predicts the size of `dest` param needs to be to call [Self::cropped_buffer]
    #[inline]
    pub fn cropped_buffer_size(&self, size: Vec2u, channels: &[usize]) -> usize {
        self.frame.cropped_buffer_size(size)
            + self
                .audio
                .as_ref()
                .map(|a| a.cropped_buffer_size(channels))
                .unwrap_or_default()
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
    const DEPTH: usize = 3;

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

    /// Predicts the number of bytes the required length of the input for
    /// [Self::cropped_buffer].
    #[inline]
    pub const fn cropped_buffer_size(&self, size: Vec2u) -> usize {
        Self::DEPTH * size.x as usize * size.y as usize
    }

    /// This returns the bytes which are within the given bounds determined by
    /// `pos` and `size`
    ///
    /// It will also do the checking if the given crop is within the bounds of
    /// the image, if it is not it will return a [SigOperationError::InvalidCrop]
    ///
    /// * `dest` is the destination slice for the bytes and should be of size
    ///   [Self::cropped_buffer_size]. This is done for efficiency
    pub fn cropped_buffer(
        &self,
        dest: &mut [u8],
        pos: Vec2u,
        size: Vec2u,
    ) -> Result<usize, SigOperationError> {
        if pos.x + size.x > self.width()
            || pos.y + size.y > self.height()
            || size.x == 0 // Means we are signing nothing and there's no point in that
            || size.y == 0
        {
            return Err(SigOperationError::InvalidCrop(pos, size));
        }

        let buf = self.raw_buffer();

        let start_idx = Self::DEPTH * (pos.x + pos.y * self.width()) as usize;
        // This ends on the first pixel outside the value (which is why we don't add `size.x`)
        let end_idx = Self::DEPTH * (pos.x + (pos.y + size.y) * self.width()) as usize;

        let row_size = Self::DEPTH * size.x as usize;

        let byte_width = Self::DEPTH * self.width() as usize;
        let mut offset = 0;
        for start in (start_idx..end_idx).step_by(byte_width) {
            dest[offset..offset + row_size].copy_from_slice(&buf[start..start + row_size]);
            offset += row_size;
        }

        Ok(offset)
    }

    /// Returns the number of nanoseconds this frame should appear at, also
    /// known as the presentation timestamp
    #[inline]
    pub const fn get_timestamp(&self) -> Timestamp {
        self.pts
    }

    #[inline]
    pub fn size(&self) -> Vec2u {
        Vec2u::new(self.width(), self.height())
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

#[cfg(test)]
mod tests {
    use std::error::Error;

    use testlibs::{test_video, videos};

    use crate::SignPipeline;

    use super::*;

    #[test]
    fn cropped_buffer() -> Result<(), Box<dyn Error>> {
        crate::gst::init()?;

        let videos = vec![
            (videos::BIG_BUNNY, 640_usize * 360 * 3),
            (videos::BIG_BUNNY_1080, 1920_usize * 1080 * 3),
        ];
        for (vid, expected_length) in videos {
            let filepath = test_video(vid);

            let pipeline = SignPipeline::build_from_path(&filepath).unwrap().build()?;

            let mut iter = pipeline.try_into_iter(())?;
            let first = iter
                .next()
                .expect("There is not at least one frame in the video")
                .expect("Frame was not able to be decoded from video");

            let size = first.frame.dimensions().into();

            let mut full_crop = vec![0; first.frame.cropped_buffer_size(size)];

            let offset = first
                .frame
                .cropped_buffer(
                    &mut full_crop,
                    Vec2u::default(),
                    first.frame.dimensions().into(),
                )
                .expect("Cropped video correctly");

            assert_eq!(
                offset,
                full_crop.len(),
                "Predicted size from cropped_buffer_size is correct"
            );

            assert_eq!(
                full_crop.len(),
                expected_length,
                "The length of the full crop is correct"
            );

            let size = Vec2u::new(100, 100);
            let mut crop = vec![0; first.frame.cropped_buffer_size(size)];

            let offset = first
                .frame
                .cropped_buffer(&mut crop, Vec2u::new(10, 10), size)
                .expect("Cropped video correctly");

            assert_eq!(
                offset,
                crop.len(),
                "Predicted size from cropped_buffer_size is correct"
            );

            assert_eq!(
                crop.len(),
                (size.x * size.y * 3) as usize,
                "The length of the small crop is correct"
            );
        }

        Ok(())
    }

    #[test]
    fn too_large_crop() -> Result<(), Box<dyn Error>> {
        crate::gst::init()?;

        let filepath = test_video(videos::BIG_BUNNY);

        let pipeline = SignPipeline::build_from_path(&filepath).unwrap().build()?;

        let mut iter = pipeline.try_into_iter(())?;
        let first = iter
            .next()
            .expect("There is not at least one frame in the video")
            .expect("Frame was not able to be decoded from video");

        let tests = vec![
            (
                Vec2u::default(),
                Vec2u::new(first.frame.width() + 1, first.frame.height()),
            ),
            (
                Vec2u::default(),
                Vec2u::new(first.frame.width(), first.frame.height() + 1),
            ),
            (
                Vec2u::new(1, 0),
                Vec2u::new(first.frame.width(), first.frame.height()),
            ),
            (
                Vec2u::new(0, 1),
                Vec2u::new(first.frame.width(), first.frame.height()),
            ),
            (
                Vec2u::new(first.frame.width(), 0),
                Vec2u::new(1, first.frame.height()),
            ),
            (
                Vec2u::new(0, first.frame.height()),
                Vec2u::new(first.frame.width(), 1),
            ),
            (Vec2u::default(), Vec2u::new(0, first.frame.height())),
            (Vec2u::default(), Vec2u::new(first.frame.width(), 0)),
        ];

        for (pos, size) in tests {
            let mut crop = vec![0; first.frame.cropped_buffer_size(size)];
            let offset = first.frame.cropped_buffer(&mut crop, pos, size);

            assert!(
                matches!(
                    offset,
                    Err(SigOperationError::InvalidCrop(epos, esize))
                        if epos == pos && esize == size,
                ),
                "An invalid crop was detected"
            );
        }

        Ok(())
    }
}
