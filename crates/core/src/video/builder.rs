//! Parts of this file is inspired by https://github.com/Farmadupe/vid_dup_finder_lib/blob/main/vid_frame_iter

use glib::object::{Cast, ObjectExt};
use gstreamer::{prelude::GstBinExt, Pipeline};

use super::{Framerate, SignPipeline, VideoError};
use crate::SignFile;

/// Enables building, signing and verifying videos outputing to a given [SignFile]
#[derive(Debug)]
pub struct SignPipelineBuilder {
    uri: String,
    fps: Option<Framerate<usize>>,
    start_offset: Option<f64>,
    sign_file: Option<SignFile>,
}

impl SignPipelineBuilder {
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
    pub fn frame_rate(mut self, fps: Framerate<usize>) -> Self {
        self.fps = Some(fps);
        self
    }

    /// Jump to the given time in seconds before beginning to return frames.
    pub fn start_offset(mut self, seconds: f64) -> Self {
        self.start_offset = Some(seconds);
        self
    }

    /// Puts all the arguments into the [SignPipeline] object to then be used
    /// later
    pub fn build(self) -> Result<SignPipeline, VideoError> {
        Ok(SignPipeline::new(
            self.build_raw_pipeline()?,
            self.start_offset,
            self.fps,
        ))
    }

    fn build_raw_pipeline(&self) -> Result<Pipeline, glib::Error> {
        let fps_arg = match self.fps {
            None => String::from(""),
            Some(fps) => fps.get_args(),
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

        Ok(pipeline)
    }
}
