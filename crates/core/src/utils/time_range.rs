use std::{
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
/// let tr: TimeRange = TimeRange::new(Timestamp::from_millis(495), Duration::from_secs_f64(0.0995));
///
/// assert_eq!(tr % Duration::from_millis(100), TimeRange::new(Timestamp::from_millis(95), Duration::from_secs_f64(0.0995)));
/// assert!((tr % Duration::from_millis(100)).is_start());
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeRange {
    start: Timestamp,
    end: Timestamp,
}

impl TimeRange {
    pub fn new(start: Timestamp, frame_duration: Duration) -> Self {
        Self {
            start,
            end: Timestamp::from(*start + frame_duration),
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
    pub fn range(&self) -> Range<Timestamp> {
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
    pub fn start(&self) -> Timestamp {
        self.start
    }

    /// Returns the duration of the frame
    pub fn frame_duration(&self) -> Duration {
        self.end - self.start
    }

    /// This checks if the given value
    pub fn multiple_of(&self, value: Duration) -> bool {
        let m = self.start % value;
        m <= self.frame_duration().into()
    }

    /// Returns if the frame is the starting frame
    pub fn is_start(&self) -> bool {
        self.previous().is_none()
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
