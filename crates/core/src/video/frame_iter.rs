//! This code is taken from https://github.com/Farmadupe/vid_dup_finder_lib/blob/main/vid_frame_iter

use std::iter::FusedIterator;

use gstreamer::{prelude::*, ClockTime, CoreError, MessageView, StateChangeSuccess};

use gstreamer_video::VideoFrameExt;
use image::GenericImageView;

/// Iterates over all the frames in a video.
/// The iterator will prduce Ok(frame) until
/// no more frames can be read. If any error occurred
/// (regardless of whether it stopped the underlying pipeline or not)
/// the iterator will produce Err(error).
/// Once all frames and the first error has been produced the iterator
/// will produce None.
#[derive(Debug)]
pub struct VideoFrameIter<RF: VideoFrame> {
    /// Source of video frames
    pub pipeline: gstreamer::Pipeline,

    /// Whether the last frame has been returned
    pub fused: bool,

    pub _phantom: std::marker::PhantomData<RF>,
}

impl<RF: VideoFrame> FusedIterator for VideoFrameIter<RF> {}
impl<RF: VideoFrame> Iterator for VideoFrameIter<RF> {
    type Item = Result<RF, glib::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_buf().map(|res| res.map(RF::new))
    }
}

impl<RF: VideoFrame> VideoFrameIter<RF> {
    pub fn next_buf(&mut self) -> Option<Result<gstreamer::Sample, glib::Error>> {
        //the amount of time to wait for a frame before assuming there are
        //none left.
        let try_pull_sample_timeout = 30 * gstreamer::ClockTime::SECOND;

        //required for FusedIterator
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
            .by_name("sink")
            .expect("Sink element not found")
            .downcast::<gstreamer_app::AppSink>()
            .expect("Sink element is expected to be an appsink!");

        //If any error/warning occurred, then return it now.
        if let Some(error) = Self::try_find_error(&bus) {
            return Some(Err(error));
        }

        let sample = appsink.try_pull_sample(try_pull_sample_timeout);
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

                match change_state_blocking(&self.pipeline, gstreamer::State::Null) {
                    Ok(()) => ret,
                    Err(e) => panic!("{e:?}"),
                }
            }
        }
    }

    pub fn pause(&self) -> Result<(), glib::Error> {
        change_state_blocking(&self.pipeline, gstreamer::State::Paused)
    }

    pub fn play(&self) -> Result<(), glib::Error> {
        change_state_blocking(&self.pipeline, gstreamer::State::Playing)
    }

    /// Seek to the given position in the file, passing the 'accurate' flag to gstreamer.
    /// If you want to make large jumps in a video file this may be faster than setting a
    /// very low framerate (because with a low framerate, gstreamer still decodes every frame).
    pub fn seek_accurate(&self, time: f64) -> Result<(), glib::Error> {
        use gstreamer::SeekFlags;
        let time_ns_f64 = time * ClockTime::SECOND.nseconds() as f64;
        let time_ns_u64 = time_ns_f64 as u64;
        let flags = SeekFlags::ACCURATE.union(SeekFlags::FLUSH);

        self.pipeline
            .seek_simple(flags, gstreamer::ClockTime::from_nseconds(time_ns_u64))
            .map_err(|e| glib::Error::new(CoreError::TooLazy, &e.message))
    }

    fn try_find_error(bus: &gstreamer::Bus) -> Option<glib::Error> {
        bus.pop_filtered(&[
            gstreamer::MessageType::Error,
            gstreamer::MessageType::Warning,
        ])
        .filter(|msg| matches!(msg.view(), MessageView::Error(_) | MessageView::Warning(_)))
        .map(into_glib_error)
    }
}

/// Must ensure all refcounted gobjects are cleaned up by the glib runtime.
/// This won't happen unless we set the pipeline state to null
///
/// Unfortunately there's no way to capture if something goes wrong, which
/// could cause silent memory leaks, so prefer to panic instead here.
impl<T: VideoFrame> Drop for VideoFrameIter<T> {
    fn drop(&mut self) {
        match change_state_blocking(&self.pipeline, gstreamer::State::Null) {
            Ok(()) => (),
            Err(e) => panic!("{e:?}"),
        };
    }
}

pub fn change_state_blocking(
    pipeline: &gstreamer::Pipeline,
    new_state: gstreamer::State,
) -> Result<(), glib::Error> {
    use StateChangeSuccess::*;
    let timeout = 10 * gstreamer::ClockTime::SECOND;

    let state_change_error = match pipeline.set_state(new_state) {
        Ok(Success | NoPreroll) => return Ok(()),
        Ok(Async) => {
            let (result, _curr, _pending) = pipeline.state(timeout);
            match result {
                Ok(Success | NoPreroll) => return Ok(()),

                //state change failed within timeout. Treat as error
                Ok(Async) => None,
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
                    glib::Error::new(
                        gstreamer::CoreError::TooLazy,
                        "Gstreamer State Change Error",
                    )
                } else {
                    glib::Error::new(gstreamer::CoreError::TooLazy, "Internal Gstreamer error")
                }
            }
        };

    // before returning, close down the pipeline to prevent memory leaks.
    // But if the pipeline can't close, cause a panic (preferable to memory leak)
    match change_state_blocking(pipeline, gstreamer::State::Null) {
        Ok(()) => Err(error),
        Err(e) => panic!("{e:?}"),
    }
}

pub fn into_glib_error(msg: gstreamer::Message) -> glib::Error {
    match msg.view() {
        MessageView::Error(e) => e.error(),
        MessageView::Warning(w) => w.error(),
        _ => {
            panic!("Only Warning and Error messages can be converted into GstreamerError")
        }
    }
}

// Drain all messages from the bus, keeping track of eos and error.
//(This prevents messages piling up and causing memory leaks)
pub fn get_bus_errors(bus: &gstreamer::Bus) -> impl Iterator<Item = glib::Error> + '_ {
    let errs_warns = [
        gstreamer::MessageType::Error,
        gstreamer::MessageType::Warning,
    ];

    std::iter::from_fn(move || bus.pop_filtered(&errs_warns).map(into_glib_error))
}

pub(crate) mod private {
    pub trait VideoFrameInternal {
        fn new(sample: gstreamer::Sample) -> Self;
        fn gst_video_format() -> gstreamer_video::VideoFormat;
    }
}
use private::VideoFrameInternal;

pub trait VideoFrame: VideoFrameInternal {
    /// Get a reference to the raw framebuffer data from gstreamer
    fn raw_frame(&self) -> &gstreamer_video::VideoFrame<gstreamer_video::video_frame::Readable>;
}

/// Conversion functions to types provided by the popular[`image`] crate.
pub trait ImageFns {
    type IB;

    /// Get a [`image::FlatSamples`] for this frame with a borrowed reference to the underlying frame data.
    fn as_flat(&self) -> image::FlatSamples<&[u8]>;

    /// Copy the underlying frame data into an owned [`image::ImageBuffer`].
    fn to_imagebuffer(&self) -> Self::IB;
}

/// A single video frame, with 24 bits per pixel, Rgb encoding.
///
/// You can access the raw data by:
/// * calling [`ImageFns::to_imagebuffer`] to copy the raw pixels into an owned [`image::ImageBuffer`].
/// * calling [`ImageFns::as_flat`] to get a [`image::FlatSamples`] struct representing the layout of the frame's raw data.
/// * directly indexing individual pixels using the functions from the [`image::GenericImageView`] trait.
/// * calling [`VideoFrame::raw_frame`] to get a reference to the raw data in gstreamer's internal format.
///
/// # Lifetimes and ownership
/// The underlying raw frame data is owned and reference counted by gstreamer, so it is generally cheap to clone frames.
/// If you want to pass frames around in your code, it is better to clone them instead of handing outreferences. In other
/// words, you can treat frames as if they were wrapped by an [`std::rc::Rc`]
///
/// Most functions have been written to avoid copying raw frames. Currently the only function that does copy is [`ImageFns::to_imagebuffer`].
///
/// # Examples
/// Sum the raw pixel values of an entire frame.
/// ```
/// # use vid_frame_iter::VideoFrameIterBuilder;
/// # use vid_frame_iter::RgbFrame;
/// # use std::ffi::OsStr;
/// #
/// # vid_frame_iter::init_gstreamer();
/// #
/// # let VIDEO_URI_HERE : String = url::Url::from_file_path(std::env::current_dir().unwrap().join(OsStr::new("examples/vids/dog.1.mp4")).to_string_lossy().to_string()).unwrap().to_string(); println!("{VIDEO_URI_HERE}");
/// #
/// # let builder = vid_frame_iter::VideoFrameIterBuilder::from_uri(VIDEO_URI_HERE);
/// # let mut f_it = builder.spawn_rgb().unwrap();
/// # let frame: RgbFrame = f_it.next().unwrap().unwrap();
/// #
///  use image::GenericImageView;
///  use image::Rgb;
///
/// // let frame: RgbFrame = { ... } // (See other examples for how to create frames)
///
///  let sum: u64 = frame
///      .pixels()
///      .map(|(_x, _y, Rgb::<u8>([r, g, b]))| (r as u64) + (g as u64) + (b as u64))
///      .sum();
/// println!("sum of pixels values in this frame: {sum}");
///
/// # // Sanity check that we did actually do what we said.
/// # assert!(sum >= 1);
/// ```
///
/// Save a frame to a PNG file on disk.
/// ```
/// # fn main() -> Result<(), image::ImageError> {
/// # vid_frame_iter::init_gstreamer();
/// # use vid_frame_iter::ImageFns;
/// # use std::ffi::OsStr;
/// #
/// # #[allow(non_snake_case)]
/// # let VIDEO_URI_HERE : String = url::Url::from_file_path(std::env::current_dir().unwrap().join(OsStr::new("examples/vids/dog.1.mp4")).to_string_lossy().to_string()).unwrap().to_string(); println!("{VIDEO_URI_HERE}");
/// #
/// # let builder = vid_frame_iter::VideoFrameIterBuilder::from_uri(VIDEO_URI_HERE);
/// # let mut f_it = builder.spawn_rgb().unwrap();
/// #
/// # let frame = f_it.next().unwrap().unwrap();
/// #
/// // let frame: RgbFrame = { ... } // (See other examples for how to create frames)
///
/// // We have to convert the frame to an [`image::ImageBuffer`] to be able to save it.
/// let frame_buf: image::RgbImage = frame.to_imagebuffer();
/// # let ret =
/// frame_buf.save_with_format("image_file.png", image::ImageFormat::Bmp)
/// # ;
/// # //sanity check.
/// # assert!(ret.is_ok());
/// # Ok(())
/// # }
/// ```
///
#[derive(Debug)]
pub struct RgbFrame(gstreamer_video::VideoFrame<gstreamer_video::video_frame::Readable>);

// Safety: See safety note for GrayFrame.
impl Clone for RgbFrame {
    /// Clone this video frame. This operation is cheap because it does not clone the underlying
    /// data (it actually relies on gstreamer's refcounting mechanism)
    fn clone(&self) -> Self {
        let buffer = self.0.buffer_owned();
        let frame = gstreamer_video::VideoFrame::from_buffer_readable(buffer, self.0.info())
            .expect("Failed to map buffer readable");
        Self(frame)
    }
}

impl GenericImageView for RgbFrame {
    type Pixel = image::Rgb<u8>;

    fn dimensions(&self) -> (u32, u32) {
        self.as_flat()
            .as_ref()
            .as_view::<image::Rgb<u8>>()
            .expect("unreachable")
            .dimensions()
    }

    fn get_pixel(&self, x: u32, y: u32) -> Self::Pixel {
        self.as_flat()
            .as_view::<image::Rgb<u8>>()
            .expect("unreachable")
            .get_pixel(x, y)
    }
}

impl VideoFrameInternal for RgbFrame {
    fn new(sample: gstreamer::Sample) -> Self {
        let caps = sample.caps().expect("Sample without caps");
        let info = gstreamer_video::VideoInfo::from_caps(caps).expect("Failed to parse caps");

        let buffer = sample
            .buffer_owned()
            .expect("Failed to get buffer from appsink");

        let frame = gstreamer_video::VideoFrame::from_buffer_readable(buffer, &info)
            .expect("Failed to map buffer readable");

        Self(frame)
    }

    fn gst_video_format() -> gstreamer_video::VideoFormat {
        gstreamer_video::VideoFormat::Rgb
    }
}

impl VideoFrame for RgbFrame {
    fn raw_frame(&self) -> &gstreamer_video::VideoFrame<gstreamer_video::video_frame::Readable> {
        &self.0
    }
}

impl ImageFns for RgbFrame {
    type IB = image::RgbImage;

    fn as_flat(&self) -> image::FlatSamples<&[u8]> {
        //safety: See safety note for send/sync impl (gstreamer guarantees that this pointer exists and does not move
        //for the life of self)
        let data_ref: &[u8] = self.0.plane_data(0).expect("rgb frames have 1 plane");
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

    #[must_use]
    fn to_imagebuffer(&self) -> Self::IB {
        let width = self.0.width();
        let height = self.0.height();

        let flat = self.as_flat();
        let view = flat.as_view().expect("unreachable");

        image::ImageBuffer::from_fn(width, height, |x, y| view.get_pixel(x, y))
    }
}
