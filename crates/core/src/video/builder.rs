//! Parts of this file is inspired by https://github.com/Farmadupe/vid_dup_finder_lib/blob/main/vid_frame_iter

use std::path::Path;

use glib::object::ObjectExt;
use gstreamer::{
    element_factory::ElementBuilder,
    prelude::{ElementExt, ElementExtManual, GstBinExtManual, PadExt},
    Element, ElementFactory, Pipeline,
};

use super::{Framerate, SignPipeline, VideoError};

pub struct CustomFramerate<'a> {
    pub capsfilter: ElementBuilder<'a>,
    pub videorate: ElementBuilder<'a>,
    pub framerate: Framerate<usize>,
}

impl<'a> CustomFramerate<'a> {
    pub fn new(frames: usize, seconds: usize) -> Self {
        Framerate::new(frames, seconds).into()
    }

    /// Returns a string of the arguments which set the framerate for the gstreamer pipeline
    ///
    /// See [gstreamer::parse::launch]
    pub fn into_args(self) -> Result<impl IntoIterator<Item = Element>, glib::BoolError> {
        let numer = self.framerate.frames();
        let denom = self.framerate.seconds();

        let caps = self.capsfilter.build()?;

        Ok([caps, self.videorate.build()?])

        // format!(
        //     "videorate name=rate ! capsfilter name=ratefilter ! video/x-raw,framerate={numer}/{denom} ! "
        // )
    }
}

impl<'a> From<Framerate<usize>> for CustomFramerate<'a> {
    fn from(value: Framerate<usize>) -> Self {
        Self {
            capsfilter: ElementFactory::make("videorate").property("name", "rate"),
            videorate: ElementFactory::make("capsfilter").property("name", "ratefilter"),
            framerate: value,
        }
    }
}

#[derive(Default)]
pub enum FramerateOption<'a> {
    #[default]
    Fastest,
    Auto,
    Custom(CustomFramerate<'a>),
}

/// Enables building, signing and verifying videos outputing to a given [SignFile]
pub struct SignPipelineBuilder<'a> {
    pub src: ElementBuilder<'a>,
    pub convert: ElementBuilder<'a>,
    pub sink: ElementBuilder<'a>,
    pub extras: Vec<Element>,
    // uri: Option<String>,
    start_offset: Option<f64>,
    // buffer_size: u32,
    // sink_name: String,
    // max_buffers: u32,
}

impl<'a> SignPipelineBuilder<'a> {
    pub fn from_path<P: AsRef<Path>>(path: &P) -> Option<Self> {
        Some(Self::from_uri(&format!(
            "file://{}",
            path.as_ref().to_str()?
        )))
    }

    pub fn from_uri<S: ToString>(uri: S) -> Self {
        Self {
            src: ElementFactory::make("uridecodebin")
                .property("uri", uri.to_string())
                .property("buffer-size", 1_i32),
            convert: ElementFactory::make("videoconvert"),
            sink: ElementFactory::make("appsink")
                .property("name", "sink")
                .property("sync", false)
                .property("max-buffers", 1_u32)
                .property("drop", false),
            extras: vec![],
            start_offset: None,
        }
    }

    /// Sets the buffer size for the URI we are recording
    pub fn buffer_size(mut self, size: i32) -> Self {
        self.src = self.src.property("buffer-size", size);
        self
    }

    /// Sets the buffer size for the URI we are recording
    pub fn max_buffers(mut self, num: u32) -> Self {
        self.sink = self.sink.property("max-buffers", num);
        self
    }

    /// Change the frame rate of the iterator. The argument is a fraction, for example:
    /// * For a framerate of one per 3 seconds, use (1, 3).
    /// * For a framerate of 12.34 frames per second use (1234 / 100).
    pub fn frame_rate(mut self, fps: FramerateOption<'a>) -> Result<Self, VideoError> {
        match fps {
            FramerateOption::Fastest => self.sink = self.sink.property("sync", false),
            FramerateOption::Auto => self.sink = self.sink.property("sync", true),
            FramerateOption::Custom(fps) => {
                self.sink = self.sink.property("sync", true);
                self.extras.extend(fps.into_args()?.into_iter());
            }
        }
        Ok(self)
    }

    /// Jump to the given time in seconds before beginning to return frames.
    pub fn start_offset(mut self, seconds: f64) -> Self {
        self.start_offset = Some(seconds);
        self
    }

    /// Sets the appsink name
    pub fn sink_name<S: ToString>(mut self, sink_name: S) -> Self {
        self.sink = self.sink.property("name", sink_name.to_string());
        self
    }

    /// Puts all the arguments into the [SignPipeline] object to then be used
    /// later
    pub fn build(self) -> Result<SignPipeline, VideoError> {
        let start = self.start_offset;
        Ok(SignPipeline::new(
            self.build_raw_pipeline()?,
            start,
            None, // TODO: Autodetect framerate
        ))
    }

    fn build_raw_pipeline(self) -> Result<Pipeline, VideoError> {
        // Create the pipeline and add elements
        let src = self.src.build()?;
        let convert = self.convert.build()?;
        let sink = self.sink.build()?;

        let pipeline = gstreamer::Pipeline::new();
        pipeline.add_many(
            [&src, &convert, &sink]
                .into_iter()
                .chain(self.extras.iter()),
        )?;

        convert.link(&sink).unwrap();

        // Connect the 'pad-added' signal to dynamically link the source to the converter
        src.connect_pad_added(move |_, pad| {
            let convert_pad = convert
                .static_pad(&sink.property::<String>("name"))
                .unwrap();
            println!("{:?}", convert_pad);
            pad.link(&convert_pad).unwrap();
        });

        Ok(pipeline)
    }
}
