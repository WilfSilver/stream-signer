use std::{
    ops::{Deref, DerefMut, Range},
    path::Path,
};

use rust_lapper::{Interval, Lapper};
use srtlib::{Subtitle, Subtitles, Timestamp as SrtTimestamp};

use crate::spec::SignatureInfo;

use super::{ParseError, Timestamp};

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
///
/// TODO: Figure out where it places in terms if [SignedChunk] (I don't like
/// the name)
#[derive(Debug)]
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
/// This reads from a file of type `.srt` -- you will notice all examples use
/// `.ssrt` to stand for "signature srt".
///
/// Creating a signature file
///
/// ```no_run
/// use stream_signer::{
///     file::{SignFile, SignedChunk},
///     spec::{Coord, PresentationReference, SignatureInfo},
/// };
/// use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
///
/// let mut sf = SignFile::new();
///
/// // Example signature
/// let s = "i0aL5051w2ADiUk3nljIz1Fk91S3ux3UTidX/B4EU058IKuzD9gcZ3vXAfS2coeCC4gRSiJSmDocHDeXW5tMCw";
///
/// let signature_info = SignatureInfo {
///      pos: Coord::new(0, 0),
///      size: Coord::new(1920, 1080),
///      presentation: PresentationReference {
///          id: "my_presentation_id".to_string(),
///      }.into(),
///      signature: STANDARD_NO_PAD.decode(s).unwrap()
/// };
///
/// sf.push(SignedChunk::new(0.into(), 1000.into(), vec![signature_info]));
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
pub struct SignFile(Lapper<u32, Vec<SignatureInfo>>);

impl SignFile {
    pub fn new() -> Self {
        SignFile(Lapper::new(vec![]))
    }

    /// Reads the buffer as the contents of a file trying to convert it into
    /// [SignedChunk] which it then stores in a manner which can be quickly
    /// accessible
    pub fn from_buf(buf: String) -> Result<Self, ParseError> {
        let subs = Subtitles::parse_from_str(buf)?;
        Self::from_subtitles(subs)
    }

    /// Reads the given file as the given path and trys to convert the contents
    /// into [SignedChunk] which it then stores in a manner which can be
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
            .map(|s| SignedChunk::from_subtitle(s).map(|c| c.into()))
            .collect::<Result<Vec<SignedInterval>, _>>()?;

        Ok(SignFile(Lapper::new(chunks)))
    }

    /// Inserts a new signed chunk into the signatures list
    ///
    /// ```no_run
    /// use stream_signer::{
    ///     file::{SignFile, SignedChunk},
    ///     spec::{Coord, PresentationReference, SignatureInfo},
    /// };
    /// use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
    ///
    /// let mut sf = SignFile::new();
    ///
    /// // Example signature
    /// let s = "i0aL5051w2ADiUk3nljIz1Fk91S3ux3UTidX/B4EU058IKuzD9gcZ3vXAfS2coeCC4gRSiJSmDocHDeXW5tMCw";
    ///
    /// let signature_info = SignatureInfo {
    ///      pos: Coord::new(0, 0),
    ///      size: Coord::new(1920, 1080),
    ///      presentation: PresentationReference {
    ///          id: "my_presentation_id".to_string(),
    ///      }.into(),
    ///      signature: STANDARD_NO_PAD.decode(s).unwrap()
    /// };
    ///
    /// sf.push(SignedChunk::new(0.into(), 1000.into(), vec![signature_info]));
    ///
    /// sf.write("./mysignatures.ssrt").expect("Failed to write to file");
    /// ```
    ///
    pub fn push(&mut self, chunk: SignedChunk) {
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
    /// ```no_run
    /// use stream_signer::{
    ///     file::{SignFile, SignedChunk},
    ///     spec::{Coord, PresentationReference, SignatureInfo},
    /// };
    /// use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
    ///
    /// let mut sf = SignFile::new();
    ///
    /// // Example signature
    /// let s = "i0aL5051w2ADiUk3nljIz1Fk91S3ux3UTidX/B4EU058IKuzD9gcZ3vXAfS2coeCC4gRSiJSmDocHDeXW5tMCw";
    ///
    /// let signature_info = SignatureInfo {
    ///      pos: Coord::new(0, 0),
    ///      size: Coord::new(1920, 1080),
    ///      presentation: PresentationReference {
    ///          id: "my_presentation_id".to_string(),
    ///      }.into(),
    ///      signature: STANDARD_NO_PAD.decode(s).unwrap()
    /// };
    ///
    /// sf.push(SignedChunk::new(0.into(), 1000.into(), vec![signature_info]));
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

impl FromIterator<SignedChunk> for SignFile {
    fn from_iter<T: IntoIterator<Item = SignedChunk>>(iter: T) -> Self {
        SignFile(Lapper::new(
            iter.into_iter().map(|c| c.into()).collect::<Vec<_>>(),
        ))
    }
}

impl Extend<SignedChunk> for SignFile {
    /// Inserts all the chunks of the given iterator into the sign file
    ///
    /// Note: Does not compress overlapping fields
    ///
    /// ```no_run
    /// use stream_signer::{
    ///     file::{SignFile, SignedChunk},
    ///     spec::{Coord, PresentationReference, SignatureInfo},
    /// };
    /// use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
    ///
    /// let mut sf = SignFile::new();
    ///
    /// // Example signature
    /// let s = "i0aL5051w2ADiUk3nljIz1Fk91S3ux3UTidX/B4EU058IKuzD9gcZ3vXAfS2coeCC4gRSiJSmDocHDeXW5tMCw";
    ///
    /// let first_signature = SignatureInfo {
    ///      pos: Coord::new(0, 0),
    ///      size: Coord::new(1920, 1080),
    ///      presentation: PresentationReference {
    ///          id: "my_presentation_id".to_string(),
    ///      }.into(),
    ///      signature: STANDARD_NO_PAD.decode(s).unwrap()
    /// };
    ///
    /// let second_signature = SignatureInfo {
    ///      pos: Coord::new(0, 0),
    ///      size: Coord::new(1920, 1080),
    ///      presentation: PresentationReference {
    ///          id: "my_presentation_id".to_string(),
    ///      }.into(),
    ///      signature: STANDARD_NO_PAD.decode(s).unwrap()
    /// };
    ///
    /// sf.extend(vec![
    ///   SignedChunk::new(0.into(), 1000.into(), vec![first_signature]),
    ///   SignedChunk::new(1000.into(), 2000.into(), vec![second_signature]),
    ///   // ...
    /// ]);
    ///
    /// sf.write("./mysignatures.ssrt").expect("Failed to write to file");
    /// ```
    ///
    ///
    fn extend<T: IntoIterator<Item = SignedChunk>>(&mut self, iter: T) {
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
