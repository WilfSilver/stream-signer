use core::fmt;
use std::{
    ops::{Add, Deref, Rem, Sub},
    time::Duration,
};

use srtlib::Timestamp as SrtTimestamp;

use crate::video::Framerate;

/// The number of nanoseconds in a millsecond.
pub const ONE_MILLI_NANOS: u64 = 1_000_000;
/// The number of milliseconds in a second.
pub const ONE_SECOND_MILLIS: u64 = 1_000;
/// The number of nanoseconds in a second.
pub const ONE_SECOND_NANOS: u64 = ONE_MILLI_NANOS * ONE_SECOND_MILLIS;
/// The number of milliseconds in a minute.
pub const ONE_MINUTE_MILLIS: u64 = 60 * ONE_SECOND_MILLIS;
/// The number of milliseconds in an hour.
pub const ONE_HOUR_MILLIS: u64 = 60 * ONE_MINUTE_MILLIS;

/// Wrapper for the timestamp to help converting between `u32` (the milliseconds)
/// and [srtlib::Timestamp]
///
/// This is used as the core timestamp object throughout the library, however
/// it is worth noting that due to this reliance on milliseconds, however when
/// accuracy is required basic [u64] as nanoseconds is used, as this is
/// what GStreamer uses.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(Duration);

impl fmt::Debug for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Timestamp {
    pub const ZERO: Timestamp = Timestamp(Duration::ZERO);

    pub const fn from_millis_u32(millis: u32) -> Self {
        Self(Duration::from_millis(millis as u64))
    }

    pub fn from_millis_f64(millis: f64) -> Self {
        Self(Duration::from_secs_f64(millis / ONE_SECOND_MILLIS as f64))
    }

    pub const fn from_millis(millis: u64) -> Self {
        Self(Duration::from_millis(millis))
    }

    pub const fn from_nanos(nanos: u64) -> Self {
        Self(Duration::from_nanos(nanos))
    }

    pub const fn from_secs(secs: u64) -> Self {
        Self(Duration::from_secs(secs))
    }

    pub fn from_secs_f64(secs: f64) -> Self {
        Self(Duration::from_secs_f64(secs))
    }

    pub const fn as_millis_u32(&self) -> u32 {
        self.0.as_millis() as u32
    }

    pub const fn as_millis_f64(&self) -> f64 {
        self.0.as_secs_f64() * 1000.
    }

    /// Returns the number of nanoseconds over the next milliseconds, due to
    /// the signfile only working to the closest millisecond
    pub const fn excess_nanos(&self) -> u64 {
        (self.0.as_nanos() % ONE_MILLI_NANOS as u128) as u64
    }

    /// Returns the number of frames over the next milliseconds, due to
    /// the signfile only working to the closest millisecond
    pub fn excess_frames(&self, fps: Framerate<usize>) -> usize {
        fps.convert_to_frames(Duration::from_nanos(self.excess_nanos()))
    }

    /// Converts the current milliseconds into the index to use for the frames
    ///
    /// - `fps` should be: (number of frames, number of seconds)
    /// - `start_offset` should be the number of milliseconds to start the video at
    pub fn into_frames(&self, fps: Framerate<usize>, start_offset: Timestamp) -> usize {
        fps.convert_to_frames(*self - start_offset)
    }
}

impl From<SrtTimestamp> for Timestamp {
    fn from(value: SrtTimestamp) -> Self {
        // For some reason it doesn't actually allow us to just get the milliseconds
        let (hours, minutes, seconds, milliseconds) = value.get();
        Timestamp(Duration::from_millis(
            (hours as u64) * ONE_HOUR_MILLIS
                + (minutes as u64) * ONE_MINUTE_MILLIS
                + (seconds as u64) * ONE_SECOND_MILLIS
                + (milliseconds as u64),
        ))
    }
}

impl From<Timestamp> for SrtTimestamp {
    fn from(value: Timestamp) -> Self {
        SrtTimestamp::from_milliseconds(value.as_millis() as u32)
    }
}

impl From<Duration> for Timestamp {
    fn from(value: Duration) -> Self {
        Self(value)
    }
}

impl From<gst::format::Time> for Timestamp {
    fn from(value: gst::format::Time) -> Self {
        Timestamp(Duration::from_nanos(value.nseconds()))
    }
}

impl Deref for Timestamp {
    type Target = Duration;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Sub for Timestamp {
    type Output = Duration;

    fn sub(self, rhs: Self) -> Self::Output {
        self.0 - rhs.0
    }
}

impl Sub<Duration> for Timestamp {
    type Output = Timestamp;

    fn sub(self, rhs: Duration) -> Self::Output {
        Timestamp(self.0 - rhs)
    }
}

impl Add for Timestamp {
    type Output = Timestamp;

    fn add(self, rhs: Self) -> Self::Output {
        Timestamp(self.0.add(rhs.0))
    }
}

impl Add<Duration> for Timestamp {
    type Output = Timestamp;

    fn add(self, rhs: Duration) -> Self::Output {
        Timestamp(self.0.add(rhs))
    }
}

impl Rem for Timestamp {
    type Output = Timestamp;

    fn rem(self, rhs: Self) -> Self::Output {
        Timestamp::from_nanos((self.0.as_nanos() % rhs.0.as_nanos()) as u64)
    }
}

impl Rem<Duration> for Timestamp {
    type Output = Timestamp;

    fn rem(self, rhs: Duration) -> Self::Output {
        Timestamp::from_nanos((self.0.as_nanos() % rhs.as_nanos()) as u64)
    }
}
