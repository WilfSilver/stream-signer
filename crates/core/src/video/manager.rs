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

use crate::{file::Timestamp, spec::Vec2u, utils::TimeRange};

use super::{
    FrameState, Framerate, Pipeline, SigOperationError, StreamError,
    frame::DecodedFrame,
    pipeline::{PipeInitiator, SrcInfo},
};

/// This trait is designed to make it easier to extract the exact bytes which
/// need encrypting from a stored buffer of [super::Frame]s
pub trait FrameBuffer {
    /// This should return the specific frames within the given range as an iterator,
    /// this is not done through a given trait of [std::ops::Range], to make it more flexible to
    /// the structures this can be applied to
    fn with_frames<'a>(
        &'a self,
        range: Range<usize>,
    ) -> Box<dyn Iterator<Item = &'a DecodedFrame<true>> + 'a>;

    /// This uses the [FrameBuffer::with_frames] to return the exact buffer which
    /// should be signed for a given position and size of the viewing window
    ///
    /// This is quite a slow function, all tactics to reduce this time have
    /// been tried, the fact is that cloning this amount of data into a vector
    /// is just quite slow
    ///
    /// We assume that all frames are the same size
    fn get_cropped_buffer(
        &self,
        pos: Vec2u,
        size: Vec2u,
        range: Range<usize>,
        channels: &[usize],
    ) -> Result<Vec<u8>, SigOperationError> {
        let frames_len = range.len();
        let mut frames = self.with_frames(range).peekable();

        let Some(first) = frames.peek() else {
            return Ok(Vec::new());
        };

        let frame_size = first.cropped_buffer_size(size, channels);

        let capacity = frame_size * frames_len;
        let mut frames_buf: Vec<u8> = Vec::new();
        frames_buf.reserve_exact(capacity);

        for f in frames {
            // We have to do it on a per frame basis due to the potential issue
            // of different frames being of different sizes
            let mut slice = vec![0; f.cropped_buffer_size(size, channels)];
            f.cropped_buffer(&mut slice, pos, size, channels)?;
            frames_buf.extend(slice);
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
#[derive(Debug)]
pub struct PipeManager<VC, FC> {
    /// The information stored about the frame itself
    pub frame: Arc<FrameManager<FC>>,
    /// The information about the pipes state and other information which was
    /// needed when building the pipeline
    pub state: Arc<PipeState<VC>>,
}

impl<VC, FC> Clone for PipeManager<VC, FC> {
    fn clone(&self) -> Self {
        Self {
            frame: self.frame.clone(),
            state: self.state.clone(),
        }
    }
}

impl<VC, FC> PipeManager<VC, FC> {
    /// Creates a new manager with the given `state` and `frame_info`
    ///
    /// For ease of use we generate the [FrameState] from the given tuple of
    /// information
    pub fn new(
        state: Arc<PipeState<VC>>,
        frame_info: (DecodedFrame<false>, FC),
    ) -> Result<Self, StreamError> {
        Ok(Self {
            frame: Arc::new(FrameManager::new(frame_info.0.check()?, frame_info.1)),
            state,
        })
    }

    /// Returns the framerate the video is expected to be playing at from its
    /// metadata
    pub fn fps(&self) -> Framerate<usize> {
        self.frame.raw.frame.fps()
    }

    /// This uses its information to convert a given [Timestamp] to a frame
    /// index relative to any buffers stored
    pub fn convert_to_frames(&self, time: Timestamp) -> f64 {
        time.into_frames(self.fps(), self.state.offset)
    }

    /// Collects the information throughout the [PipeManager] to produce a
    /// [FrameState] which can then be shared with the user
    pub async fn get_frame_state(&self) -> FrameState {
        let exact_time = self.frame.raw.frame.get_timestamp();

        FrameState {
            video: self
                .state
                .src
                .lock()
                .await
                .clone()
                .expect("Source information was not set"),
            info: self.frame.raw.clone(),
            is_last: self.frame.raw.is_last,
            pipe: self.state.pipe.clone(),
            frame_idx: self.frame.raw.idx,
            time: TimeRange::new(exact_time, self.fps().frame_time()),
            start_offset: self.state.offset,
        }
    }
}

/// Stores any information relating to the video as a whole and persists for
/// the whole video.
#[derive(Debug)]
pub struct PipeState<VC> {
    /// Stores cached information at the source, and is filled once the source
    /// is loarded
    pub src: Arc<Mutex<Option<SrcInfo>>>,

    /// The raw gstreamer pipelien
    pub pipe: Arc<Pipeline>,
    /// The start offset time for the video
    pub offset: Timestamp,
    /// Any extra context which is stored about the state
    pub context: VC,

    /// The name of the sink element for video created while building the pipeline
    pub video_sink: String,

    /// The name of the sink element for audio created while building the pipeline
    pub audio_sink: String,
}

impl<VC> Drop for PipeState<VC> {
    fn drop(&mut self) {
        self.close().expect("Failed to close pipeline");
    }
}

impl<VC> PipeState<VC> {
    /// Creates a new pipeline state with a custom context
    pub fn new(init: PipeInitiator, context: VC) -> Result<Self, glib::Error> {
        let state = Self {
            src: init.src,
            video_sink: init.video_sink,
            audio_sink: init.audio_sink,
            pipe: Arc::new(init.pipe),
            offset: init.offset,
            context,
        };

        state.pause()?;

        if init.offset > Timestamp::ZERO {
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

    /// Sets the pipeline to the [gst::State::Null] state, via [gst::State::Paused]
    /// and [gst::State::Ready] to help clear memory leaks
    ///
    /// This is required to stop any memory leaks when the pipeline ends
    pub(super) fn close(&self) -> Result<(), glib::Error> {
        self.pipe.close()
    }

    /// Seek to the given position in the file, passing the 'accurate' flag to gstreamer.
    /// If you want to make large jumps in a video file this may be faster than setting a
    /// very low framerate (because with a low framerate, gstreamer still decodes every frame).
    pub fn seek_accurate(&self, time: Timestamp) -> Result<(), glib::Error> {
        self.pipe.seek_accurate(time)
    }

    /// Returns an [AppSink] from the stored information about the sink.
    /// This is assumed to never fail, relying on the setup to be correct
    pub fn get_video_sink(&self) -> AppSink {
        self.pipe
            .raw()
            .by_name(&self.video_sink)
            .expect("Video element not found")
            .downcast::<gst_app::AppSink>()
            .expect("Sink element is expected to be an appsink!")
    }

    /// Returns an [AppSink] from the stored information about the sink.
    /// This is assumed to never fail, relying on the setup to be correct
    ///
    /// Unlike the video sink, we don't always assume the audio sink will
    /// exist as it is only added when needed
    pub fn get_audio_sink(&self) -> Option<AppSink> {
        self.pipe
            .raw()
            .by_name(&self.audio_sink)
            .map(Cast::downcast::<gst_app::AppSink>)
            .map(|e| e.expect("Sink element is expected to be an appsink!"))
    }

    /// Returns a [gst::Bus] for the current pipeline
    pub fn bus(&self) -> gst::Bus {
        self.pipe
            .raw()
            .bus()
            .expect("Failed to get pipeline from bus. Shouldn't happen!")
    }

    /// Sets the `sync` property in the `sink` to be the given value returning
    /// the old value
    pub fn set_clock_sync(&self, val: bool) -> bool {
        let video_sink = self.get_video_sink();

        let sync = video_sink.property("sync");
        video_sink.set_property("sync", val);

        if let Some(audio_sink) = self.get_audio_sink() {
            audio_sink.set_property("sync", val);
        }

        sync
    }

    /// Sets the `sync` property in the `sink` to be false so that we
    /// go through the frames as fast as possible and returns the value
    /// it was set to
    pub fn unsync_clock(&self) -> bool {
        self.set_clock_sync(false)
    }
}

/// This stores specific information about the state of the current frame
#[derive(Debug)]
pub struct FrameManager<FC> {
    /// The raw frame information
    pub raw: DecodedFrame<true>,

    /// The timestamp which this frame appears at, calculated from the start
    /// offset and the index itself
    pub time: TimeRange,
    /// Any extra context about the frame required
    pub context: FC,
}

impl<FC> FrameManager<FC> {
    pub fn new(frame: DecodedFrame<true>, context: FC) -> Self {
        let timestamp = frame.frame.get_timestamp();
        let time = TimeRange::new(timestamp, frame.frame.get_duration());

        Self {
            raw: frame,
            time,
            context,
        }
    }

    /// Returns the width and height of the frame as a [Vec2u]
    pub fn size(&self) -> Vec2u {
        self.raw.frame.size()
    }
}

#[cfg(feature = "signing")]
pub mod sign {
    //! Contains specific implementation of [super::PipeManager] which is for
    //! signing

    use std::{collections::VecDeque, future::Future, marker::PhantomData, pin::Pin};

    use futures::{Stream, StreamExt, TryFutureExt, stream};
    use identity_iota::storage::JwkStorageDocumentError;

    use crate::{
        file::SignedInterval,
        spec::CHUNK_LENGTH_RANGE,
        video::{
            ChunkSigner, Signer, SigningError, StreamError,
            frame::FrameWithAudio,
            sign::{ChunkSignerBuilder, Controller},
        },
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
    type MyFrameBuffer = Box<[DecodedFrame<true>]>;
    impl FrameBuffer for MyFrameBuffer {
        fn with_frames<'a>(
            &'a self,
            range: Range<usize>,
        ) -> Box<dyn Iterator<Item = &'a DecodedFrame<true>> + 'a> {
            Box::new(self[range].iter())
        }
    }

    /// This is the specific implementation of the [PipeManager] used by the
    /// signing process
    pub type Manager<S, C> = PipeManager<SigningContext<S, C>, MyFrameBuffer>;
    type MyPipeState<S, C> = Arc<PipeState<SigningContext<S, C>>>;

    type StatesPair<S, C> = (MyPipeState<S, C>, Result<FrameWithAudio, StreamError>);
    type SigningItem<S, C> = (StatesPair<S, C>, Arc<Mutex<VecDeque<DecodedFrame<true>>>>);

    /// Makes it easily create a manager from an iterator, it is done as such
    /// mostly to make it easier to integrate into existing solutions e.g.
    pub async fn manage<S, C>(
        ((state, frame), buffer): SigningItem<S, C>,
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

        // NOTE: we don't use the frame later on in our calculations
        let frame: DecodedFrame<false> = frame.into();
        if let Ok(frame) = frame.clone().check() {
            buffer.push_back(frame.clone());
        }

        // I think cloning the buffer might be a bit slow... but unsure what
        // is a better method D:
        let mut frames = Vec::new();
        frames.reserve_exact(buffer.len());

        for f in buffer.iter() {
            frames.push(f.clone());
        }

        Ok(PipeManager::new(state, (frame, frames.into_boxed_slice()))?)
    }

    impl FrameManager<MyFrameBuffer> {
        /// This calculates the starting index for the frame at a given
        /// timestamp relative to the [MyFrameBuffer]
        fn get_chunk_start(&self, start: Timestamp) -> Result<usize, SigOperationError> {
            let context_start = self.context[0].frame.get_timestamp();
            let Some(rel_time) = start.floor_millis().checked_sub(*context_start) else {
                return Err(SigOperationError::OutOfRange(start, self.time.end()));
            };
            Ok(Timestamp::from(rel_time)
                .into_frames(self.raw.frame.fps(), Timestamp::ZERO)
                .ceil() as usize)
        }

        /// This gets the buffer the [ChunkSigner] is wanting and calls [ChunkSigner::sign]
        ///
        /// This returns a result of the future so that the signing process can be separated
        /// off onto another thread if needed by the caller
        pub fn sign<S>(
            &self,
            signer: ChunkSigner<S>,
            start_offset: Timestamp,
        ) -> Result<
            impl Future<Output = Result<SignedInterval, JwkStorageDocumentError>>,
            SigOperationError,
        >
        where
            S: Signer + 'static,
        {
            // Floor the millis here with the length as that is what the SRT
            // file will do
            let Some(length) = self.time.end().floor_millis().checked_sub(*signer.start) else {
                return Err(SigOperationError::OutOfRange(signer.start, self.time.end()));
            };

            if !CHUNK_LENGTH_RANGE.contains(&length) {
                return Err(SigOperationError::InvalidChunkSize(length));
            } else if signer.start < start_offset {
                return Err(SigOperationError::OutOfRange(signer.start, self.time.end()));
            }

            let start_idx = self.get_chunk_start(signer.start)?;
            let end = self.context.len() - self.time.end().excess_frames(self.raw.frame.fps());

            let buf = self.context.get_cropped_buffer(
                signer.pos,
                signer.size,
                start_idx..end,
                &signer.channels,
            )?;

            let start = signer.start;
            // We want to end the chunk at the end of this frame
            let end = self.time.end();

            Ok(signer
                .sign(buf)
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
        pub async fn request_sign_info(&self) -> Vec<ChunkSignerBuilder<S>> {
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

            let frame = self.frame;
            let offset = self.state.offset;
            let res = stream::iter(sign_info).then(move |si| {
                let frame = frame.clone();
                async move {
                    match frame.sign(si.build(&frame.raw), offset) {
                        Ok(fut) => fut.await.map_err(SigningError::from),
                        Err(e) => Err(e.into()),
                    }
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

    use futures::{Stream, StreamExt, stream};
    use identity_iota::{
        prelude::Resolver,
        verification::jws::{JwsAlgorithm, VerificationInput},
    };

    use crate::{
        SignFile,
        file::SignedInterval,
        spec::CHUNK_LENGTH_RANGE,
        utils::CredentialStore,
        video::{Frame, verify::SignatureState},
    };

    use super::*;

    /// Due to the caching of [crate::utils::DelayedStream],
    /// the buffer stores both the idex and the [Frame].
    pub type FrameIdxPair = (usize, Result<Frame, glib::Error>);

    /// This is the specific implementation of the [PipeManager] used when
    /// verifying
    pub type Manager = PipeManager<SigVideoContext, FutureFramesContext>;

    /// This describes the ID we use to uniquely identify signed chunks
    type SigID = (Timestamp, usize);
    type SigCache = HashMap<SigID, SignatureState>;

    /// This stores the specific information about the Video that is needed
    /// when verifying
    pub struct SigVideoContext {
        /// The cache of the verified signatures with their states
        pub cache: Mutex<SigCache>,
        /// A cache of the credential/presentations that have been
        /// defined up to this current point
        pub credentials: Mutex<CredentialStore>,
        /// The sign file with all the signatures themselves
        pub signfile: SignFile,
    }

    impl SigVideoContext {
        pub fn new(signfile: SignFile, resolver: Arc<Resolver>) -> Self {
            Self {
                cache: Mutex::new(HashMap::new()),
                credentials: Mutex::new(CredentialStore::new(resolver)),
                signfile,
            }
        }
    }

    pub type FutureFramesContext = Box<[DecodedFrame<false>]>;

    impl Manager {
        /// This will verify all the signatures for the current frame
        pub async fn verify_signatures(self) -> Vec<SignatureState> {
            self.sigs_iter().collect::<Vec<SignatureState>>().await
        }

        /// Iterates through all the signatures that apply to the current frame,
        /// and verifies it, outputing an iterator of the resultant [SignatureState]
        fn sigs_iter(&self) -> impl Stream<Item = SignatureState> + '_ {
            let sigs = self
                .state
                .context
                .signfile
                .get_signatures_at(self.frame.time.start());

            stream::iter(sigs.zip(std::iter::repeat(self))).then(|(chunk, manager)| async move {
                let cache_key = (
                    chunk.range.start,
                    chunk.signature.signature.as_ptr() as usize,
                );

                // Query the cache without locking it for longer than
                // required
                let mut cache = manager.state.context.cache.lock().await;

                // Due to us having to pass to the output of the iterator
                let state = match cache.get(&cache_key) {
                    Some(s) => s.clone(),
                    None => {
                        // Temporarily set to loading before we spin off
                        // another thread
                        cache.insert(cache_key, SignatureState::Loading);

                        // Validate the signature on a separate thread
                        // so it doesn't affect the video itself
                        let m = manager.clone();
                        let interval: SignedInterval = chunk.into();
                        tokio::spawn(async move {
                            let state = m.verify_sig(interval).await;
                            let mut cache = m.state.context.cache.lock().await;
                            cache.insert(cache_key, state);
                        });

                        SignatureState::Loading
                    }
                };

                state
            })
        }

        /// Verifies a single chunk
        async fn verify_sig(&self, chunk: SignedInterval) -> SignatureState {
            // The end frame, we want to ceiling it, to be consistent with the
            // floor when getting excess frames
            let start_idx = chunk
                .start()
                .into_frames(self.fps(), self.state.offset)
                .ceil() as usize;
            let end_idx = chunk
                .stop()
                .into_frames(self.fps(), self.state.offset)
                .ceil() as usize;
            let Some(end_frame) = end_idx.checked_sub(start_idx) else {
                return SignatureState::Invalid(
                    SigOperationError::OutOfRange(chunk.start(), chunk.stop()).into(),
                );
            };
            // (chunk.stop() - *chunk.start()).into_frames(self.fps(), Timestamp::ZERO);
            // let end_frame = end_frame.round() as usize;

            let sig = &chunk.val;

            // Save the credential first before doing any checking
            let mut credentials = self.state.context.credentials.lock().await;
            let signer = credentials.normalise(sig.presentation.clone()).await;

            let length = chunk.stop() - chunk.start();
            if !CHUNK_LENGTH_RANGE.contains(&length) {
                return SignatureState::Invalid(SigOperationError::InvalidChunkSize(length).into());
            } else if chunk.start() < self.state.offset {
                return SignatureState::Invalid(
                    SigOperationError::OutOfRange(chunk.start(), chunk.stop()).into(),
                );
            }

            let frames_buf =
                self.frame
                    .get_cropped_buffer(sig.pos, sig.size, 0..end_frame, &sig.channels);

            let frames_buf = match frames_buf {
                Ok(b) => b,
                Err(e) => return SignatureState::Invalid(e.into()),
            };

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
        ) -> Box<dyn Iterator<Item = &'a DecodedFrame<true>> + 'a> {
            if range.start == 0 {
                Box::new(
                    vec![&self.raw].into_iter().chain(
                        // Minus one from the range as we are adding one self.raw
                        self.context[0..range.end - 1]
                            .iter()
                            .filter_map(|a| a.check_ref().ok()),
                    ),
                )
            } else {
                // Minus one from the indexes as technically we hare self.raw
                // as the first
                Box::new(
                    self.context[range.start - 1..range.end - 1]
                        .iter()
                        .filter_map(|a| a.check_ref().ok()),
                )
            }
        }
    }
}
