//! Stores basic wrappers around the GStreamer library

use gst::{
    CoreError, SeekFlags, StateChangeSuccess,
    prelude::{ElementExt, ElementExtManual},
};

use crate::{file::Timestamp, video::utils::get_bus_errors};

pub trait SetState {
    fn set_state_blocking(&self, new_state: gst::State) -> Result<(), glib::Error>;
}

impl<T: ElementExt> SetState for T {
    fn set_state_blocking(&self, new_state: gst::State) -> Result<(), glib::Error> {
        let timeout = gst::ClockTime::SECOND;

        let state_change_error = match self.set_state(new_state) {
            Ok(StateChangeSuccess::Success | StateChangeSuccess::NoPreroll) => return Ok(()),
            Ok(StateChangeSuccess::Async) => {
                let (result, _curr, _pending) = self.state(timeout);
                match result {
                    Ok(StateChangeSuccess::Success | StateChangeSuccess::NoPreroll) => {
                        return Ok(());
                    }

                    // state change failed within timeout. Treat as error
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
            match get_bus_errors(&self.bus().expect("failed to get gst bus")).next() {
                Some(e) => e,
                _ => {
                    if let Some(_e) = state_change_error {
                        glib::Error::new(gst::CoreError::TooLazy, "Gstreamer State Change Error")
                    } else {
                        glib::Error::new(gst::CoreError::TooLazy, "Internal Gstreamer error")
                    }
                }
            };

        if new_state == gst::State::Null {
            return Err(error);
        }

        // Before returning, close down the pipeline to prevent memory leaks.
        // But if the pipeline can't close, cause a panic (preferable to memory leak)
        match self.set_state_blocking(gst::State::Null) {
            Ok(()) => Err(error),
            Err(e) => panic!("{e:?}"),
        }
    }
}

/// This is a friendly wrapper around [gst::Pipeline]
///
/// Note this does not implement [Drop] so that it can be cloned and
/// shared, you are expected to run [Self::close] when you want to drop
#[derive(Debug)]
pub struct Pipeline(pub(super) gst::Pipeline);

impl Pipeline {
    pub const fn raw(&self) -> &gst::Pipeline {
        &self.0
    }

    /// Sets the pipeline to the [gst::State::Paused] state, blocking the
    /// thread until it has changed
    pub fn pause(&self) -> Result<(), glib::Error> {
        self.set_state_blocking(gst::State::Paused)
    }

    /// Sets the pipeline to the [gst::State::Playing] state, blocking the
    /// thread until it has changed
    pub fn play(&self) -> Result<(), glib::Error> {
        self.set_state_blocking(gst::State::Playing)
    }

    /// Sets the pipeline to the [gst::State::Null] state, via [gst::State::Paused]
    /// and [gst::State::Ready] to help clear memory leaks
    ///
    /// This is required to stop any memory leaks when the pipeline ends
    pub fn close(&self) -> Result<(), glib::Error> {
        let state = self.0.state(None).1;

        // Nothing needs doing if we are already in the null state
        if state == gst::State::Null {
            return Ok(());
        }

        if state == gst::State::Playing {
            self.set_state_blocking(gst::State::Paused)?;
            self.set_state_blocking(gst::State::Ready)?;
        }

        self.set_state_blocking(gst::State::Null)
    }

    /// Seek to the given position in the file, passing the 'accurate' flag to gstreamer.
    /// If you want to make large jumps in a video file this may be faster than setting a
    /// very low framerate (because with a low framerate, gstreamer still decodes every frame).
    pub fn seek_accurate(&self, time: Timestamp) -> Result<(), glib::Error> {
        let flags = SeekFlags::ACCURATE.union(SeekFlags::FLUSH);

        self.raw()
            .seek_simple(flags, gst::ClockTime::from_nseconds(time.as_nanos() as u64))
            .map_err(|e| glib::Error::new(CoreError::TooLazy, &e.message))
    }
}

impl SetState for Pipeline {
    fn set_state_blocking(&self, new_state: gst::State) -> Result<(), glib::Error> {
        self.raw().set_state_blocking(new_state)
    }
}

impl From<gst::Pipeline> for Pipeline {
    fn from(value: gst::Pipeline) -> Self {
        Self(value)
    }
}
