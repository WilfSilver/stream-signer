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

use super::frame_iter::RgbFrame;

#[derive(Debug)]
pub struct UnverifiedSignature {
    error: SingleStructError<SignatureVerificationErrorKind>,
    signer: Signer,
}

#[derive(Debug, Error, Clone)]
pub enum InvalidSignatureError {
    #[error("Invalid JWT object: {0:?}")]
    Jwt(Vec<Arc<JwtValidationError>>),
    #[error(transparent)]
    UnknownRef(#[from] UnknownKey),
}

#[derive(Debug)]
pub enum SignatureState {
    Invalid(InvalidSignatureError),
    Unverified(UnverifiedSignature),
    Unresolved(Arc<ResolverError>),
    Verified(Signer),
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
                    Ok(()) => Self::Verified(s.clone()),
                    Err(e) => Self::Unverified(UnverifiedSignature {
                        error: e.into(),
                        signer: s.clone(),
                    }),
                }
            }
            // TODO: Improve names
            SignerState::Invalid(e) => Self::Invalid(InvalidSignatureError::Jwt(e.clone())),
            SignerState::ResolverFailed(e) => Self::Unresolved(e.clone()),
        }
    }
}

#[derive(Debug)]
pub struct VerifiedFrame {
    pub frame: RgbFrame,
    pub sigs: Vec<SignatureState>,
}
