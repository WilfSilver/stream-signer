use std::sync::Arc;

use identity_eddsa_verifier::EdDSAJwsVerifier;
use identity_iota::{
    core::SingleStructError,
    credential::JwtValidationError,
    resolver::Error as ResolverError,
    verification::jws::{JwsVerifier, SignatureVerificationErrorKind, VerificationInput},
};
use thiserror::Error;

use crate::utils::{Subject, SubjectState, UnknownKey};

use super::FrameInfo;

#[derive(Debug, Error, Clone)]
pub enum InvalidSignatureError {
    #[error("Invalid JWT object: {0:?}")]
    Jwt(Vec<Arc<JwtValidationError>>),
    #[error(transparent)]
    UnknownRef(#[from] UnknownKey),
}

#[derive(Debug, Clone)]
pub enum SignatureState {
    Invalid(InvalidSignatureError),
    Unverified {
        error: Arc<SingleStructError<SignatureVerificationErrorKind>>,
        subject: Box<Subject>,
    },
    Unresolved(Arc<ResolverError>),
    Verified(Box<Subject>),
}

impl SignatureState {
    pub fn from_signer(
        signer: Result<&SubjectState, UnknownKey>,
        input: VerificationInput,
    ) -> Self {
        let signer = match signer {
            Ok(s) => s,
            Err(e) => return Self::Invalid(InvalidSignatureError::UnknownRef(e)),
        };

        let verifier = EdDSAJwsVerifier::default();
        match signer {
            SubjectState::Valid(s) => {
                let verified = verifier.verify(input, &s.public_key);
                match verified {
                    // TODO: Potential to remove clone and use lifetime
                    Ok(()) => Self::Verified(s.clone()),
                    Err(e) => Self::Unverified {
                        error: e.into(),
                        subject: s.clone(),
                    },
                }
            }
            SubjectState::Invalid(e) => Self::Invalid(InvalidSignatureError::Jwt(e.clone())),
            SubjectState::ResolverFailed(e) => Self::Unresolved(e.clone()),
        }
    }
}

// TODO: Make this interface slightly more compact please
#[derive(Debug)]
pub struct VerifiedFrame {
    pub info: FrameInfo,
    pub sigs: Vec<SignatureState>,
}
