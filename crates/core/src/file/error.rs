use srtlib::ParsingError;

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
