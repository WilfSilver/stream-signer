//! This includes all the specific types and functions that just relate to
//! verifying the video.
//!
//! However note that a lot of logic is still found within the [super::manager]
//! module due to the expansion of those types

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

use super::{FrameState, SigOperationError};

/// These are potential errors which could go wrong while preparing the
/// [crate::spec::ChunkSignature] to be verified
#[derive(Debug, Error, Clone)]
pub enum InvalidSignatureError {
    /// This is caused when we are trying to decode the [identity_iota::credential::Jwt]
    /// for the presentation to retrieve the subject information
    #[error("Invalid JWT object: {0:?}")]
    Jwt(Vec<Arc<JwtValidationError>>),
    /// This is when there is something wrong with the configuration of the [crate::spec::ChunkSignature]
    #[error(transparent)]
    Operation(#[from] SigOperationError),
}

/// This stores the state of a signature, which is then passed onto the user
/// within [FrameWithSignatures]
#[derive(Debug, Clone)]
pub enum SignatureState {
    /// Due to the asyncronous nature of the library to stop stutters, the
    /// signature may not have been verified yet, and so it is set to `Loading`
    /// while we await for the verification process to be completed
    Loading,
    /// This is returned when there is a fundemental issue with the
    /// [crate::spec::ChunkSignature] defining this signature.
    Invalid(InvalidSignatureError),
    /// This is returned when the signatures did not match for the configured
    /// [crate::spec::ChunkSignature]
    Unverified {
        error: Arc<SingleStructError<SignatureVerificationErrorKind>>,
        subject: Box<Subject>,
    },
    /// This is caused when the [identity_iota::resolver::Resolver] could not
    /// get the public key needed to verify the credential
    Unresolved(Arc<ResolverError>),
    /// The signature has been validated
    Verified(Box<Subject>),
}

impl SignatureState {
    pub fn from_signer(
        signer: Result<&SubjectState, UnknownKey>,
        input: VerificationInput,
    ) -> Self {
        let signer = match signer {
            Ok(s) => s,
            Err(e) => return Self::Invalid(InvalidSignatureError::Operation(e.into())),
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

/// This is a wrapper around the [FrameState] to also include all the
/// [SignatureState]s that relate to the frame.
#[derive(Debug)]
pub struct FrameWithSignatures {
    pub state: FrameState,
    pub sigs: Vec<SignatureState>,
}
