use std::{
    fmt::Debug,
    ops::{self, Bound, Range, RangeBounds},
    time::Duration,
};

use crate::file::Timestamp;

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
/// use std::time::Duration;
/// use stream_signer::{file::Timestamp, utils::TimeRange};
/// let tr: TimeRange = TimeRange::new(Timestamp::from_millis(495), Duration::from_millis(99));
///
/// assert!(tr.crosses_interval(Duration::from_millis(100)).is_some());
/// assert!(tr.contains(&Timestamp::from_millis(500)));
/// ```
#[derive(Clone, Copy, PartialEq)]
pub struct TimeRange {
    start: Timestamp,
    end: Timestamp,
}

impl Debug for TimeRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.start.fmt(f)?;
        write!(f, "..")?;
        self.end.fmt(f)
    }
}

impl TimeRange {
    pub fn new(start: Timestamp, frame_duration: Duration) -> Self {
        Self {
            start,
            end: Timestamp(*start + frame_duration),
        }
    }

    /// Returns the [TimeRange] of the previous frame, if the start of the
    /// range is within the first frame it will return [None]
    pub fn previous(&self) -> Option<Self> {
        Some(Self {
            start: self.start.checked_sub(self.frame_duration())?.into(),
            end: self.start,
        })
    }

    /// Returns the current object as a range from the start time to the end
    /// time of the frame being displayed
    pub const fn range(&self) -> Range<Timestamp> {
        self.start..self.end
    }

    /// This checks if the given timestamp is in the current
    /// [range](TimeRange::range).
    ///
    /// This is equivalent to the implementation of [`PartialEq<Timestamp>::eq`]
    /// function
    pub fn contains(&self, timestamp: &Timestamp) -> bool {
        self.range().contains(timestamp)
    }

    /// Returns the timestamp at the start of the region
    #[inline]
    pub const fn start(&self) -> Timestamp {
        self.start
    }

    /// Returns the timestamp at the end of the region
    #[inline]
    pub const fn end(&self) -> Timestamp {
        self.end
    }

    /// Returns the duration of the frame
    pub fn frame_duration(&self) -> Duration {
        self.end - self.start
    }

    /// Returns if the frame is the starting frame
    pub fn is_start(&self) -> bool {
        self.previous().is_none()
    }

    /// This complies with specification with signing on the frame that crosses
    /// a boundary.
    ///
    /// This can be used to sign in intervals.
    ///
    /// Please note that this works with milliseconds due to GStreamer rounding
    /// to the nearest millisecond for presentation times
    ///
    /// Returns the time at which it crosses if it does
    pub fn crosses_interval(&self, interval: Duration) -> Option<Timestamp> {
        let end = (self.start % interval) + self.frame_duration();
        let rel_cross = end.round_millis().checked_sub(interval)?;
        Some(self.end - rel_cross)
    }

    /// This returns if this is the first frame, it works within the closest
    /// millisecond due to inaccuracies within gstreamer.
    pub fn is_first(&self) -> bool {
        self.start.as_millis() < self.frame_duration().as_millis()
    }
}

impl RangeBounds<Timestamp> for TimeRange {
    fn start_bound(&self) -> Bound<&Timestamp> {
        Bound::Included(&self.start)
    }
    fn end_bound(&self) -> Bound<&Timestamp> {
        Bound::Excluded(&self.end)
    }
}

impl PartialEq<Timestamp> for TimeRange {
    fn eq(&self, other: &Timestamp) -> bool {
        self.contains(other)
    }
}

impl PartialOrd<Timestamp> for TimeRange {
    fn partial_cmp(&self, other: &Timestamp) -> Option<std::cmp::Ordering> {
        if self.contains(other) {
            Some(std::cmp::Ordering::Equal)
        } else if other < &self.start {
            Some(std::cmp::Ordering::Less)
        } else {
            Some(std::cmp::Ordering::Greater)
        }
    }
}

impl ops::Rem<Timestamp> for TimeRange {
    type Output = Self;

    fn rem(self, rhs: Timestamp) -> Self::Output {
        TimeRange::new(self.start % rhs, self.frame_duration())
    }
}

impl ops::Rem<Duration> for TimeRange {
    type Output = Self;

    fn rem(self, rhs: Duration) -> Self::Output {
        TimeRange::new(self.start % rhs, self.frame_duration())
    }
}

impl ops::Sub<Timestamp> for TimeRange {
    type Output = TimeRange;

    fn sub(self, rhs: Timestamp) -> Self::Output {
        TimeRange::new((self.start - rhs).into(), self.frame_duration())
    }
}

impl ops::Add<Timestamp> for TimeRange {
    type Output = TimeRange;

    fn add(self, rhs: Timestamp) -> Self::Output {
        TimeRange::new(self.start + rhs, self.frame_duration())
    }
}
