use std::{
    ops::{Deref, DerefMut, Range},
    path::Path,
};

use crate::spec::SignatureInfo;
use rust_lapper::{Interval, Lapper};
use srtlib::{ParsingError, Subtitle, Subtitles, Timestamp as SrtTimestamp};

/// The number of milliseconds in a second.
const ONE_SECOND_MILLIS: u32 = 1000;
/// The number of milliseconds in a minute.
const ONE_MINUTE_MILLIS: u32 = 60 * ONE_SECOND_MILLIS;
/// The number of milliseconds in an hour.
const ONE_HOUR_MILLIS: u32 = 60 * ONE_MINUTE_MILLIS;

/// Wrapper for the timestamp to help converting between `u32` (the milliseconds)
/// and `srtlib::Timestamp`
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(u32);

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
    pub fn from_frames(frame: usize, fps: (u64, u64), start_offset: Option<f64>) -> (Self, usize) {
        let fps = (fps.0 as f64, fps.1 as f64);

        let milliseconds = start_offset.unwrap_or_default() + (fps.1 * frame as f64) / fps.0;

        (
            (milliseconds as u32).into(),
            ((milliseconds % 1.) * fps.0 / fps.1) as usize,
        )
    }

    /// Converts the current milliseconds into the index to use for the frames
    ///
    /// - `fps` should be: (number of frames, number of seconds)
    /// - `start_offset` should be the number of milliseconds to start the video at
    pub fn into_frames(&self, fps: (u64, u64), start_offset: Option<f64>) -> usize {
        let fps = (fps.0 as f64, fps.1 as f64);

        (fps.0 * (self.0 as f64 - start_offset.unwrap_or_default()) / fps.1) as usize
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

/// Stores the possible errors that may be encountered when dealing with a sign
/// file
#[derive(Debug)]
pub enum ParseError {
    /// This may arise when reading an srt file, and is caused by malformed
    /// format
    SrtError(ParsingError),
    /// This happens when seriaising or deserialising JSON for the signature
    /// information, caused by malformed JSON
    JsonError(serde_json::Error),
}

impl From<ParsingError> for ParseError {
    fn from(value: ParsingError) -> Self {
        ParseError::SrtError(value)
    }
}

impl From<serde_json::Error> for ParseError {
    fn from(value: serde_json::Error) -> Self {
        ParseError::JsonError(value)
    }
}

type SignedInterval = Interval<u32, Vec<SignatureInfo>>;

/// Wrapper type for the Interval storing the time range and signatures which
/// have signed that interval of video
#[derive(Debug, Clone)]
pub struct SignedChunk(SignedInterval);

impl SignedChunk {
    pub fn new(from: Timestamp, to: Timestamp, signature: Vec<SignatureInfo>) -> Self {
        SignedChunk(SignedInterval {
            start: from.into(),
            stop: to.into(),
            val: signature,
        })
    }

    pub fn from_subtitle(sub: Subtitle) -> Result<Self, ParseError> {
        Ok(SignedChunk::new(
            sub.start_time.into(),
            sub.end_time.into(),
            serde_json::from_str(&sub.text)?,
        ))
    }

    pub fn into_subtitle(self) -> Result<Subtitle, ParseError> {
        Ok(Subtitle::new(
            0,
            SrtTimestamp::from_milliseconds(self.start),
            SrtTimestamp::from_milliseconds(self.stop),
            serde_json::to_string(&self.val)?,
        ))
    }
}

impl From<SignedInterval> for SignedChunk {
    fn from(value: SignedInterval) -> Self {
        SignedChunk(value)
    }
}

impl From<SignedChunk> for SignedInterval {
    fn from(value: SignedChunk) -> Self {
        value.0
    }
}

impl Deref for SignedChunk {
    type Target = SignedInterval;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SignedChunk {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl PartialEq for SignedChunk {
    fn eq(&self, other: &Self) -> bool {
        self.start == other.start && self.stop == other.stop
    }
}

/// This gives a single signature with the range which that signature is
/// applied to to make it easy to iterate over
pub struct SignatureWithRange<'a> {
    pub range: Range<Timestamp>,
    pub signature: &'a SignatureInfo,
}

impl<'a> SignatureWithRange<'a> {
    pub fn new(range: Range<Timestamp>, signature: &'a SignatureInfo) -> Self {
        SignatureWithRange { range, signature }
    }
}

/// Cache of information about all the signatures for one video. This includes
/// support for overlapping time ranges.
///
/// Creating a signature file
///
/// ```
/// let mut sf = SignFile::new();
///
/// sf.push(chunk);
///
/// sf.write("./mysignatures.srt");
/// ```
///
/// Or reading signatures for a given time frame
///
/// ```
/// let sf = SignFile::from_file("./mysignatures.srt")
///
/// for s in sf.get_signatures_at(2000) { // Get at 2 seconds mark
///   // ...
/// }
/// ```
///
#[derive(Debug)]
pub struct SignFile(Lapper<u32, Vec<SignatureInfo>>);

impl SignFile {
    pub fn new() -> Self {
        SignFile(Lapper::new(vec![]))
    }

    pub fn from_buf(buf: String) -> Result<Self, ParseError> {
        let subs = Subtitles::parse_from_str(buf)?;
        Self::from_subtitles(subs)
    }

    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ParseError> {
        let subs = Subtitles::parse_from_file(path, None)?;
        Self::from_subtitles(subs)
    }

    fn from_subtitles(subs: Subtitles) -> Result<Self, ParseError> {
        let chunks = subs
            .into_iter()
            .map(|s| SignedChunk::from_subtitle(s).map(|c| c.into()))
            .collect::<Result<Vec<SignedInterval>, _>>()?;

        Ok(SignFile(Lapper::new(chunks)))
    }

    /// Inserts a new signed chunk into the signatures list
    ///
    /// ```
    /// let mut sf = SignFile::new();
    ///
    /// sf.push(SignedChunk::new(0.into(), 1000.into(), vec![signature]));
    ///
    /// sf.write("./mysignatures.srt");
    /// ```
    ///
    pub fn push(&mut self, chunk: SignedChunk) {
        self.0.insert(chunk.into())
    }

    /// Find all the signatures which are applied to the given timestamp and
    /// the full ranges in which they apply for.
    ///
    /// ```
    /// let sf = SignFile::from_file("./mysignatures.srt")
    ///
    /// for s in sf.get_signatures_at(2000) { // Get at 2 seconds mark
    ///   // ...
    /// }
    /// ```
    ///
    pub fn get_signatures_at(&self, at: Timestamp) -> impl Iterator<Item = SignatureWithRange<'_>> {
        self.0.find(at.into(), at.into()).flat_map(|i| {
            i.val
                .iter()
                .map(|s| SignatureWithRange::new(i.start.into()..i.stop.into(), s))
        })
    }

    /// Iterates over all the `SignedChunk`s stored in the tree, note this is
    /// not the individual signatures
    pub fn iter(&self) -> impl Iterator<Item = SignedChunk> + use<'_> {
        self.0.iter().map(|i| SignedChunk::from(i.clone()))
    }

    /// Writes the current store to a specified srt file
    ///
    /// ```
    /// let mut sf = SignFile::new();
    ///
    /// sf.push(SignedChunk::new(0.into(), 1000.into(), vec![signature]));
    ///
    /// sf.write("./mysignatures.srt");
    /// ```
    ///
    pub fn write<P: AsRef<Path>>(&self, path: P) -> Result<(), ParseError> {
        let subtitles = self
            .iter()
            .map(|c| c.into_subtitle())
            .collect::<Result<Vec<_>, _>>()?;

        Subtitles::new_from_vec(subtitles).write_to_file(path, None)?;

        Ok(())
    }
}

impl Default for SignFile {
    fn default() -> Self {
        Self::new()
    }
}
