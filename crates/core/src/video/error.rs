use identity_iota::storage::JwkStorageDocumentError;
use thiserror::Error;

use crate::{file::Timestamp, UnknownKey};

#[derive(Error, Debug)]
pub enum VideoError {
    #[error(transparent)]
    Glib(#[from] glib::Error),
    #[error(transparent)]
    GlibBool(#[from] glib::BoolError),
    #[error("Could not find the given file")]
    InvalidPath,
    #[error("Tried to access range which is not covered by the video: {0} -> {1}")]
    OutOfRange(Timestamp, Timestamp),
    #[error(transparent)]
    Sign(#[from] JwkStorageDocumentError),
    #[error(transparent)]
    UnknownCredential(#[from] UnknownKey),
}
