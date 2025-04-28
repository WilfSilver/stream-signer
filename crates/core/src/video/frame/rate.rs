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
/// use std::time::Duration;
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
/// assert_eq!(fps.convert_to_frames(Duration::from_millis(67)) as usize, 2);
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
    /// let fps = Framerate::<usize>::new(30, 1);
    ///
    /// assert_eq!(fps.convert_to_time(1).as_millis(), 33);
    /// assert_eq!(fps.convert_to_time(2).as_millis(), 66);
    /// // Note here that it cannot round up due to not using floating points
    /// // in the backend
    /// assert_eq!(fps.convert_to_time(3).as_millis(), 99);
    /// ```
    #[inline]
    pub fn convert_to_time(&self, frames: usize) -> Timestamp {
        Timestamp::from(self.frame_time() * frames as u32)
    }

    /// This converts the given [Duration] into an index for the
    /// frame which is associated with that time frame.
    ///
    /// The return type is [f64] so that the caller can then determine how they
    /// wish to convert to indexes in different contexts
    ///
    /// ```
    /// use std::time::Duration;
    /// use stream_signer::video::Framerate;
    ///
    /// let fps = Framerate::<usize>::new(30, 1);
    /// // Note here that as we are rounding down 33 rounds to index 0 instead
    /// // of 1 (even tho it is 33.3333). it is up to the user of this function
    /// // to determine how to convert into usize
    /// assert_eq!(fps.convert_to_frames(Duration::from_millis(10)) as usize, 0);
    /// assert_eq!(fps.convert_to_frames(Duration::from_millis(33)) as usize, 0);
    /// assert_eq!(fps.convert_to_frames(Duration::from_millis(34)) as usize, 1);
    /// assert_eq!(fps.convert_to_frames(Duration::from_millis(65)) as usize, 1);
    /// assert_eq!(fps.convert_to_frames(Duration::from_millis(67)) as usize, 2);
    /// assert_eq!(fps.convert_to_frames(Duration::from_millis(100)) as usize, 3);
    /// ```
    #[inline]
    pub fn convert_to_frames(&self, duration: Duration) -> f64 {
        (duration * <u32 as NumCast>::from(self.frames()).unwrap()).div_duration_f64(
            Duration::from_secs(<u64 as NumCast>::from(self.seconds()).unwrap()),
        )
    }

    /// This returns the amount of time a frame is expected to be visible for
    /// with the given framerate
    ///
    /// For 30 fps:
    ///
    /// ```
    /// use stream_signer::video::Framerate;
    ///
    /// let fps = Framerate::<usize>::new(30, 1);
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
    /// let fps = Framerate::<usize>::new(60, 1);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_to_time() {
        let fps = Framerate::<usize>::new(30, 1);

        for (i, millis) in [0, 33, 66, 99, 133, 166, 199].into_iter().enumerate() {
            assert_eq!(
                fps.convert_to_time(i).as_millis(),
                millis,
                "Converting frame {i} to the time it first appears should equal {millis}"
            );
        }
    }

    #[test]
    fn convert_to_frames() {
        let fps = Framerate::<usize>::new(30, 1);

        for (i, millis) in [0, 34, 67, 100, 134, 167, 200].into_iter().enumerate() {
            assert_eq!(
                fps.convert_to_frames(Duration::from_millis(millis)).round() as usize,
                i,
                "Converting {millis}ms to the frame equals {i}"
            );
        }
    }

    #[test]
    fn conversions() {
        let fps = Framerate::<usize>::new(30, 1);

        for i in 0..100 {
            assert_eq!(
                fps.convert_to_frames(*fps.convert_to_time(i)).round() as usize,
                i,
                "Converting {i} to time and back again equals the same number"
            );
        }
    }

    #[test]
    fn frame_time() {
        let rates = [
            (Framerate::<usize>::new(30, 1), 33),
            (Framerate::<usize>::new(30, 1), 33),
            (Framerate::<usize>::new(30, 1), 33),
            (Framerate::<usize>::new(30, 1), 33),
            (Framerate::<usize>::new(30, 1), 33),
        ];

        for (fps, expected) in rates {
            assert_eq!(
                fps.frame_time().as_millis(),
                expected,
                "{fps:?} has a frame time of {expected}"
            );
        }
    }
}
