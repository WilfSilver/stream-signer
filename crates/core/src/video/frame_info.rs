use std::ops::{Bound, Range, RangeBounds, Rem};

use crate::file::Timestamp;

use super::{frame_iter::RgbFrame, Framerate};

/// This stores information about the timestamp. Due to the fact that the
/// iterator does not run for every millisecond, instead running for every
/// frame, it can mean that checking the timestamp against a set time could
/// not always match.
///
/// Therefore this struct provides an iterface to help deal with this range
/// of timestaps the frame appears over.
///
/// <div class="warning">
/// The time range is equivalent to the time range which the frame is expected
/// to exist for.
/// </div>
///
/// So for example if you wanted to check if a [TimeRange] shows on the
/// millisecond which is a multiple of `x` then you can simply do:
///
/// ```
/// # let tr: TimeRange = ...;
///
/// # if tr % 100 == 0 {
/// #   // Do something
/// # }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct TimeRange {
    start: f64,
    end: f64,
}

impl TimeRange {
    fn new<T: Into<f64>>(timestamp: T, frame_duration: f64) -> Self {
        let start: f64 = timestamp.into();
        Self {
            start,
            end: start + frame_duration,
        }
    }

    /// Returns the [TimeRange] of the previous frame
    pub fn previous(&self) -> Self {
        Self {
            start: self.start - self.frame_duration(),
            end: self.start,
        }
    }

    /// Returns the current object as a range from the start time to the end
    /// time of the frame being displayed
    pub fn range(&self) -> Range<f64> {
        self.start..self.end
    }

    /// This checks if the given timestamp is in the current
    /// [range](TimeRange::range).
    ///
    /// This is equivalent to the implementation of [PartialEq<Timestamp>::eq]
    /// function
    pub fn contains<A: Into<f64>>(&self, timestamp: A) -> bool {
        self.range().contains(&timestamp.into())
    }

    /// Returns the timestamp at the start of the region
    pub fn start(&self) -> Timestamp {
        self.start.into()
    }

    /// Returns the duration of the frame
    pub fn frame_duration(&self) -> f64 {
        self.end - self.start
    }

    /// This checks if the given value
    pub fn multiple_of<T: Into<f64>>(&self, value: T) -> bool {
        let m = self.start % value.into();
        m <= self.frame_duration()
    }

    /// Returns if the frame is the starting frame
    pub fn is_start(&self) -> bool {
        self.contains(0)
    }
}

impl RangeBounds<f64> for TimeRange {
    fn start_bound(&self) -> Bound<&f64> {
        Bound::Included(&self.start)
    }
    fn end_bound(&self) -> Bound<&f64> {
        Bound::Excluded(&self.end)
    }
}

impl<N> PartialEq<N> for TimeRange
where
    N: Into<f64> + Copy,
{
    fn eq(&self, other: &N) -> bool {
        self.contains(*other)
    }
}

impl PartialOrd<Timestamp> for TimeRange {
    fn partial_cmp(&self, other: &Timestamp) -> Option<std::cmp::Ordering> {
        if self.contains(other) {
            Some(std::cmp::Ordering::Equal)
        } else if f64::from(other) < self.start {
            Some(std::cmp::Ordering::Less)
        } else {
            Some(std::cmp::Ordering::Greater)
        }
    }
}

impl Rem<Timestamp> for TimeRange {
    type Output = Self;

    fn rem(self, rhs: Timestamp) -> Self::Output {
        TimeRange::new(self.start % f64::from(rhs), self.frame_duration())
    }
}

#[derive(Debug)]
pub struct FrameInfo<'a> {
    pub frame: &'a RgbFrame,
    pub time: TimeRange,
    frame_idx: usize,
}

impl<'a> FrameInfo<'a> {
    pub fn new(
        frame: &'a RgbFrame,
        timestamp: Timestamp,
        frame_idx: usize,
        fps: Framerate<usize>,
    ) -> Self {
        Self {
            frame,
            frame_idx,
            time: TimeRange::new(timestamp, fps.seconds() as f64 / fps.frames() as f64),
        }
    }

    pub fn idx(&self) -> usize {
        self.frame_idx
    }
}
