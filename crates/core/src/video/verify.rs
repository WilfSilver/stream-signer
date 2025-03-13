use std::{collections::HashMap, future::Future, sync::Arc};

use futures::StreamExt;
use futures::{stream, Stream};
use identity_eddsa_verifier::EdDSAJwsVerifier;
use identity_iota::{
    core::SingleStructError,
    credential::JwtValidationError,
    prelude::Resolver,
    resolver::Error as ResolverError,
    verification::jws::{
        JwsAlgorithm, JwsVerifier, SignatureVerificationErrorKind, VerificationInput,
    },
};
use thiserror::Error;
use tokio::sync::Mutex;

use crate::{
    file::Timestamp, spec::Coord, CredentialStore, SignFile, Signer, SignerState, UnknownKey,
};

use super::{Frame, FrameError, FrameInfo, Framerate};

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

type SigID<'a> = (Timestamp, &'a Vec<u8>);
type SigCache<'a> = HashMap<SigID<'a>, SignatureState>;

pub(crate) struct VideoState<'a> {
    pub cache: Mutex<SigCache<'a>>,
    pub credentials: Mutex<CredentialStore>,
    pub signfile: &'a SignFile,
    pub start_offset: Option<f64>,
}

impl<'a> VideoState<'a> {
    pub fn new(signfile: &'a SignFile, offset: Option<f64>, resolver: Resolver) -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
            credentials: Mutex::new(CredentialStore::new(resolver)),
            signfile,
            start_offset: offset,
        }
    }
}

type FrameStateInfo = (
    usize,
    Arc<(usize, Result<Frame, glib::Error>)>,
    Box<[Arc<(usize, Result<Frame, glib::Error>)>]>,
);

pub(crate) struct FrameManager<'a> {
    pub video_state: Arc<VideoState<'a>>,
    pub frame_state: Arc<FrameState>,
}

impl<'a> FrameManager<'a> {
    pub async fn new(
        video_state: Arc<VideoState<'a>>,
        frame_state: FrameStateInfo,
    ) -> Result<Self, FrameError> {
        let offset = video_state.start_offset;

        Ok(Self {
            frame_state: Arc::new(FrameState::new(frame_state, offset)?),
            video_state,
        })
    }

    pub async fn verify_signatures(self) -> Result<Vec<SignatureState>, FrameError> {
        self.sigs_iter()
            .fold(Ok(vec![]), |state, info| async {
                let Ok(mut state) = state else {
                    return state;
                };
                let item = match info.await {
                    Ok(i) => i,
                    Err(e) => return Err(e),
                };
                state.push(item);
                Ok(state)
            })
            .await
    }

    fn sigs_iter(
        self,
    ) -> impl Stream<Item = impl Future<Output = Result<SignatureState, FrameError>> + 'a> + 'a
    {
        let sigs = self
            .video_state
            .signfile
            .get_signatures_at(self.frame_state.timestamp);

        let aself = Arc::new(self);

        stream::iter(sigs).map(move |sig| {
            let aself = aself.clone();
            async move {
                let start = sig.range.start;
                let end = sig.range.end;
                let sig = sig.signature;
                let cache_key = (start, &sig.signature);

                let mut state_cache = aself.video_state.cache.lock().await;

                // Due to us having to pass to the output of the iterator
                let state = match state_cache.get(&cache_key) {
                    Some(s) => s.clone(),
                    None => {
                        // TODO: Check if the end frame is after 10 second mark
                        let end_frame =
                            aself.convert_to_frames(end) - aself.convert_to_frames(start);

                        let frames_buf = aself.frame_state.get_chunk_buffer(end_frame);

                        let mut credentials = aself.video_state.credentials.lock().await;

                        let signer = credentials.normalise(sig.presentation.clone()).await;

                        let state = SignatureState::from_signer(
                            signer,
                            VerificationInput {
                                alg: JwsAlgorithm::EdDSA,
                                signing_input: frames_buf.into_boxed_slice(),
                                decoded_signature: sig.signature.clone().into_boxed_slice(),
                            },
                        );
                        state_cache.insert(cache_key, state.clone());
                        state
                    }
                };

                Ok(state)
            }
        })
    }

    fn convert_to_frames(&self, time: Timestamp) -> usize {
        time.into_frames(self.frame_state.fps, self.video_state.start_offset)
    }
}

type FrameIdxPair = (usize, Result<Frame, glib::Error>);
pub(crate) struct FrameState {
    pub idx: usize,
    pub frame: Frame,
    pub timestamp: Timestamp,
    pub fps: Framerate<usize>,
    pub next_frames: Box<[Arc<FrameIdxPair>]>,
}

impl FrameState {
    pub fn new(
        (i, frame, next_frames): FrameStateInfo,
        offset: Option<f64>,
    ) -> Result<Self, FrameError> {
        let frame: Frame = match &frame.1 {
            Ok(f) => f.clone(),
            Err(e) => return Err(e.clone().into()),
        };

        let fps = frame.fps();

        let (timestamp, _) = Timestamp::from_frames(i, frame.fps(), offset);

        Ok(FrameState {
            frame,
            timestamp,
            fps,
            next_frames,
            idx: i,
        })
    }

    pub fn get_chunk_buffer(&self, end_idx: usize) -> Vec<u8> {
        let size = self.size();
        let mut frames_buf: Vec<u8> =
            Vec::with_capacity(size.x as usize * size.y as usize * self.next_frames.len());

        // TODO: This will have to be controlled by the verifier/signer respectively
        let it = vec![&self.frame]
            .into_iter()
            .chain(self.next_frames[0..end_idx - 1].iter().map(|a| {
                // TODO: Check this
                // It is safe to unwrap here due to the previous
                // checks on the frames
                let res = a.1.as_ref().unwrap();
                res
            }));

        for f in it {
            // TODO: Deal with the sign crop and stuff
            frames_buf.extend_from_slice(f.raw_buffer());
        }

        frames_buf
    }

    pub fn size(&self) -> Coord {
        Coord::new(self.frame.width(), self.frame.height())
    }
}

impl From<Arc<FrameState>> for FrameInfo {
    fn from(value: Arc<FrameState>) -> Self {
        FrameInfo::new(value.frame.clone(), value.timestamp, value.idx, value.fps)
    }
}
