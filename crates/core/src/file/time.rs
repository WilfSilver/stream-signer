use std::{
    fmt::Display,
    ops::{Add, Deref, Rem, Sub},
};

use num_traits::{NumCast, PrimInt};
use srtlib::Timestamp as SrtTimestamp;

use crate::video::Framerate;

/// The number of milliseconds in a second.
pub const ONE_SECOND_MILLIS: u32 = 1000;
/// The number of milliseconds in a minute.
pub const ONE_MINUTE_MILLIS: u32 = 60 * ONE_SECOND_MILLIS;
/// The number of milliseconds in an hour.
pub const ONE_HOUR_MILLIS: u32 = 60 * ONE_MINUTE_MILLIS;

/// Wrapper for the timestamp to help converting between `u32` (the milliseconds)
/// and `srtlib::Timestamp`
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(u32);

impl Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Timestamp {
    pub const fn new(milliseconds: u32) -> Self {
        Self(milliseconds)
    }

    /// Creates a timestamp from the frame index, when given the frame rate
    ///
    /// TODO: Check the start offset is correct
    ///
    /// - `fps` should be: (number of frames, number of seconds)
    /// - `start_offset` should be the number of milliseconds to start the video at
    pub fn from_frames(
        frame: usize,
        fps: Framerate<usize>,
        start_offset: Option<f64>,
    ) -> (Self, usize) {
        let fps: Framerate<f64> = fps.into();

        let milliseconds = start_offset.unwrap_or_default() + fps.convert_to_ms(frame);

        (
            (milliseconds as u32).into(),
            fps.convert_to_frames(milliseconds % 1.),
        )
    }

    /// Converts the current milliseconds into the index to use for the frames
    ///
    /// - `fps` should be: (number of frames, number of seconds)
    /// - `start_offset` should be the number of milliseconds to start the video at
    pub fn into_frames(&self, fps: Framerate<usize>, start_offset: Option<f64>) -> usize {
        let fps: Framerate<f64> = fps.into();

        fps.convert_to_frames(self.0 as f64 - start_offset.unwrap_or_default())
    }
}

impl From<Timestamp> for u32 {
    fn from(value: Timestamp) -> Self {
        value.0
    }
}

impl From<u32> for Timestamp {
    fn from(value: u32) -> Self {
        Timestamp(value)
    }
}

impl From<f64> for Timestamp {
    fn from(value: f64) -> Self {
        Timestamp(value as u32)
    }
}

impl From<Timestamp> for f64 {
    fn from(value: Timestamp) -> Self {
        *value as f64
    }
}

impl From<&Timestamp> for f64 {
    fn from(value: &Timestamp) -> Self {
        **value as f64
    }
}

impl From<SrtTimestamp> for Timestamp {
    fn from(value: SrtTimestamp) -> Self {
        // For some reason it doesn't actually allow us to just get the milliseconds
        let (hours, minutes, seconds, milliseconds) = value.get();
        Timestamp(
            (hours as u32) * ONE_HOUR_MILLIS
                + (minutes as u32) * ONE_MINUTE_MILLIS
                + (seconds as u32) * ONE_SECOND_MILLIS
                + (milliseconds as u32),
        )
    }
}

impl From<Timestamp> for SrtTimestamp {
    fn from(value: Timestamp) -> Self {
        SrtTimestamp::from_milliseconds(*value)
    }
}

impl Deref for Timestamp {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Sub for Timestamp {
    type Output = Timestamp;

    fn sub(self, rhs: Self) -> Self::Output {
        Timestamp(self.0.sub(rhs.0))
    }
}

impl<T: PrimInt> Sub<T> for Timestamp {
    type Output = Timestamp;

    fn sub(self, rhs: T) -> Self::Output {
        Timestamp(self.0.sub(<u32 as NumCast>::from(rhs).unwrap()))
    }
}

impl Add for Timestamp {
    type Output = Timestamp;

    fn add(self, rhs: Self) -> Self::Output {
        Timestamp(self.0.add(rhs.0))
    }
}

impl Rem for Timestamp {
    type Output = Timestamp;

    fn rem(self, rhs: Self) -> Self::Output {
        Timestamp(self.0.rem(rhs.0))
    }
}
