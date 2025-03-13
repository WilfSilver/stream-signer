//! Parts of this file is inspired by <https://github.com/Farmadupe/vid_dup_finder_lib/blob/main/vid_frame_iter>

use std::path::Path;

use glib::object::ObjectExt;
use gst::{
    element_factory::ElementBuilder,
    prelude::{ElementExt, ElementExtManual, GstBinExtManual, PadExt},
    Element, ElementFactory, Pipeline,
};

use super::{SignPipeline, VideoError};

#[derive(Clone, Copy, Debug, Default)]
pub enum FramerateOption {
    #[default]
    Fastest,
    Auto,
}

/// Enables building, signing and verifying videos
pub struct SignPipelineBuilder<'a> {
    pub src: ElementBuilder<'a>,
    pub convert: ElementBuilder<'a>,
    pub sink: ElementBuilder<'a>,
    pub extras: Result<Vec<Element>, glib::BoolError>,
    start_offset: Option<f64>,
}

impl SignPipelineBuilder<'_> {
    pub fn from_path<P: AsRef<Path>>(path: &P) -> Option<Self> {
        Some(Self::from_uri(format!(
            "file://{}",
            path.as_ref().to_str()?
        )))
    }

    pub fn from_uri<S: ToString>(uri: S) -> Self {
        Self {
            src: ElementFactory::make("uridecodebin")
                .property("uri", uri.to_string())
                .property("buffer-size", 1_i32),
            // .property("caps", caps),
            convert: ElementFactory::make("videoconvert"),
            sink: ElementFactory::make("appsink")
                .property("name", "sink")
                .property("sync", false)
                .property("max-buffers", 1_u32)
                .property("drop", false),
            extras: Ok(vec![]),
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
        self.sink = self.sink.property("max-buffers", num);
        self
    }

    pub fn with_known_extra(self, extra: Element) -> Self {
        self.with_extra(Ok(extra))
    }

    pub fn with_known_extras<I>(self, extras: I) -> Self
    where
        I: IntoIterator<Item = Element>,
    {
        self.with_potential_extras(Ok(extras))
    }

    pub fn with_extra(mut self, extra: Result<Element, glib::BoolError>) -> Self {
        if let Ok(extras) = &mut self.extras {
            match extra {
                Ok(value) => extras.push(value),
                Err(e) => self.extras = Err(e),
            }
        }
        self
    }

    pub fn with_potential_extras<I>(mut self, extras: Result<I, glib::BoolError>) -> Self
    where
        I: IntoIterator<Item = Element>,
    {
        if let Ok(old_extras) = &mut self.extras {
            match extras {
                Ok(values) => old_extras.extend(values),
                Err(e) => {
                    self.extras = Err(e);
                }
            }
        }
        self
    }

    pub fn with_extras<I>(mut self, extras: I) -> Self
    where
        I: IntoIterator<Item = Result<Element, glib::BoolError>>,
    {
        if let Ok(old_extras) = &mut self.extras {
            for e in extras {
                match e {
                    Ok(value) => old_extras.push(value),
                    Err(e) => {
                        self.extras = Err(e);
                        break;
                    }
                }
            }
        }
        self
    }

    /// Change the frame rate of the iterator. The argument is a fraction, for example:
    /// * For a framerate of one per 3 seconds, use (1, 3).
    /// * For a framerate of 12.34 frames per second use (1234 / 100).
    pub fn with_frame_rate(mut self, fps: FramerateOption) -> Self {
        match fps {
            FramerateOption::Fastest => self.sink = self.sink.property("sync", false),
            FramerateOption::Auto => self.sink = self.sink.property("sync", true),
        }
        self
    }

    /// Jump to the given time in seconds before beginning to return frames.
    pub fn with_start_offset(mut self, seconds: f64) -> Self {
        self.start_offset = Some(seconds);
        self
    }

    /// Sets the appsink name
    pub fn with_sink_name<S: ToString>(mut self, sink_name: S) -> Self {
        self.sink = self.sink.property("name", sink_name.to_string());
        self
    }

    /// Puts all the arguments into the [SignPipeline] object to then be used
    /// later
    pub fn build(self) -> Result<SignPipeline, VideoError> {
        let start = self.start_offset;
        let (pipe, sink) = self.build_raw_pipeline()?;
        // TODO: Pass sink name
        Ok(SignPipeline::new(pipe, start, sink))
    }

    fn build_raw_pipeline(self) -> Result<(Pipeline, String), VideoError> {
        // Create the pipeline and add elements
        let src = self.src.build()?;
        let convert = self.convert.build()?;
        let caps = gst::Caps::builder("video/x-raw")
            .field("format", gst_video::VideoFormat::Rgb.to_string())
            .build();
        let sink = self.sink.property("caps", caps).build()?;
        let sink_name = sink.property::<String>("name");
        let extras = self.extras?;

        let pipeline = gst::Pipeline::new();
        let chain = [&src]
            .into_iter()
            .chain(extras.iter())
            .chain([&convert, &sink]);

        pipeline.add_many(chain.clone())?;
        // We dynamically link the source later on
        Element::link_many(chain.skip(1))?;

        // Connect the 'pad-added' signal to dynamically link the source to the converter
        let sn = sink_name.clone();
        src.connect_pad_added(move |src, pad| {
            let first_pad = extras
                .first()
                .unwrap_or(&convert)
                .static_pad(&sn)
                .expect("Could not get expected sink");
            pad.link(&first_pad).expect("Could not link pad to appsink");

            println!(
                "Duration: {}",
                src.query_duration::<gst::format::Time>().unwrap()
            );
        });

        Ok((pipeline, sink_name))
    }
}
