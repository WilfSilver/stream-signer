use std::{
    ops::{Deref, DerefMut, Range},
    path::Path,
};

use rust_lapper::{Interval, Lapper};
use srtlib::{Subtitle, Subtitles, Timestamp as SrtTimestamp};

use crate::spec::ChunkSignature;

use super::{ParseError, Timestamp};

type RawSignedInterval = Interval<u32, ChunkSignature>;

pub trait AsSubtitle {
    fn as_subtitle(&self) -> Result<Subtitle, ParseError>;
}

impl AsSubtitle for RawSignedInterval {
    fn as_subtitle(&self) -> Result<Subtitle, ParseError> {
        Ok(Subtitle::new(
            0,
            SrtTimestamp::from_milliseconds(self.start),
            SrtTimestamp::from_milliseconds(self.stop),
            serde_json::to_string(&self.val)?,
        ))
    }
}

/// Wrapper type for the Interval storing the time range and signatures which
/// have signed that interval of video
#[derive(Debug, Clone)]
pub struct SignedInterval(RawSignedInterval);

impl SignedInterval {
    pub fn new(from: Timestamp, to: Timestamp, signature: ChunkSignature) -> Self {
        SignedInterval(RawSignedInterval {
            start: from.as_millis_u32(),
            stop: to.as_millis_u32(),
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

    pub const fn start(&self) -> Timestamp {
        Timestamp::from_millis_u32(self.0.start)
    }

    pub const fn stop(&self) -> Timestamp {
        Timestamp::from_millis_u32(self.0.stop)
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

impl<'a> From<SignedChunk<'a>> for SignedInterval {
    fn from(value: SignedChunk<'a>) -> Self {
        SignedInterval::new(value.range.start, value.range.end, value.signature.clone())
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
///     file::{SignedInterval, SignFile, Timestamp},
///     spec::{ChunkSignature, Vec2u, PresentationOrId},
/// };
/// use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
///
/// let mut sf = SignFile::new();
///
/// // Example signature
/// let s = "i0aL5051w2ADiUk3nljIz1Fk91S3ux3UTidX/B4EU058IKuzD9gcZ3vXAfS2coeCC4gRSiJSmDocHDeXW5tMCw";
///
/// let signature_info = ChunkSignature {
///      pos: Vec2u::new(0, 0),
///      size: Vec2u::new(1920, 1080),
///      channels: vec![1, 2],
///      presentation: PresentationOrId::new_ref("my_presentation_id"),
///      signature: STANDARD_NO_PAD.decode(s).unwrap()
/// };
///
/// sf.push(SignedInterval::new(Timestamp::ZERO, Timestamp::from_millis(1000), signature_info));
///
/// sf.write("./mysignatures.ssrt").expect("Failed to write signature file");
/// ```
///
/// Or reading signatures for a given time frame
///
/// ```no_run
/// use stream_signer::{SignFile, file::Timestamp};
///
/// let sf = SignFile::from_file("./mysignatures.ssrt").expect("Failed to read sign file");
///
/// for s in sf.get_signatures_at(Timestamp::from_millis(2000)) { // Get at 2 seconds mark
///   // ...
/// }
/// ```
///
#[derive(Debug)]
pub struct SignFile(Lapper<u32, ChunkSignature>);

impl SignFile {
    pub fn new() -> Self {
        SignFile(Lapper::new(Vec::new()))
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
    ///     file::{SignFile, SignedInterval, Timestamp},
    ///     spec::{Vec2u, PresentationOrId, ChunkSignature},
    /// };
    /// use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
    ///
    /// let mut sf = SignFile::new();
    ///
    /// // Example signature
    /// let s = "i0aL5051w2ADiUk3nljIz1Fk91S3ux3UTidX/B4EU058IKuzD9gcZ3vXAfS2coeCC4gRSiJSmDocHDeXW5tMCw";
    ///
    /// let signature_info = ChunkSignature {
    ///      pos: Vec2u::new(0, 0),
    ///      size: Vec2u::new(1920, 1080),
    ///      channels: vec![1, 2],
    ///      presentation: PresentationOrId::new_ref("my_presentation_id"),
    ///      signature: STANDARD_NO_PAD.decode(s).unwrap()
    /// };
    ///
    /// sf.push(SignedInterval::new(Timestamp::ZERO, Timestamp::from_millis(1000), signature_info));
    ///
    /// sf.write("./mysignatures.ssrt").expect("Failed to write to file");
    /// ```
    pub fn push(&mut self, chunk: SignedInterval) {
        self.0.insert(chunk.into())
    }

    /// Find all the signatures which are applied to the given timestamp and
    /// the full ranges in which they apply for.
    ///
    /// ```no_run
    /// use stream_signer::{file::Timestamp, SignFile};
    ///
    /// let sf = SignFile::from_file("./mysignatures.ssrt").expect("Failed to read file");
    ///
    /// for s in sf.get_signatures_at(Timestamp::from_millis(2000)) { // Get at 2 seconds mark
    ///   // ...
    /// }
    /// ```
    ///
    pub fn get_signatures_at(&self, at: Timestamp) -> impl Iterator<Item = SignedChunk<'_>> {
        self.0
            .find(at.as_millis_u32(), at.as_millis_u32())
            .map(|i| {
                SignedChunk::new(
                    Timestamp::from_millis_u32(i.start)..Timestamp::from_millis_u32(i.stop),
                    &i.val,
                )
            })
    }

    /// Writes the current store to a specified srt file
    ///
    /// ```no_run
    /// use stream_signer::{
    ///     file::{SignFile, SignedInterval, Timestamp},
    ///     spec::{Vec2u, PresentationOrId, ChunkSignature},
    /// };
    /// use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
    ///
    /// let mut sf = SignFile::new();
    ///
    /// // Example signature
    /// let s = "i0aL5051w2ADiUk3nljIz1Fk91S3ux3UTidX/B4EU058IKuzD9gcZ3vXAfS2coeCC4gRSiJSmDocHDeXW5tMCw";
    ///
    /// let signature_info = ChunkSignature {
    ///      pos: Vec2u::new(0, 0),
    ///      size: Vec2u::new(1920, 1080),
    ///      channels: vec![1, 2],
    ///      presentation: PresentationOrId::new_ref("my_presentation_id"),
    ///      signature: STANDARD_NO_PAD.decode(s).unwrap()
    /// };
    ///
    /// sf.push(SignedInterval::new(Timestamp::ZERO, Timestamp::from_millis(1000), signature_info));
    ///
    /// sf.write("./mysignatures.ssrt").expect("Failed to write to file");
    /// ```
    ///
    pub fn write<P: AsRef<Path>>(&self, path: P) -> Result<(), ParseError> {
        let subtitles = self
            .0
            .iter()
            .map(|c| c.as_subtitle())
            .collect::<Result<Vec<_>, _>>()?;

        Subtitles::new_from_vec(subtitles).write_to_file(path, None)?;

        Ok(())
    }
}

impl IntoIterator for SignFile {
    type Item = SignedInterval;

    type IntoIter = Box<dyn Iterator<Item = Self::Item>>;

    fn into_iter(self) -> Self::IntoIter {
        let res = self.0.into_iter().map(SignedInterval::from);
        Box::new(res)
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
    /// ```
    /// use stream_signer::{
    ///     file::{SignFile, SignedInterval, Timestamp},
    ///     spec::{Vec2u, PresentationOrId, ChunkSignature},
    /// };
    /// use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
    ///
    /// let mut sf = SignFile::new();
    ///
    /// // Example signature
    /// let s = "i0aL5051w2ADiUk3nljIz1Fk91S3ux3UTidX/B4EU058IKuzD9gcZ3vXAfS2coeCC4gRSiJSmDocHDeXW5tMCw";
    ///
    /// let first_signature = ChunkSignature {
    ///      pos: Vec2u::new(0, 0),
    ///      size: Vec2u::new(1920, 1080),
    ///      channels: vec![1, 2],
    ///      presentation: PresentationOrId::new_ref("my_presentation_id"),
    ///      signature: STANDARD_NO_PAD.decode(s).unwrap()
    /// };
    ///
    /// let second_signature = ChunkSignature {
    ///      pos: Vec2u::new(0, 0),
    ///      size: Vec2u::new(1920, 1080),
    ///      channels: vec![1, 2],
    ///      presentation: PresentationOrId::new_ref("my_presentation_id"),
    ///      signature: STANDARD_NO_PAD.decode(s).unwrap()
    /// };
    ///
    /// sf.extend(vec![
    ///   SignedInterval::new(Timestamp::ZERO, Timestamp::from_millis(1000), first_signature),
    ///   SignedInterval::new(Timestamp::from_millis(1000), Timestamp::from_millis(2000), second_signature),
    ///   // ...
    /// ]);
    ///
    /// # let filepath = testlibs::random_test_file() + ".ssrt";
    /// sf.write(&filepath).expect("Failed to write to file");
    /// ```
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
    use crate::spec::Vec2u;

    use super::*;

    fn chunk_signature() -> ChunkSignature {
        ChunkSignature {
            pos: Vec2u::new(0, 0),
            size: Vec2u::new(1920, 1080),
            channels: vec![0, 1],
            presentation: crate::spec::PresentationOrId::new_ref("did:url:https://example.com/sig"),
            signature: "AAAAAAA".as_bytes().to_owned(),
        }
    }

    #[test]
    fn write_and_read() {
        let mut signfile = SignFile::new();
        signfile.push(SignedInterval::new(
            Timestamp::from_millis(0),
            Timestamp::from_millis(100),
            chunk_signature(),
        ));

        signfile.push(SignedInterval::new(
            Timestamp::from_millis(100),
            Timestamp::from_millis(200),
            chunk_signature(),
        ));

        signfile.push(SignedInterval::new(
            Timestamp::from_millis(200),
            Timestamp::from_millis(300),
            chunk_signature(),
        ));

        let file = testlibs::random_temp_file() + ".ssrt";
        signfile.write(&file).expect("Failed to write to file");
        let signs = signfile.into_iter().collect::<Vec<_>>();

        let signfile = SignFile::from_file(&file).expect("Could not read file");
        let new_signs = signfile.into_iter().collect::<Vec<_>>();
        assert_eq!(
            signs, new_signs,
            "Signatures are the same after reading and writing to file"
        );
    }

    #[test]
    fn read_invalid_struct() {
        let res = SignFile::from_file("./tests/ssrts/invalid_struct.ssrt");

        assert!(
            matches!(res, Err(ParseError::JsonError(_))),
            "Invalid JSON is detected and error thrown"
        )
    }

    #[test]
    fn read_invalid_srt() {
        let res = SignFile::from_file("./tests/ssrts/invalid.ssrt");

        assert!(
            matches!(res, Err(ParseError::SrtError(_))),
            "Invalid SRT is detected and error thrown"
        )
    }

    #[test]
    fn write_and_read_with_overlaps() {
        let mut signfile = SignFile::new();
        signfile.push(SignedInterval::new(
            Timestamp::from_millis(0),
            Timestamp::from_millis(100),
            chunk_signature(),
        ));

        signfile.push(SignedInterval::new(
            Timestamp::from_millis(50),
            Timestamp::from_millis(150),
            chunk_signature(),
        ));

        signfile.push(SignedInterval::new(
            Timestamp::from_millis(100),
            Timestamp::from_millis(200),
            chunk_signature(),
        ));

        let file = testlibs::random_temp_file() + ".ssrt";
        signfile.write(&file).expect("Failed to write to file");
        let signs = signfile.into_iter().collect::<Vec<_>>();

        let signfile = SignFile::from_file(&file).expect("Could not read file");
        let new_signs = signfile.into_iter().collect::<Vec<_>>();
        assert_eq!(
            signs, new_signs,
            "Signatures are the same after reading and writing to file"
        );
    }
}
