use std::sync::Arc;

use identity_eddsa_verifier::EdDSAJwsVerifier;
use identity_iota::{
    core::SingleStructError,
    credential::JwtValidationError,
    resolver::Error as ResolverError,
    verification::jws::{JwsVerifier, SignatureVerificationErrorKind, VerificationInput},
};
use thiserror::Error;

use crate::{Signer, SignerState, UnknownKey};

use super::FrameInfo;

#[derive(Debug, Clone)]
pub struct UnverifiedSignature {
    pub error: Arc<SingleStructError<SignatureVerificationErrorKind>>,
    pub signer: Box<Signer>,
}

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
    Unverified(UnverifiedSignature),
    Unresolved(Arc<ResolverError>),
    Verified(Box<Signer>),
}

impl SignatureState {
    pub fn from_signer(signer: Result<&SignerState, UnknownKey>, input: VerificationInput) -> Self {
        let signer = match signer {
            Ok(s) => s,
            Err(e) => return Self::Invalid(InvalidSignatureError::UnknownRef(e)),
        };

        let verifier = EdDSAJwsVerifier::default();
        match signer {
            SignerState::Valid(s) => {
                let verified = verifier.verify(input, &s.public_key);
                match verified {
                    // TODO: Potential to remove clone and use lifetime
                    Ok(()) => Self::Verified(s.clone()),
                    Err(e) => Self::Unverified(UnverifiedSignature {
                        error: e.into(),
                        signer: s.clone(),
                    }),
                }
            }
            SignerState::Invalid(e) => Self::Invalid(InvalidSignatureError::Jwt(e.clone())),
            SignerState::ResolverFailed(e) => Self::Unresolved(e.clone()),
        }
    }
}

// TODO: Make this interface slightly more compact please
#[derive(Debug)]
pub struct VerifiedFrame {
    pub info: FrameInfo,
    pub sigs: Vec<SignatureState>,
}
