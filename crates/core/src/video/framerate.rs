use std::{
    fmt::{Debug, Display},
    ops::{Div, Mul},
};

use num_traits::NumCast;

use crate::time::ONE_SECOND_MILLIS;

/// Basic trait to capture all the possible types available to be used in the
/// Framerate type
pub trait FramerateCompatible:
    Copy + NumCast + Mul<Output = Self> + Div<Output = Self> + Debug + Display
{
}

impl<T> FramerateCompatible for T where
    T: Copy + NumCast + Mul<Output = T> + Div<Output = T> + Debug + Display
{
}

/// Wrapper object for the gstreamer framerate aiming to provide a clearer
/// interface and make conversions easier.
///
/// It is generic to provide better options of how you want the outputs of the
/// function, for example if you wish outputs as floats you can convert between
/// them.
///
#[derive(Debug, Clone, Copy)]
pub struct Framerate<T>(T, T)
where
    T: FramerateCompatible;

impl<T> Framerate<T>
where
    T: FramerateCompatible,
{
    /// To create a new framerate, you define the number of frames for a given
    /// number of seconds. For example, 60fps would be `Framerate::new(60, 1)`
    pub const fn new(frames: T, seconds: T) -> Self {
        Self(frames, seconds)
    }

    /// The number of frames per the given number of seconds (see [Self::seconds])
    pub const fn frames(&self) -> T {
        self.0
    }

    /// The number of seconds which has the given number of frames (see [Self::frames])
    pub const fn seconds(&self) -> T {
        self.1
    }

    /// This simply converts [Self::seconds] into milliseconds by multiplying
    /// it by 1000 (using the [ONE_SECOND_MILLIS] constant)
    #[inline]
    pub fn milliseconds(&self) -> T {
        self.seconds() * <T as NumCast>::from(ONE_SECOND_MILLIS).unwrap()
    }

    /// This converts the given index of current frame into the number of
    /// milliseconds for the video
    ///
    /// Note if the framerate is over 1000fps, some frames my have the same
    /// timestamp
    #[inline]
    pub fn convert_to_ms(&self, frames: usize) -> T {
        <T as NumCast>::from(frames).unwrap() * self.milliseconds() / self.frames()
    }

    /// This converts the given number of milliseconds into an index for the
    /// frame which is associated with that time frame.
    ///
    /// Note if the framerate is over 1000fps, some frames my have the same
    /// timestamp
    #[inline]
    pub fn convert_to_frames(&self, milli: T) -> usize {
        <usize as NumCast>::from(milli * self.frames() / self.milliseconds()).unwrap()
    }
}

impl<T> From<Framerate<T>> for (T, T)
where
    T: FramerateCompatible,
{
    fn from(value: Framerate<T>) -> Self {
        (value.frames(), value.seconds())
    }
}

impl<T> Default for Framerate<T>
where
    T: FramerateCompatible,
{
    fn default() -> Self {
        Framerate(
            <T as NumCast>::from(60).unwrap(),
            <T as NumCast>::from(1).unwrap(),
        )
    }
}

impl From<Framerate<usize>> for Framerate<f64> {
    fn from(value: Framerate<usize>) -> Self {
        Framerate(value.frames() as f64, value.seconds() as f64)
    }
}
