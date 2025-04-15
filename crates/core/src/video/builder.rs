//! Parts of this file is inspired by [vid_frame_iter](https://github.com/Farmadupe/vid_dup_finder_lib/blob/main/vid_frame_iter)

use std::{path::Path, sync::Arc};

use futures::executor;
use glib::object::Cast;
use gst::{
    caps,
    element_factory::ElementBuilder,
    prelude::{ElementExt, ElementExtManual, GstBinExtManual, GstObjectExt, PadExt},
    ElementFactory,
};
use gst_app::{app_sink::AppSinkBuilder, AppSink};
use tokio::sync::Mutex;

use crate::{file::Timestamp, video::manager::SrcInfo};

use super::{manager::PipeInitiator, SignPipeline};

#[derive(Clone, Copy, Debug, Default)]
pub enum FramerateOption {
    #[default]
    Fastest,
    Auto,
}

pub type BuilderError = glib::BoolError;

/// Enables building, signing and verifying videos
pub struct SignPipelineBuilder<'a> {
    pub src: ElementBuilder<'a>,

    pub video_convert: ElementBuilder<'a>,
    pub video_sink: AppSinkBuilder,
    pub video_caps: gst_video::VideoCapsBuilder<caps::NoFeature>,

    pub audio_convert: ElementBuilder<'a>,
    pub audio_sink: AppSinkBuilder,
    pub audio_caps: gst_audio::AudioCapsBuilder<caps::NoFeature>,

    start_offset: Option<f64>,
}

impl SignPipelineBuilder<'_> {
    /// This will utilimately create a [SignPipeline] that is pointing to a file to
    /// read and stream from
    pub fn from_path<P: AsRef<Path>>(path: &P) -> Option<Self> {
        Some(Self::from_uri(format!(
            "file://{}",
            path.as_ref().to_str()?
        )))
    }

    /// This will utilimately create a [SignPipeline] that is pointing to a given
    /// url
    pub fn from_uri<S: ToString>(uri: S) -> Self {
        Self {
            src: ElementFactory::make("uridecodebin")
                .property("uri", uri.to_string())
                .property("buffer-size", 1_i32),

            video_convert: ElementFactory::make("videoconvert"),
            video_sink: AppSink::builder()
                .name("video_sink")
                .sync(false)
                .max_buffers(1_u32)
                .drop(false),
            video_caps: gst_video::VideoCapsBuilder::new().format(gst_video::VideoFormat::Rgb),

            audio_convert: ElementFactory::make("audioconvert"),
            audio_sink: AppSink::builder().name("audio_sink"),
            // .sync(false)
            // .drop(false),
            audio_caps: gst_audio::AudioCapsBuilder::new().format(gst_audio::AudioFormat::F32le),

            start_offset: None,
        }
    }

    /// Sets the buffer size for the URI we are recording
    pub fn with_buffer_size(mut self, size: i32) -> Self {
        self.src = self.src.property("buffer-size", size);
        self
    }

    /// Sets the buffer size for the URI we are recording
    pub fn with_max_buffers(mut self, num: u32) -> Self {
        self.video_sink = self.video_sink.max_buffers(num);
        self.audio_sink = self.audio_sink.max_buffers(num);
        self
    }

    /// Change the frame rate of the iterator. The argument is a fraction, for example:
    /// * For a framerate of one per 3 seconds, use (1, 3).
    /// * For a framerate of 12.34 frames per second use (1234 / 100).
    pub fn with_frame_rate(mut self, fps: FramerateOption) -> Self {
        match fps {
            FramerateOption::Fastest => {
                self.video_sink = self.video_sink.sync(false);
                self.audio_sink = self.audio_sink.sync(false);
            }
            FramerateOption::Auto => {
                self.video_sink = self.video_sink.sync(true);
                self.audio_sink = self.audio_sink.sync(true);
            }
        }
        self
    }

    /// Sets the number of audio channels to be expected from the format
    pub fn with_audio_channels(mut self, channels: u32) -> Self {
        self.audio_caps = self.audio_caps.field("channels", channels);
        self
    }

    /// Jump to the given time in seconds before beginning to return frames.
    pub fn with_start_offset(mut self, seconds: f64) -> Self {
        self.start_offset = Some(seconds);
        self
    }

    /// Sets the appsink name
    pub fn with_video_sink_name<S: ToString>(mut self, sink_name: S) -> Self {
        self.video_sink = self.video_sink.name(sink_name.to_string());
        self
    }

    pub fn with_audio_sink_name<S: ToString>(mut self, sink_name: S) -> Self {
        self.video_sink = self.video_sink.name(sink_name.to_string());
        self
    }

    /// Puts all the arguments into the [SignPipeline] object to then be used
    /// later
    pub fn build(self) -> Result<SignPipeline, BuilderError> {
        Ok(SignPipeline::new(self.build_raw_pipeline()?))
    }

    /// Converts this object into a [Pipeline] with the sink name it is using
    /// for the frames
    fn build_raw_pipeline(self) -> Result<PipeInitiator, BuilderError> {
        // Create the pipeline and add elements
        let pipe = gst::Pipeline::new();
        let src = self.src.build()?;
        pipe.add_many([&src])?;

        // VIDEO
        let video_convert = self.video_convert.build()?;
        let video_sink = self.video_sink.caps(&self.video_caps.build()).build();
        let video_sink_name = video_sink.name().to_string();
        pipe.add_many([&video_convert, video_sink.upcast_ref()])?;
        video_convert.link(&video_sink)?;

        // AUDIO
        let audio_convert = self.audio_convert.build()?;
        let audio_sink = self.audio_sink.caps(&self.audio_caps.build()).build();
        let audio_sink_name = audio_sink.name().to_string();

        pipe.add_many([&audio_convert, audio_sink.upcast_ref()])?;
        audio_convert.link(&audio_sink)?;

        // DYNAMIC LINKING

        let src_info = Arc::new(Mutex::new(None));
        let si = src_info.clone();
        // Connect the 'pad-added' signal to dynamically link the source to the converter
        src.connect_pad_added(move |src, src_pad| {
            let caps = src_pad.current_caps().expect("Could not get CAPS");
            let name = caps
                .structure(0)
                .expect("Could not get CAPS structure")
                .name();

            if name.starts_with("audio/") {
                let sink_pad = audio_convert
                    .static_pad("sink")
                    .expect("audioconvert should have a pad");

                if !sink_pad.is_linked() {
                    src_pad
                        .link(&sink_pad)
                        .expect("Could not link audio to src pad");
                    println!("Connected audio");
                }
            } else if name.starts_with("video/") {
                let sink_pad = video_convert
                    .static_pad("sink")
                    .expect("videoconvert should have a pad");

                if !sink_pad.is_linked() {
                    src_pad
                        .link(&sink_pad)
                        .expect("Could not link video to src pad");
                    println!("Connected video");
                }
            } else {
                println!("Got an extra pad added we didn't expect, ignoring")
            }

            let duration: Timestamp = src.query_duration::<gst::format::Time>().unwrap().into();
            executor::block_on(async {
                let mut info = si.lock().await;
                *info = Some(SrcInfo { duration });
            });
        });

        Ok(PipeInitiator {
            src: src_info,
            pipe: pipe.into(),
            // receiver: rx,
            video_sink: video_sink_name,
            audio_sink: audio_sink_name,
            offset: self.start_offset.unwrap_or_default(),
        })
    }
}
