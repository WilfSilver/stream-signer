use identity_iota::storage::JwkStorageDocumentError;
use thiserror::Error;

use crate::video::{SigOperationError, StreamError, VideoError};

/// This stores all the posible errors which may arise while signing a video
#[cfg(feature = "signing")]
#[derive(Error, Debug)]
pub enum SigningError {
    #[error(transparent)]
    Stream(#[from] StreamError),
    #[error(transparent)]
    Operation(#[from] SigOperationError),
    #[error(transparent)]
    Sign(#[from] JwkStorageDocumentError),
}

#[cfg(feature = "signing")]
impl From<SigningError> for VideoError {
    fn from(value: SigningError) -> Self {
        match value {
            SigningError::Stream(e) => e.into(),
            SigningError::Operation(e) => e.into(),
            SigningError::Sign(e) => e.into(),
        }
    }
}
