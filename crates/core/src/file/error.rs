use srtlib::ParsingError;
use thiserror::Error;

/// Stores the possible errors that may be encountered when dealing with a sign
/// file
#[derive(Error, Debug)]
pub enum ParseError {
    /// This may arise when reading an srt file, and is caused by malformed
    /// format
    #[error(transparent)]
    SrtError(#[from] ParsingError),
    /// This happens when seriaising or deserialising JSON for the signature
    /// information, caused by malformed JSON
    #[error(transparent)]
    JsonError(#[from] serde_json::Error),
}
