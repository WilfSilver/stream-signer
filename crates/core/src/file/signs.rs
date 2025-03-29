use std::{
    ops::{Deref, DerefMut, Range},
    path::Path,
};

use rust_lapper::{Interval, Lapper};
use srtlib::{Subtitle, Subtitles, Timestamp as SrtTimestamp};

use crate::spec::ChunkSignature;

use super::{ParseError, Timestamp};

/// TODO: Remove the Vec because it seems to be much much slower
type RawSignedInterval = Interval<u32, Vec<ChunkSignature>>;

/// Wrapper type for the Interval storing the time range and signatures which
/// have signed that interval of video
#[derive(Debug, Clone)]
pub struct SignedInterval(RawSignedInterval);

impl SignedInterval {
    pub fn new(from: Timestamp, to: Timestamp, signature: Vec<ChunkSignature>) -> Self {
        SignedInterval(RawSignedInterval {
            start: from.into(),
            stop: to.into(),
            val: signature,
        })
    }

    pub fn from_subtitle(sub: Subtitle) -> Result<Self, ParseError> {
        Ok(SignedInterval::new(
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

impl From<RawSignedInterval> for SignedInterval {
    fn from(value: RawSignedInterval) -> Self {
        SignedInterval(value)
    }
}

impl From<SignedInterval> for RawSignedInterval {
    fn from(value: SignedInterval) -> Self {
        value.0
    }
}

impl Deref for SignedInterval {
    type Target = RawSignedInterval;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SignedInterval {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl PartialEq for SignedInterval {
    fn eq(&self, other: &Self) -> bool {
        self.start == other.start && self.stop == other.stop
    }
}

/// This gives a single signature with the range which that signature is
/// applied to to make it easy to iterate over
#[derive(Debug)]
pub struct SignedChunk<'a> {
    pub range: Range<Timestamp>,
    pub signature: &'a ChunkSignature,
}

impl<'a> SignedChunk<'a> {
    pub fn new(range: Range<Timestamp>, signature: &'a ChunkSignature) -> Self {
        SignedChunk { range, signature }
    }
}

/// Cache of information about all the signatures for one video. This includes
/// support for overlapping time ranges.
///
/// This reads from a file of type `.srt` -- you will notice all examples use
/// `.ssrt` to stand for "signature srt".
///
/// Creating a signature file
///
/// ```no_run
/// use stream_signer::{
///     file::{SignedInterval, SignFile},
///     spec::{ChunkSignature, Coord, PresentationOrId},
/// };
/// use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
///
/// let mut sf = SignFile::new();
///
/// // Example signature
/// let s = "i0aL5051w2ADiUk3nljIz1Fk91S3ux3UTidX/B4EU058IKuzD9gcZ3vXAfS2coeCC4gRSiJSmDocHDeXW5tMCw";
///
/// let signature_info = ChunkSignature {
///      pos: Coord::new(0, 0),
///      size: Coord::new(1920, 1080),
///      presentation: PresentationOrId::new_ref("my_presentation_id"),
///      signature: STANDARD_NO_PAD.decode(s).unwrap()
/// };
///
/// sf.push(SignedInterval::new(0.into(), 1000.into(), vec![signature_info]));
///
/// sf.write("./mysignatures.ssrt").expect("Failed to write signature file");
/// ```
///
/// Or reading signatures for a given time frame
///
/// ```no_run
/// use stream_signer::SignFile;
///
/// let sf = SignFile::from_file("./mysignatures.ssrt").expect("Failed to read sign file");
///
/// for s in sf.get_signatures_at(2000.into()) { // Get at 2 seconds mark
///   // ...
/// }
/// ```
///
#[derive(Debug)]
pub struct SignFile(Lapper<u32, Vec<ChunkSignature>>);

impl SignFile {
    pub fn new() -> Self {
        SignFile(Lapper::new(vec![]))
    }

    /// Reads the buffer as the contents of a file trying to convert it into
    /// [SignedInterval] which it then stores in a manner which can be quickly
    /// accessible
    pub fn from_buf(buf: String) -> Result<Self, ParseError> {
        let subs = Subtitles::parse_from_str(buf)?;
        Self::from_subtitles(subs)
    }

    /// Reads the given file as the given path and trys to convert the contents
    /// into [SignedInterval] which it then stores in a manner which can be
    /// quickly accessible
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ParseError> {
        let subs = Subtitles::parse_from_file(path, None)?;
        Self::from_subtitles(subs)
    }

    /// Generates directly from [Subtitles], normally read from an `.srt` file
    /// beforehand
    fn from_subtitles(subs: Subtitles) -> Result<Self, ParseError> {
        let chunks = subs
            .into_iter()
            .map(|s| SignedInterval::from_subtitle(s).map(|c| c.into()))
            .collect::<Result<Vec<RawSignedInterval>, _>>()?;

        Ok(SignFile(Lapper::new(chunks)))
    }

    /// Inserts a new signed chunk into the signatures list
    ///
    /// ```no_run
    /// use stream_signer::{
    ///     file::{SignFile, SignedInterval},
    ///     spec::{Coord, PresentationOrId, ChunkSignature},
    /// };
    /// use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
    ///
    /// let mut sf = SignFile::new();
    ///
    /// // Example signature
    /// let s = "i0aL5051w2ADiUk3nljIz1Fk91S3ux3UTidX/B4EU058IKuzD9gcZ3vXAfS2coeCC4gRSiJSmDocHDeXW5tMCw";
    ///
    /// let signature_info = ChunkSignature {
    ///      pos: Coord::new(0, 0),
    ///      size: Coord::new(1920, 1080),
    ///      presentation: PresentationOrId::new_ref("my_presentation_id"),
    ///      signature: STANDARD_NO_PAD.decode(s).unwrap()
    /// };
    ///
    /// sf.push(SignedInterval::new(0.into(), 1000.into(), vec![signature_info]));
    ///
    /// sf.write("./mysignatures.ssrt").expect("Failed to write to file");
    /// ```
    ///
    pub fn push(&mut self, chunk: SignedInterval) {
        self.0.insert(chunk.into())
    }

    /// Find all the signatures which are applied to the given timestamp and
    /// the full ranges in which they apply for.
    ///
    /// ```no_run
    /// use stream_signer::SignFile;
    ///
    /// let sf = SignFile::from_file("./mysignatures.ssrt").expect("Failed to read file");
    ///
    /// for s in sf.get_signatures_at(2000.into()) { // Get at 2 seconds mark
    ///   // ...
    /// }
    /// ```
    ///
    pub fn get_signatures_at(&self, at: Timestamp) -> impl Iterator<Item = SignedChunk<'_>> {
        self.0.find(at.into(), at.into()).flat_map(|i| {
            i.val
                .iter()
                .map(|s| SignedChunk::new(i.start.into()..i.stop.into(), s))
        })
    }

    /// Iterates over all the `SignedInterval`s stored in the tree, note this is
    /// not the individual signatures
    pub fn iter(&self) -> impl Iterator<Item = SignedInterval> + use<'_> {
        self.0.iter().map(|i| SignedInterval::from(i.clone()))
    }

    /// Writes the current store to a specified srt file
    ///
    /// ```no_run
    /// use stream_signer::{
    ///     file::{SignFile, SignedInterval},
    ///     spec::{Coord, PresentationOrId, ChunkSignature},
    /// };
    /// use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
    ///
    /// let mut sf = SignFile::new();
    ///
    /// // Example signature
    /// let s = "i0aL5051w2ADiUk3nljIz1Fk91S3ux3UTidX/B4EU058IKuzD9gcZ3vXAfS2coeCC4gRSiJSmDocHDeXW5tMCw";
    ///
    /// let signature_info = ChunkSignature {
    ///      pos: Coord::new(0, 0),
    ///      size: Coord::new(1920, 1080),
    ///      presentation: PresentationOrId::new_ref("my_presentation_id"),
    ///      signature: STANDARD_NO_PAD.decode(s).unwrap()
    /// };
    ///
    /// sf.push(SignedInterval::new(0.into(), 1000.into(), vec![signature_info]));
    ///
    /// sf.write("./mysignatures.ssrt").expect("Failed to write sign file");
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

impl FromIterator<SignedInterval> for SignFile {
    fn from_iter<T: IntoIterator<Item = SignedInterval>>(iter: T) -> Self {
        SignFile(Lapper::new(
            iter.into_iter().map(|c| c.into()).collect::<Vec<_>>(),
        ))
    }
}

impl Extend<SignedInterval> for SignFile {
    /// Inserts all the chunks of the given iterator into the sign file
    ///
    /// Note: Does not compress overlapping fields
    ///
    /// ```no_run
    /// use stream_signer::{
    ///     file::{SignFile, SignedInterval},
    ///     spec::{Coord, PresentationOrId, ChunkSignature},
    /// };
    /// use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
    ///
    /// let mut sf = SignFile::new();
    ///
    /// // Example signature
    /// let s = "i0aL5051w2ADiUk3nljIz1Fk91S3ux3UTidX/B4EU058IKuzD9gcZ3vXAfS2coeCC4gRSiJSmDocHDeXW5tMCw";
    ///
    /// let first_signature = ChunkSignature {
    ///      pos: Coord::new(0, 0),
    ///      size: Coord::new(1920, 1080),
    ///      presentation: PresentationOrId::new_ref("my_presentation_id"),
    ///      signature: STANDARD_NO_PAD.decode(s).unwrap()
    /// };
    ///
    /// let second_signature = ChunkSignature {
    ///      pos: Coord::new(0, 0),
    ///      size: Coord::new(1920, 1080),
    ///      presentation: PresentationOrId::new_ref("my_presentation_id"),
    ///      signature: STANDARD_NO_PAD.decode(s).unwrap()
    /// };
    ///
    /// sf.extend(vec![
    ///   SignedInterval::new(0.into(), 1000.into(), vec![first_signature]),
    ///   SignedInterval::new(1000.into(), 2000.into(), vec![second_signature]),
    ///   // ...
    /// ]);
    ///
    /// sf.write("./mysignatures.ssrt").expect("Failed to write to file");
    /// ```
    ///
    ///
    fn extend<T: IntoIterator<Item = SignedInterval>>(&mut self, iter: T) {
        for c in iter {
            self.push(c)
        }
    }
}

impl Default for SignFile {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    // use super::*;

    // TODO:
    // - Write and Read
    // - Read invalid
    // - Write + Read with overlaps
}
