use std::{
    fmt::{Debug, Display},
    ops::{Div, Mul},
};

use num_traits::NumCast;
use tokio::time::{self, Duration, Sleep};

use crate::{file::Timestamp, time::ONE_SECOND_MILLIS};

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
/// ## Examples
///
/// If you were wanting to create a 30 fps video it would be as follows:
///
/// ```
/// use stream_signer::video::Framerate;
///
/// let fps = Framerate::new(30, 1);
///
/// // Note here that the frame time is rounded down to the closest millis as
/// // we are only getting the millisecond
/// assert_eq!(fps.frame_time().as_millis(), 33);
/// assert_eq!(fps.frames(), 30);
/// assert_eq!(fps.milliseconds(), 1_000); // 1 second as milliseconds
///
/// assert_eq!(fps.convert_to_frames(Duration::from_millis(67)), 2);
/// ```
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

    /// This converts the given index of current frame into the timestamp for
    /// the video
    ///
    /// Due to the underlying times using nanoseconds, there should be little
    /// loss
    ///
    /// ```
    /// use stream_signer::video::Framerate;
    ///
    /// let fps = Framerate::new(30, 1);
    ///
    /// assert_eq!(fps.convert_to_time(1).as_millis(), 33);
    /// assert_eq!(fps.convert_to_time(2).as_millis(), 66);
    /// assert_eq!(fps.convert_to_time(3).as_millis(), 100);
    /// ```
    #[inline]
    pub fn convert_to_time(&self, frames: usize) -> Timestamp {
        Timestamp::from(self.frame_time() * frames as u32)
    }

    /// This converts the given [Duration] into an index for the
    /// frame which is associated with that time frame.
    ///
    /// Note if the framerate is over 1000fps, some frames my have the same
    /// timestamp
    ///
    /// ```
    /// use stream_signer::video::Framerate;
    ///
    /// let fps = Framerate::new(30, 1);
    ///
    /// assert_eq!(fps.convert_to_frames(Duration::from_millis(33)), 0);
    /// assert_eq!(fps.convert_to_frames(Duration::from_millis(34)), 1);
    /// assert_eq!(fps.convert_to_frames(Duration::from_millis(67)), 2);
    /// assert_eq!(fps.convert_to_frames(Duration::from_millis(100)), 3);
    /// ```
    #[inline]
    pub fn convert_to_frames(&self, duration: Duration) -> usize {
        (duration * <u32 as NumCast>::from(self.frames()).unwrap()
            / <u32 as NumCast>::from(self.milliseconds()).unwrap())
        .as_millis() as usize
    }

    /// This returns the amount of time a frame is expected to be visible for
    /// with the given framerate
    ///
    /// For 30 fps:
    ///
    /// ```
    /// use stream_signer::video::Framerate;
    ///
    /// let fps = Framerate::new(30, 1);
    ///
    /// // Note here that the frame time is rounded down to the closest millis as
    /// // we are only getting the millisecond
    /// assert_eq!(fps.frame_time().as_millis(), 33);
    /// ```
    ///
    /// For 60 fps:
    ///
    /// ```
    /// use stream_signer::video::Framerate;
    ///
    /// let fps = Framerate::new(60, 1);
    ///
    /// // Note here that the frame time is rounded down to the closest millis as
    /// // we are only getting the millisecond
    /// assert_eq!(fps.frame_time().as_millis(), 16);
    /// ```
    pub fn frame_time(&self) -> Duration {
        Duration::from_secs_f64(<f64 as NumCast>::from(self.seconds()).unwrap())
            .div(<u32 as NumCast>::from(self.frames()).unwrap())
    }
}

impl Framerate<usize> {
    /// Sleeps for the remainder of the time a frame should be shown, with
    /// the `taken` being the current time used within the frame
    #[inline]
    pub fn sleep_for_rest(&self, taken: Duration) -> Sleep {
        time::sleep(self.frame_time().checked_sub(taken).unwrap_or_default())
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

impl From<Framerate<usize>> for Framerate<f64> {
    fn from(value: Framerate<usize>) -> Self {
        Framerate(value.frames() as f64, value.seconds() as f64)
    }
}

impl From<gst::Fraction> for Framerate<usize> {
    fn from(value: gst::Fraction) -> Self {
        Framerate(*value.0.numer() as usize, *value.0.denom() as usize)
    }
}
