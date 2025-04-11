//! This stores the mostly internally used tools to make it easier to iterate
//! over some given frames while signing or verifying them
//!
//! This holds the specific logic for either verifying (in [verification])
//! or signing (in [sign]) and has specific implementations depending on the
//! contexts used within the manager

use std::{ops::Range, sync::Arc};

use glib::object::{Cast, ObjectExt};
use gst::prelude::{ElementExt, GstBinExt};
use gst_app::AppSink;
use tokio::sync::Mutex;

use crate::{file::Timestamp, spec::Vec2u};

use super::{
    Frame, FrameState, Framerate, Pipeline, SigOperationError, StreamError, MAX_CHUNK_LENGTH,
    MIN_CHUNK_LENGTH,
};

/// This trait is designed to make it easier to extract the exact bytes which
/// need encrypting from a stored buffer of [Frame]s
pub trait FrameBuffer {
    /// This should return the specific frames within the given range as an iterator,
    /// this is not done through a given trait of [std::ops::Range], to make it more flexible to
    /// the structures this can be applied to
    fn with_frames<'a>(&'a self, range: Range<usize>) -> Box<dyn Iterator<Item = &'a Frame> + 'a>;

    /// This uses the [FrameBuffer::with_frames] to return the exact buffer which
    /// should be signed for a given position and size of the viewing window
    ///
    /// This is quite a slow function, all tactics to reduce this time have
    /// been tried, the fact is that cloning this amount of data into a vector
    /// is just quite slow
    fn get_cropped_buffer(
        &self,
        pos: Vec2u,
        size: Vec2u,
        range: Range<usize>,
    ) -> Result<Vec<u8>, SigOperationError> {
        // TODO: Add audio
        let capacity = 3 * size.x as usize * size.y as usize * range.len();
        let mut frames_buf: Vec<u8> = Vec::new();
        frames_buf.reserve_exact(capacity);

        let frames = self.with_frames(range);

        for f in frames {
            frames_buf.extend(f.cropped_buffer(pos, size)?);
        }

        Ok(frames_buf)
    }
}

/// This manager should be created for each frame and stores the state of the
/// pipeline and frame
///
/// By having an overall manager, it has access to a much wider range of
/// information and can use that to its advantage.
///
/// Both [FrameState] and [PipeState] have their own contexts, which is basically
/// a structure of extra information which then can be used by their respective
/// impelementations
#[derive(Debug, Clone)]
pub struct PipeManager<VC, FC> {
    /// The information stored about the frame itself
    pub frame: Arc<FrameManager<FC>>,
    /// The information about the pipes state and other information which was
    /// needed when building the pipeline
    pub state: Arc<PipeState<VC>>,
}

impl<VC, FC> PipeManager<VC, FC> {
    /// Creates a new manager with the given `state` and `frame_info`
    ///
    /// For ease of use we generate the [FrameState] from the given tuple of
    /// information
    pub fn new(
        state: Arc<PipeState<VC>>,
        frame_info: (usize, Result<Frame, StreamError>, FC),
    ) -> Result<Self, StreamError> {
        let offset = state.offset;

        Ok(Self {
            frame: Arc::new(FrameManager::new(
                frame_info.0,
                frame_info.1,
                frame_info.2,
                offset,
            )?),
            state,
        })
    }

    /// Returns the framerate the video is expected to be playing at from its
    /// metadata
    pub fn fps(&self) -> Framerate<usize> {
        self.frame.raw.fps()
    }

    /// This uses its information to convert a given [Timestamp] to a frame
    /// index relative to any buffers stored
    pub fn convert_to_frames(&self, time: Timestamp) -> usize {
        time.into_frames(self.fps(), self.state.offset)
    }

    pub async fn get_frame_state(&self) -> FrameState {
        FrameState::new(
            self.state
                .src
                .lock()
                .await
                .clone()
                .expect("Source information was not set"),
            self.state.pipe.clone(),
            self.frame.raw.clone(),
            self.frame.idx,
            self.fps(),
            self.state.offset,
        )
    }
}

#[derive(Debug, Clone)]
pub struct SrcInfo {
    pub duration: Timestamp,
}

#[derive(Debug)]
pub struct PipeInitiator {
    pub src: Arc<Mutex<Option<SrcInfo>>>,
    pub pipe: Pipeline,
    pub offset: f64,
    pub video_sink: String,
}

#[derive(Debug)]
pub struct PipeState<VC> {
    /// Stores cached information at the source, and is filled once the source
    /// is loarded
    pub src: Arc<Mutex<Option<SrcInfo>>>,

    /// The raw gstreamer pipelien
    pub pipe: Pipeline,
    /// The start offset time for the video
    pub offset: f64,
    /// Any extra context which is stored about the state
    pub context: VC,

    /// The name of the sink element created while building the pipeline
    pub video_sink: String,
}

impl<VC> Drop for PipeState<VC> {
    fn drop(&mut self) {
        self.close().expect("Failed to close pipeline");
    }
}

impl<VC> PipeState<VC> {
    /// Creates a new pipeline state
    pub fn new(init: PipeInitiator, context: VC) -> Result<Self, glib::Error> {
        let state = Self {
            src: init.src,
            video_sink: init.video_sink,
            pipe: init.pipe,
            offset: init.offset, // TODO: Test if this is necessary
            context,
        };

        state.pause()?;

        if init.offset > 0.0 {
            state.seek_accurate(init.offset)?;
        }

        Ok(state)
    }

    /// Sets the pipeline to the [gst::State::Paused] state
    pub fn pause(&self) -> Result<(), glib::Error> {
        self.pipe.pause()
    }

    /// Sets the pipeline to the [gst::State::Playing] state
    pub fn play(&self) -> Result<(), glib::Error> {
        self.pipe.play()
    }

    /// Sets the pipeline to the [gst::State::Null] state
    ///
    /// This is required to stop any memory leaks when the pipeline ends
    pub(super) fn close(&self) -> Result<(), glib::Error> {
        self.pipe.close()
    }

    /// Seek to the given position in the file, passing the 'accurate' flag to gstreamer.
    /// If you want to make large jumps in a video file this may be faster than setting a
    /// very low framerate (because with a low framerate, gstreamer still decodes every frame).
    pub fn seek_accurate(&self, time: f64) -> Result<(), glib::Error> {
        self.pipe.seek_accurate(time)
    }

    /// Returns an [AppSink] from the stored information about the sink.
    /// This is assumed to never fail, relying on the setup to be correct
    pub fn get_sink(&self) -> AppSink {
        self.pipe
            .raw()
            .by_name(&self.video_sink)
            .expect("Sink element not found")
            .downcast::<gst_app::AppSink>()
            .expect("Sink element is expected to be an appsink!")
    }

    /// Returns a [gst::Bus] for the current pipeline
    pub fn bus(&self) -> gst::Bus {
        self.pipe
            .raw()
            .bus()
            .expect("Failed to get pipeline from bus. Shouldn't happen!")
    }

    /// Sets the `sync` property in the `sink` to be false so that we
    /// go through the frames as fast as possible and returns the value
    /// it was set to
    pub fn set_clock_sync(&self, val: bool) {
        let appsink = self
            .pipe
            .raw()
            .by_name(&self.video_sink)
            .expect("Sink element not found")
            .downcast::<gst_app::AppSink>()
            .expect("Sink element is expected to be an appsink!");

        appsink.set_property("sync", val);
    }

    /// Sets the `sync` property in the `sink` to be false so that we
    /// go through the frames as fast as possible and returns the value
    /// it was set to
    pub fn unsync_clock(&self) -> bool {
        let appsink = self
            .pipe
            .raw()
            .by_name(&self.video_sink)
            .expect("Sink element not found")
            .downcast::<gst_app::AppSink>()
            .expect("Sink element is expected to be an appsink!");

        let sync = appsink.property("sync");
        appsink.set_property("sync", false);

        sync
    }
}

/// This stores specific information about the state of the current frame
#[derive(Debug)]
pub struct FrameManager<FC> {
    /// The relative index to the start of the video itself
    pub idx: usize,
    /// The number of frames which could be shown within the next millisecond
    /// (the minimum unit for [Timestamp])
    pub excess_frames: usize,
    /// The raw frame information
    pub raw: Frame,
    /// The timestamp which this frame appears at, calculated from the start
    /// offset and the index itself
    pub timestamp: Timestamp,
    /// Any extra context about the frame required
    pub context: FC,
}

impl<FC> FrameManager<FC> {
    pub fn new(
        idx: usize,
        frame: Result<Frame, StreamError>,
        context: FC,
        offset: f64,
    ) -> Result<Self, StreamError> {
        frame.map(|frame| {
            let (timestamp, excess_frames) = Timestamp::from_frames(idx, frame.fps(), offset);

            Self {
                idx,
                excess_frames,
                raw: frame,
                timestamp,
                context,
            }
        })
    }

    /// Returns the width and height of the frame as a [Vec2u]
    pub fn size(&self) -> Vec2u {
        Vec2u::new(self.raw.width(), self.raw.height())
    }
}

#[cfg(feature = "signing")]
pub mod sign {
    //! Contains specific implementation of [super::PipeManager] which is for
    //! signing

    use std::{collections::VecDeque, future::Future, marker::PhantomData, pin::Pin};

    use futures::{stream, Stream, StreamExt, TryFutureExt};
    use identity_iota::storage::JwkStorageDocumentError;

    use crate::{
        file::SignedInterval,
        video::{sign::Controller, ChunkSigner, Signer, SigningError},
    };

    use super::*;

    /// This stores the extra information that we need when signing frames, as
    /// it doesn't change it is stored in the [PipeState]
    pub struct SigningContext<S, C>
    where
        S: Signer + 'static,
        C: Controller<S>,
    {
        /// The function that is given by the user to get the [ChunkSigner]s
        /// which then determine when and how a chunk should be signed
        pub controller: C,
        _phatom: PhantomData<S>,
    }

    impl<S, C> SigningContext<S, C>
    where
        S: Signer + 'static,
        C: Controller<S>,
    {
        pub fn new(controller: C) -> Self {
            Self {
                controller,
                _phatom: PhantomData,
            }
        }
    }

    /// This is the extra context which is used by [FrameState] and stores a
    /// cache of all the previous frames before it
    type MyFrameBuffer = Box<[Frame]>;
    impl FrameBuffer for MyFrameBuffer {
        fn with_frames<'a>(
            &'a self,
            range: Range<usize>,
        ) -> Box<dyn Iterator<Item = &'a Frame> + 'a> {
            Box::new(self[range].iter())
        }
    }

    /// This is the specific implementation of the [PipeManager] used by the
    /// signing process
    pub type Manager<S, C> = PipeManager<SigningContext<S, C>, MyFrameBuffer>;
    type MyPipeState<S, C> = Arc<PipeState<SigningContext<S, C>>>;

    type StatesPair<S, C> = (MyPipeState<S, C>, Result<Frame, glib::Error>);
    type EnumeratedStatesPair<S, C> = (usize, StatesPair<S, C>);
    type SigningItem<S, C> = (EnumeratedStatesPair<S, C>, Arc<Mutex<VecDeque<Frame>>>);

    /// Makes it easily create a manager from an iterator, it is done as such
    /// mostly to make it easier to integrate into existing solutions e.g.
    ///
    /// TODO: Write an example pls
    pub async fn manage<S, C>(
        ((i, (state, frame)), buffer): SigningItem<S, C>,
    ) -> Result<Manager<S, C>, SigningError>
    where
        S: Signer + 'static,
        C: Controller<S>,
    {
        let mut buffer = buffer.lock().await;

        // NOTE: We are technically not too bothered about exactly how many frames
        // are stored, therefore to make it easier, we will just use VecDeque::capacity,
        // even tho it might be greater than the expected number
        if buffer.len() == buffer.capacity() {
            buffer.pop_front();
        }

        if let Ok(frame) = frame.as_ref() {
            buffer.push_back(frame.clone());
        }

        // I think cloning the buffer might be a bit slow... but unsure what
        // is a better method D:
        let mut frames = Vec::new();
        frames.reserve_exact(buffer.len());

        buffer.iter().cloned().for_each(|it| frames.push(it));

        Ok(PipeManager::new(
            state,
            (i, frame, frames.into_boxed_slice()),
        )?)
    }

    impl FrameManager<MyFrameBuffer> {
        /// This calculates the starting index for the frame at a given
        /// timestamp relative to the [MyFrameBuffer]
        fn get_chunk_start(
            &self,
            start: Timestamp,
            start_offset: f64,
        ) -> Result<usize, SigOperationError> {
            self.context
                .len()
                .checked_sub(
                    self.idx - self.excess_frames - start.into_frames(self.raw.fps(), start_offset),
                )
                .ok_or(SigOperationError::OutOfRange(start, self.timestamp))
        }

        /// This gets the buffer the [ChunkSigner] is wanting and calls [ChunkSigner::sign]
        ///
        /// This returns a result of the future so that the signing process can be separated
        /// off onto another thread if needed by the caller
        pub fn sign<S>(
            &self,
            signer: ChunkSigner<S>,
            start_offset: f64,
        ) -> Result<
            impl Future<Output = Result<SignedInterval, JwkStorageDocumentError>>,
            SigOperationError,
        >
        where
            S: Signer + 'static,
        {
            let length: usize = (self.timestamp - signer.start).into();
            if !(MIN_CHUNK_LENGTH..=MAX_CHUNK_LENGTH).contains(&length) {
                return Err(SigOperationError::InvalidChunkSize(length));
            }

            let start_idx = self.get_chunk_start(signer.start, start_offset)?;
            let default_size = self.size();

            let buf = self.context.get_cropped_buffer(
                signer.pos.unwrap_or_default(),
                signer.size.unwrap_or(default_size),
                start_idx..self.context.len(),
            )?;

            let start = signer.start;
            let end = self.timestamp;

            Ok(signer
                .sign(buf, default_size)
                .and_then(move |res| async move { Ok(SignedInterval::new(start, end, res)) }))
        }
    }

    impl<S, C> Manager<S, C>
    where
        S: Signer + 'static,
        C: Controller<S> + 'static,
    {
        /// This calls the `sign_with` function stored in [SigningContext] and returns the result
        /// as an iterator
        pub async fn request_sign_info(&self) -> Vec<ChunkSigner<S>> {
            self.state
                .context
                .controller
                .get_chunks(self.get_frame_state().await)
                .await
        }

        /// This performs the signing process by calling the `sign_with` function and signs
        /// each defined chunk, returning the [SignedInterval]s as a stream which can
        /// then be interpreted
        pub async fn request_chunks(
            self,
        ) -> Pin<Box<dyn Stream<Item = Result<SignedInterval, SigningError>> + Send>> {
            let sign_info = self.request_sign_info().await;

            let res = stream::iter(
                sign_info
                    .into_iter()
                    .map(move |si| self.frame.sign(si, self.state.offset)),
            )
            .then(|res| async {
                match res {
                    Ok(fut) => fut.await.map_err(SigningError::from),
                    Err(e) => Err(e.into()),
                }
            });

            Box::pin(res)
        }
    }
}

#[cfg(feature = "verifying")]
pub mod verification {
    //! Contains specific implementation of [super::PipeManager] which is for
    //! verification

    use std::collections::HashMap;

    use futures::{stream, Stream, StreamExt};
    use identity_iota::{
        prelude::Resolver,
        verification::jws::{JwsAlgorithm, VerificationInput},
    };

    use crate::{file::SignedChunk, utils::CredentialStore, video::SignatureState, SignFile};

    use super::*;

    /// Due to the caching of [crate::utils::DelayedStream],
    /// the buffer stores both the idex and the [Frame].
    pub type FrameIdxPair = (usize, Result<Frame, glib::Error>);

    /// This is the specific implementation of the [PipeManager] used when
    /// verifying
    pub type Manager<'a> = PipeManager<SigVideoContext<'a>, FutureFramesContext>;

    /// This describes the ID we use to uniquely identify signed chunks
    type SigID<'a> = (Timestamp, &'a Vec<u8>);
    type SigCache<'a> = HashMap<SigID<'a>, SignatureState>;

    /// This stores the specific information about the Video that is needed
    /// when verifying
    pub struct SigVideoContext<'a> {
        /// The cache of the verified signatures with their states
        pub cache: Mutex<SigCache<'a>>,
        /// A cache of the credential/presentations that have been
        /// defined up to this current point
        pub credentials: Mutex<CredentialStore<'a>>,
        /// The sign file with all the signatures themselves
        pub signfile: &'a SignFile,
    }

    impl<'a> SigVideoContext<'a> {
        pub fn new(signfile: &'a SignFile, resolver: &'a Resolver) -> Self {
            Self {
                cache: Mutex::new(HashMap::new()),
                credentials: Mutex::new(CredentialStore::new(resolver)),
                signfile,
            }
        }
    }

    pub type FutureFramesContext = Box<[Arc<(usize, Result<Frame, glib::Error>)>]>;

    impl<'a> Manager<'a> {
        /// This will verify all the signatures for the current frame
        pub async fn verify_signatures(self) -> Vec<SignatureState> {
            self.sigs_iter().collect::<Vec<SignatureState>>().await
        }

        /// Iterates through all the signatures that apply to the current frame,
        /// and verifies it, outputing an iterator of the resultant [SignatureState]
        fn sigs_iter(self) -> impl Stream<Item = SignatureState> + 'a {
            let sigs = self
                .state
                .context
                .signfile
                .get_signatures_at(self.frame.timestamp);

            stream::iter(sigs.zip(std::iter::repeat(Arc::new(self)))).then(
                |(chunk, manager)| async move {
                    let cache_key = (chunk.range.start, &chunk.signature.signature);

                    let mut state_cache = manager.state.context.cache.lock().await;

                    // Due to us having to pass to the output of the iterator
                    let state = match state_cache.get(&cache_key) {
                        Some(s) => s.clone(),
                        None => {
                            let state = manager.verify_sig(&chunk).await;
                            state_cache.insert(cache_key, state.clone());
                            state
                        }
                    };

                    state
                },
            )
        }

        /// Verifies a single chunk
        async fn verify_sig(&self, chunk: &SignedChunk<'a>) -> SignatureState {
            let end_frame =
                self.convert_to_frames(chunk.range.end) - self.convert_to_frames(chunk.range.start);

            let sig = chunk.signature;

            let length: usize = (chunk.range.end - chunk.range.start).into();
            if !(MIN_CHUNK_LENGTH..=MAX_CHUNK_LENGTH).contains(&length) {
                return SignatureState::Invalid(SigOperationError::InvalidChunkSize(length).into());
            }

            let frames_buf = self
                .frame
                .get_cropped_buffer(sig.pos, sig.size, 0..end_frame - 1);
            let frames_buf = match frames_buf {
                Ok(b) => b,
                Err(e) => return SignatureState::Invalid(e.into()),
            };

            let mut credentials = self.state.context.credentials.lock().await;

            let signer = credentials.normalise(sig.presentation.clone()).await;

            SignatureState::from_signer(
                signer,
                VerificationInput {
                    alg: JwsAlgorithm::EdDSA,
                    signing_input: frames_buf.into_boxed_slice(),
                    decoded_signature: sig.signature.clone().into_boxed_slice(),
                },
            )
        }
    }

    impl FrameBuffer for FrameManager<FutureFramesContext> {
        fn with_frames<'a>(
            &'a self,
            range: Range<usize>,
        ) -> Box<dyn Iterator<Item = &'a Frame> + 'a> {
            Box::new(
                vec![&self.raw]
                    .into_iter()
                    .chain(self.context[range].iter().filter_map(|a| a.1.as_ref().ok())),
            )
        }
    }
}
