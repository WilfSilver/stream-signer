// Parts of this file is inspired by https://github.com/Farmadupe/vid_dup_finder_lib/blob/main/vid_frame_iter

#[cfg(feature = "signing")]
pub use super::sign::SignInfo;
#[cfg(feature = "verifying")]
use pqc_dilithium::verify;

use glib::object::{Cast, ObjectExt};
use gstreamer::prelude::GstBinExt;
use image::GenericImageView;
use std::collections::{HashMap, VecDeque};

use super::frame_iter::{ImageFns, RgbFrame, VideoFrame, VideoFrameIter};
use crate::{spec::Coord, SignFile, SignedChunk, Timestamp};

pub enum VideoError {
    Glib(glib::Error),
    OutOfRange((Timestamp, Timestamp)),
}

impl From<glib::Error> for VideoError {
    fn from(value: glib::Error) -> Self {
        VideoError::Glib(value)
    }
}

#[derive(Debug)]
pub struct SignedVideoBuilder {
    uri: String,
    fps: Option<(u64, u64)>,
    start_offset: Option<f64>,
    sign_file: Option<SignFile>,
}

impl SignedVideoBuilder {
    pub fn from_uri<S: AsRef<str>>(uri: S) -> Self {
        Self {
            uri: uri.as_ref().to_string(),
            fps: None,
            start_offset: None,
            sign_file: None,
        }
    }

    /// Gets the URI which the video will be streamed from
    pub fn get_uri(&self) -> &str {
        &self.uri
    }

    /// Sets the signatures of the video to allow it to be verified later
    pub fn sign_file(mut self, sign_file: SignFile) -> Self {
        self.sign_file = Some(sign_file);
        self
    }

    /// Change the frame rate of the iterator. The argument is a fraction, for example:
    /// * For a framerate of one per 3 seconds, use (1, 3).
    /// * For a framerate of 12.34 frames per second use (1234 / 100).
    pub fn frame_rate(mut self, fps: (u64, u64)) -> Self {
        self.fps = Some(fps);
        self
    }

    /// Jump to the given time in seconds before beginning to return frames.
    pub fn start_offset(mut self, seconds: f64) -> Self {
        self.start_offset = Some(seconds);
        self
    }

    /// Returns the fps which has been set or the default
    ///
    /// TODO: Change this to get from the current pipeline
    pub fn fps_or_default(&self) -> (u64, u64) {
        self.fps.unwrap_or((60, 1))
    }

    /// Signs the current built video writing to the sign_file
    #[cfg(feature = "signing")]
    pub fn sign<F>(&mut self, sign_with: F) -> Result<(), VideoError>
    where
        F: Fn(Timestamp, &RgbFrame) -> Vec<SignInfo>,
    {
        let fps = self.fps_or_default();
        let buf_capacity = (10 * fps.0 / fps.1) as usize; // TODO: Make constant
        let mut frame_buffer: VecDeque<RgbFrame> = VecDeque::with_capacity(buf_capacity);

        for (i, frame) in self.spawn_rgb()?.enumerate() {
            let frame = frame?;
            if frame_buffer.len() == buf_capacity {
                frame_buffer.pop_front();
            }
            let size = Coord::new(frame.width(), frame.height());

            let (timestamp, excess_frames) = Timestamp::from_frames(i, fps, self.start_offset);
            let sign_info = sign_with(timestamp, &frame);

            frame_buffer.push_back(frame);
            let mut chunks: HashMap<Timestamp, SignedChunk> = HashMap::default();
            for si in sign_info {
                let start = si.start;

                // TODO: Create custom error type
                let i = frame_buffer
                    .len()
                    .checked_sub(i - excess_frames - start.into_frames(fps, self.start_offset))
                    .ok_or(VideoError::OutOfRange((start, timestamp)))?;

                let mut frames_buf: Vec<u8> = Vec::with_capacity(
                    size.x as usize * size.y as usize * (frame_buffer.len() - i),
                );
                for f in frame_buffer.range(i..frame_buffer.len()) {
                    frames_buf.extend_from_slice(
                        f.as_flat()
                            .image_slice()
                            .ok_or(VideoError::OutOfRange((start, timestamp)))?,
                    );
                }

                let signature = si.sign(&frames_buf, size);
                match chunks.get_mut(&start) {
                    Some(c) => c.val.push(signature),
                    None => {
                        chunks.insert(start, SignedChunk::new(start, timestamp, vec![signature]));
                    }
                }
            }
        }

        Ok(())
    }

    /// Consumes the builder and creates an iterator returning video frames.
    /// Frames are Rgb, with 8 bits per colour.
    fn spawn_rgb(&self) -> Result<VideoFrameIter<RgbFrame>, glib::Error> {
        self.create_pipeline::<RgbFrame>()
    }

    fn create_pipeline<RF: VideoFrame>(&self) -> Result<VideoFrameIter<RF>, glib::Error> {
        let fps_arg = match self.fps {
            None => String::from(""),
            Some((numer, denom)) => {
                format!(
                    "videorate name=rate ! capsfilter name=ratefilter ! video/x-raw,framerate={numer}/{denom} ! "
                )
            }
        };

        // Create our pipeline from a pipeline description string.
        let src_path = &self.uri;
        let pipeline_desc = format!(
            "uridecodebin uri=\"{src_path}\" buffer-size=1 ! {fps_arg} videoconvert ! appsink name=sink"
        );

        let pipeline = gstreamer::parse::launch(&pipeline_desc)?
            .downcast::<gstreamer::Pipeline>()
            .expect("Expected a gstreamer::Pipeline");

        // Get access to the appsink element.
        let appsink = pipeline
            .by_name("sink")
            .expect("Sink element not found")
            .downcast::<gstreamer_app::AppSink>()
            .expect("Sink element is expected to be an appsink!");

        // Don't synchronize on the clock.
        appsink.set_property("sync", false);

        // To save memory and CPU time prevent the sink element decoding any more than the minimum.
        appsink.set_max_buffers(1);
        appsink.set_drop(false);

        // Tell the appsink what format we want.
        // This can be set after linking the two objects, because format negotiation between
        // both elements will happen during pre-rolling of the pipeline.
        appsink.set_caps(Some(
            &gstreamer::Caps::builder("video/x-raw")
                .field("format", RF::gst_video_format().to_str())
                .build(),
        ));

        let pipeline = VideoFrameIter::<RF> {
            pipeline,
            fused: false,
            _phantom: std::marker::PhantomData,
        };
        pipeline.pause()?;

        if let Some(skip_amount) = self.start_offset {
            pipeline.seek_accurate(skip_amount)?;
        }

        pipeline.play()?;
        Ok(pipeline)
    }
}
