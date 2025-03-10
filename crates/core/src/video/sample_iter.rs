//! This provides an easy interface to iterate over the samples in a pipeline
//!
//! This basically a cut down version of <https://github.com/Farmadupe/vid_dup_finder_lib/blob/main/vid_frame_iter>

use std::iter::FusedIterator;

use glib::object::Cast;
use gst::{
    prelude::{ElementExt, ElementExtManual, GstBinExt},
    ClockTime, CoreError, MessageView, SeekFlags, StateChangeSuccess,
};

#[derive(Debug)]
pub struct SampleIter {
    /// Source of video frames
    pub pipeline: gst::Pipeline,

    // The amount of time to wait for a frame before assuming there are
    // none left.
    pub timeout: gst::ClockTime,

    /// The sink element created
    pub sink: String,

    /// Whether the last frame has been returned
    pub fused: bool,
}

impl SampleIter {
    pub fn new(pipeline: gst::Pipeline) -> Self {
        let sink = "sink".to_string();

        let appsink = pipeline
            .by_name(&sink)
            .expect("Sink element not found")
            .downcast::<gst_app::AppSink>()
            .expect("Sink element is expected to be an appsink!");

        // Tell the appsink what format we want.
        // This can be set after linking the two objects, because format negotiation between
        // both elements will happen during pre-rolling of the pipeline.
        // appsink.set_caps(Some(
        //     &gst::Caps::builder("video/x-raw")
        //         .field("format", gst_video::VideoFormat::Rgb)
        //         .build(),
        // ));

        Self {
            pipeline,
            timeout: 30 * gst::ClockTime::SECOND,
            sink,
            fused: false,
        }
    }

    pub fn pause(&self) -> Result<(), glib::Error> {
        change_state_blocking(&self.pipeline, gst::State::Paused)
    }

    pub fn play(&self) -> Result<(), glib::Error> {
        change_state_blocking(&self.pipeline, gst::State::Playing)
    }

    /// Seek to the given position in the file, passing the 'accurate' flag to gstreamer.
    /// If you want to make large jumps in a video file this may be faster than setting a
    /// very low framerate (because with a low framerate, gstreamer still decodes every frame).
    pub fn seek_accurate(&self, time: f64) -> Result<(), glib::Error> {
        let time_ns_f64 = time * ClockTime::SECOND.nseconds() as f64;
        let time_ns_u64 = time_ns_f64 as u64;
        let flags = SeekFlags::ACCURATE.union(SeekFlags::FLUSH);

        self.pipeline
            .seek_simple(flags, gst::ClockTime::from_nseconds(time_ns_u64))
            .map_err(|e| glib::Error::new(CoreError::TooLazy, &e.message))
    }

    fn try_find_error(bus: &gst::Bus) -> Option<glib::Error> {
        bus.pop_filtered(&[gst::MessageType::Error, gst::MessageType::Warning])
            .filter(|msg| matches!(msg.view(), MessageView::Error(_) | MessageView::Warning(_)))
            .map(into_glib_error)
    }
}

impl FusedIterator for SampleIter {}
impl Iterator for SampleIter {
    type Item = Result<gst::Sample, glib::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        // Required for FusedIterator
        if self.fused {
            return None;
        }

        let bus = self
            .pipeline
            .bus()
            .expect("Failed to get pipeline from bus. Shouldn't happen!");

        // Get access to the appsink element.
        let appsink = self
            .pipeline
            .by_name(&self.sink)
            .expect("Sink element not found")
            .downcast::<gst_app::AppSink>()
            .expect("Sink element is expected to be an appsink!");

        // If any error/warning occurred, then return it now.
        if let Some(error) = Self::try_find_error(&bus) {
            return Some(Err(error));
        }

        let sample = appsink.try_pull_sample(self.timeout);
        match sample {
            //If a frame was extracted then return it.
            Some(sample) => Some(Ok(sample)),

            None => {
                // Make sure no more frames can be drawn if next is called again
                self.fused = true;

                //if no sample was returned then we might have hit the timeout.
                //If so check for any possible error being written into the log
                //at that time
                let ret = match Self::try_find_error(&bus) {
                    Some(error) => Some(Err(error)),
                    _ => {
                        if !appsink.is_eos() {
                            Some(Err(glib::Error::new(
                                CoreError::TooLazy,
                                "Gstreamer timed out",
                            )))

                        // Otherwise we hit EOS and nothing else suspicious happened
                        } else {
                            None
                        }
                    }
                };

                match change_state_blocking(&self.pipeline, gst::State::Null) {
                    Ok(()) => ret,
                    Err(e) => panic!("{e:?}"),
                }
            }
        }
    }
}

fn change_state_blocking(
    pipeline: &gst::Pipeline,
    new_state: gst::State,
) -> Result<(), glib::Error> {
    let timeout = 10 * gst::ClockTime::SECOND;

    let state_change_error = match pipeline.set_state(new_state) {
        Ok(StateChangeSuccess::Success | StateChangeSuccess::NoPreroll) => return Ok(()),
        Ok(StateChangeSuccess::Async) => {
            let (result, _curr, _pending) = pipeline.state(timeout);
            match result {
                Ok(StateChangeSuccess::Success | StateChangeSuccess::NoPreroll) => return Ok(()),

                //state change failed within timeout. Treat as error
                Ok(StateChangeSuccess::Async) => None,
                Err(e) => Some(e),
            }
        }

        Err(e) => Some(e),
    };

    // If there was any error then return that.
    // If no error but timed out then say so.
    // If no error and no timeout then any report will do.
    let error: glib::Error =
        match get_bus_errors(&pipeline.bus().expect("failed to get gst bus")).next() {
            Some(e) => e,
            _ => {
                if let Some(_e) = state_change_error {
                    glib::Error::new(gst::CoreError::TooLazy, "Gstreamer State Change Error")
                } else {
                    glib::Error::new(gst::CoreError::TooLazy, "Internal Gstreamer error")
                }
            }
        };

    // Before returning, close down the pipeline to prevent memory leaks.
    // But if the pipeline can't close, cause a panic (preferable to memory leak)
    match change_state_blocking(pipeline, gst::State::Null) {
        Ok(()) => Err(error),
        Err(e) => panic!("{e:?}"),
    }
}

fn into_glib_error(msg: gst::Message) -> glib::Error {
    match msg.view() {
        MessageView::Error(e) => e.error(),
        MessageView::Warning(w) => w.error(),
        _ => {
            panic!("Only Warning and Error messages can be converted into GstreamerError")
        }
    }
}

/// Drain all messages from the bus, keeping track of eos and error.
/// (This prevents messages piling up and causing memory leaks)
fn get_bus_errors(bus: &gst::Bus) -> impl Iterator<Item = glib::Error> + '_ {
    let errs_warns = [gst::MessageType::Error, gst::MessageType::Warning];

    std::iter::from_fn(move || bus.pop_filtered(&errs_warns).map(into_glib_error))
}
