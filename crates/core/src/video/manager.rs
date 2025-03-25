use std::{ops::Range, sync::Arc};

use glib::object::Cast;
use gst::{
    prelude::{ElementExt, ElementExtManual, GstBinExt},
    ClockTime, CoreError, MessageView, Pipeline, SeekFlags, StateChangeSuccess,
};
use gst_app::AppSink;
use tokio::sync::Mutex;

use crate::{file::Timestamp, spec::Coord};

use super::{Frame, FrameError, Framerate};

pub trait FrameBuffer {
    fn with_frames<'a>(&'a self, range: Range<usize>) -> Box<dyn Iterator<Item = &'a Frame> + 'a>;

    fn get_cropped_buffer(
        &self,
        pos: Coord,
        size: Coord,
        range: Range<usize>,
    ) -> Result<Vec<u8>, FrameError> {
        // TODO: Add audio
        // TODO: I believe this to be slightly broken and so needs heavy testing!!
        let capacity = 3 * size.x as usize * size.y as usize * range.len();
        let mut frames_buf: Vec<u8> = Vec::new();
        frames_buf.reserve_exact(capacity);

        for f in self.with_frames(range) {
            frames_buf.extend(f.cropped_buffer(pos, size)?);
        }

        Ok(frames_buf)
    }
}

pub trait FrameContext {}
pub trait VideoContext {}
impl VideoContext for () {}

// TODO: swap round the defintions please
#[derive(Debug)]
pub struct PipeManager<FC: FrameContext, VC: VideoContext> {
    pub frame: Arc<FrameState<FC>>,
    pub state: Arc<PipeState<VC>>,
}

impl<FC: FrameContext, VC: VideoContext> PipeManager<FC, VC> {
    pub fn new(
        state: Arc<PipeState<VC>>,
        frame_info: (usize, Result<Frame, glib::Error>, FC),
    ) -> Result<Self, FrameError> {
        let offset = state.offset;

        Ok(Self {
            frame: Arc::new(FrameState::new(
                frame_info.0,
                frame_info.1,
                frame_info.2,
                offset,
            )?),
            state,
        })
    }

    pub fn fps(&self) -> Framerate<usize> {
        self.frame.raw.fps()
    }

    fn convert_to_frames(&self, time: Timestamp) -> usize {
        time.into_frames(self.fps(), self.state.offset.into())
    }
}

#[derive(Debug)]
pub struct PipeState<VC: VideoContext> {
    pub pipe: Pipeline,
    pub offset: f64,
    pub context: VC,

    /// The sink element created
    pub sink: String,
}

impl<VC: VideoContext> PipeState<VC> {
    pub fn new<S: ToString>(
        pipe: Pipeline,
        sink: S,
        offset: Option<f64>,
        context: VC,
    ) -> Result<Self, glib::Error> {
        let state = Self {
            sink: sink.to_string(),
            pipe,
            offset: offset.unwrap_or_default(), // TODO: Test if this is necessary
            context,
        };

        state.pause()?;

        if let Some(skip_amount) = offset {
            state.seek_accurate(skip_amount)?;
        }

        Ok(state)
    }

    /// Sets the pipeline to the [gst::State::Paused] state
    pub fn pause(&self) -> Result<(), glib::Error> {
        change_state_blocking(&self.pipe, gst::State::Paused)
    }

    /// Sets the pipeline to the [gst::State::Playing] state
    pub fn play(&self) -> Result<(), glib::Error> {
        change_state_blocking(&self.pipe, gst::State::Playing)
    }

    /// Sets the pipeline to the [gst::State::Null] state
    ///
    /// This is required to stop any memory leaks when the pipeline ends
    fn close(&self) -> Result<(), glib::Error> {
        change_state_blocking(&self.pipe, gst::State::Null)
    }

    /// Seek to the given position in the file, passing the 'accurate' flag to gstreamer.
    /// If you want to make large jumps in a video file this may be faster than setting a
    /// very low framerate (because with a low framerate, gstreamer still decodes every frame).
    pub fn seek_accurate(&self, time: f64) -> Result<(), glib::Error> {
        let time_ns_f64 = time * ClockTime::SECOND.nseconds() as f64;
        let time_ns_u64 = time_ns_f64 as u64;
        let flags = SeekFlags::ACCURATE.union(SeekFlags::FLUSH);

        self.pipe
            .seek_simple(flags, gst::ClockTime::from_nseconds(time_ns_u64))
            .map_err(|e| glib::Error::new(CoreError::TooLazy, &e.message))
    }

    /// Returns an [AppSink] from the stored information about the sink.
    /// This is assumed to never fail, relying on the setup to be correct
    pub fn get_sink(&self) -> AppSink {
        self.pipe
            .by_name(&self.sink)
            .expect("Sink element not found")
            .downcast::<gst_app::AppSink>()
            .expect("Sink element is expected to be an appsink!")
    }

    /// Returns a [gst::Bus] for the current pipeline
    pub fn bus(&self) -> gst::Bus {
        self.pipe
            .bus()
            .expect("Failed to get pipeline from bus. Shouldn't happen!")
    }
}

#[derive(Debug)]
pub struct FrameState<FC: FrameContext> {
    pub idx: usize,
    pub excess_frames: usize,
    pub raw: Frame,
    pub timestamp: Timestamp,
    pub context: FC,
}

impl<FC: FrameContext> FrameState<FC> {
    pub fn new(
        idx: usize,
        frame: Result<Frame, glib::Error>,
        context: FC,
        offset: f64,
    ) -> Result<Self, FrameError> {
        let frame: Frame = match frame {
            Ok(s) => s.into(),
            Err(e) => return Err(e.clone().into()),
        };

        let (timestamp, excess_frames) = Timestamp::from_frames(idx, frame.fps(), offset.into());

        Ok(Self {
            idx,
            excess_frames,
            raw: frame,
            timestamp,
            context,
        })
    }

    pub fn size(&self) -> Coord {
        Coord::new(self.raw.width(), self.raw.height())
    }
}

#[cfg(feature = "signing")]
pub mod sign {
    //! Contains specific implementation of [super::PipeManager] which is for
    //! signing

    use std::{
        collections::{HashMap, VecDeque},
        future::Future,
        pin::Pin,
    };

    use futures::{stream, Stream, StreamExt};
    use identity_iota::storage::JwkStorageDocumentError;
    use tokio::task::JoinSet;

    use crate::{
        file::SignedChunk,
        spec::SignatureInfo,
        video::{ChunkSigner, FrameInfo, Signer},
    };

    use super::*;

    pub struct SigningContext<S, F, ITER>
    where
        S: Signer + 'static,
        F: FnMut(FrameInfo) -> ITER,
        ITER: IntoIterator<Item = ChunkSigner<S>>,
    {
        pub sign_with: Mutex<F>,
    }

    impl<S, F, ITER> VideoContext for SigningContext<S, F, ITER>
    where
        S: Signer + 'static,
        F: FnMut(FrameInfo) -> ITER,
        ITER: IntoIterator<Item = ChunkSigner<S>>,
    {
    }

    type MyFrameBuffer = Box<[Frame]>;
    impl FrameBuffer for MyFrameBuffer {
        fn with_frames<'a>(
            &'a self,
            range: Range<usize>,
        ) -> Box<dyn Iterator<Item = &'a Frame> + 'a> {
            Box::new(self[range].iter())
        }
    }
    impl FrameContext for MyFrameBuffer {}

    pub type Manager<S, F, ITER> = PipeManager<MyFrameBuffer, SigningContext<S, F, ITER>>;
    type MyPipeState<S, F, ITER> = Arc<PipeState<SigningContext<S, F, ITER>>>;

    /// Makes it easily create a manager from an iterator, it is done as such
    /// mostly to make it easier to integrate into existing solutions e.g.
    ///
    /// TODO: Write an example pls
    pub async fn manage<S, F, ITER>(
        ((i, (state, frame)), buffer): (
            (usize, (MyPipeState<S, F, ITER>, Result<Frame, glib::Error>)),
            Arc<Mutex<VecDeque<Frame>>>,
        ),
    ) -> Result<Manager<S, F, ITER>, FrameError>
    where
        S: Signer + 'static,
        F: FnMut(FrameInfo) -> ITER,
        ITER: IntoIterator<Item = ChunkSigner<S>>,
    {
        let mut buffer = buffer.lock().await;
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

        PipeManager::new(state, (i, frame, frames.into_boxed_slice()))
    }

    impl FrameState<MyFrameBuffer> {
        fn get_chunk_start(
            &self,
            start: Timestamp,
            start_offset: f64,
        ) -> Result<usize, FrameError> {
            self.context
                .len()
                .checked_sub(
                    self.idx
                        - self.excess_frames
                        - start.into_frames(self.raw.fps(), start_offset.into()),
                )
                .ok_or(FrameError::OutOfRange(start, self.timestamp))
        }

        pub fn sign<S>(
            &self,
            signer: ChunkSigner<S>,
            start_offset: f64,
        ) -> Result<impl Future<Output = Result<SignatureInfo, JwkStorageDocumentError>>, FrameError>
        where
            S: Signer + 'static,
        {
            let start_idx = self.get_chunk_start(signer.start, start_offset)?;
            let default_size = self.size();

            let buf = self.context.get_cropped_buffer(
                signer.pos.unwrap_or_default(),
                signer.size.unwrap_or(default_size),
                start_idx..self.context.len(),
            )?;

            Ok(signer.sign(buf, default_size))
        }
    }

    impl<S, F, ITER> Manager<S, F, ITER>
    where
        S: Signer + 'static,
        F: FnMut(FrameInfo) -> ITER,
        ITER: IntoIterator<Item = ChunkSigner<S>>,
    {
        pub async fn request_sign_info(&self) -> impl Iterator<Item = ChunkSigner<S>> {
            let fps = self.fps();

            let mut sign_with = self.state.context.sign_with.lock().await;
            let sign_info = sign_with(FrameInfo::new(
                self.frame.raw.clone(),
                self.frame.timestamp,
                self.frame.idx,
                fps,
            ));

            sign_info.into_iter()
        }

        pub async fn request_chunks(
            self,
        ) -> Pin<Box<dyn Stream<Item = Result<SignedChunk, FrameError>> + Send>> {
            let sign_info = self.request_sign_info().await;

            type SigInfoReturn = Result<SignatureInfo, JwkStorageDocumentError>;

            let mut chunks: HashMap<Timestamp, JoinSet<SigInfoReturn>> = HashMap::new();
            for si in sign_info.into_iter() {
                // TODO: Add protections if the timeframe is too short/long
                let start = si.start;

                let fut = self.frame.sign(si, self.state.offset);

                let fut = match fut {
                    Ok(f) => f,
                    Err(e) => return Box::pin(stream::iter([Err(e)])),
                };

                match chunks.get_mut(&start) {
                    Some(c) => {
                        c.spawn(fut);
                    }
                    None => {
                        let mut sigs = JoinSet::new();
                        sigs.spawn(fut);
                        chunks.insert(start, sigs);
                    }
                }
            }

            let timestamp = self.frame.timestamp;
            let res = stream::iter(chunks).then(move |(start, futures)| async move {
                futures.join_all().await.into_iter().try_fold(
                    SignedChunk::new(start, timestamp, vec![]),
                    |mut curr, signature| match signature {
                        Ok(sig) => {
                            curr.val.push(sig);
                            Ok(curr)
                        }
                        Err(e) => Err(e.into()),
                    },
                )
            });

            Box::pin(res)
        }
    }
}

#[cfg(feature = "verifying")]
pub mod verification {
    //! Contains specific implementation of [super::PipeManager] which is for
    //! verification

    use std::{collections::HashMap, future::Future};

    use futures::{stream, Stream, StreamExt};
    use identity_iota::{
        prelude::Resolver,
        verification::jws::{JwsAlgorithm, VerificationInput},
    };

    use crate::{
        video::{FrameInfo, SignatureState},
        CredentialStore, SignFile,
    };

    use super::*;

    pub type FrameIdxPair = (usize, Result<Frame, glib::Error>);

    pub type Manager<'a> = PipeManager<FutureFramesContext, SigVideoContext<'a>>;

    type SigID<'a> = (Timestamp, &'a Vec<u8>);
    type SigCache<'a> = HashMap<SigID<'a>, SignatureState>;

    pub struct SigVideoContext<'a> {
        pub cache: Mutex<SigCache<'a>>,
        pub credentials: Mutex<CredentialStore>,
        pub signfile: &'a SignFile,
    }

    impl<'a> SigVideoContext<'a> {
        pub fn new(signfile: &'a SignFile, resolver: Resolver) -> Self {
            Self {
                cache: Mutex::new(HashMap::new()),
                credentials: Mutex::new(CredentialStore::new(resolver)),
                signfile,
            }
        }
    }

    impl<'a> VideoContext for SigVideoContext<'a> {}

    pub type FutureFramesContext = Box<[Arc<(usize, Result<Frame, glib::Error>)>]>;

    impl FrameContext for FutureFramesContext {}

    impl<'a> Manager<'a> {
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
                .state
                .context
                .signfile
                .get_signatures_at(self.frame.timestamp);

            stream::iter(sigs.zip(std::iter::repeat(Arc::new(self)))).map(
                |(sig, manager)| async move {
                    let start = sig.range.start;
                    let end = sig.range.end;
                    let sig = sig.signature;
                    let cache_key = (start, &sig.signature);

                    let mut state_cache = manager.state.context.cache.lock().await;

                    // Due to us having to pass to the output of the iterator
                    let state = match state_cache.get(&cache_key) {
                        Some(s) => s.clone(),
                        None => {
                            // TODO: Check if the end frame is after 10 second mark
                            let end_frame =
                                manager.convert_to_frames(end) - manager.convert_to_frames(start);

                            let frames_buf = manager.frame.get_cropped_buffer(
                                sig.pos,
                                sig.size,
                                0..end_frame - 1,
                            )?;

                            let mut credentials = manager.state.context.credentials.lock().await;

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
                },
            )
        }
    }

    impl FrameBuffer for FrameState<FutureFramesContext> {
        fn with_frames<'a>(
            &'a self,
            range: Range<usize>,
        ) -> Box<dyn Iterator<Item = &'a Frame> + 'a> {
            Box::new(
                vec![&self.raw]
                    .into_iter()
                    .chain(self.context[range].iter().map(|a| {
                        // TODO: Check this
                        // It is safe to unwrap here due to the previous
                        // checks on the frames
                        let res = a.1.as_ref().unwrap();
                        res
                    })),
            )
        }
    }

    impl From<Arc<FrameState<FutureFramesContext>>> for FrameInfo {
        fn from(value: Arc<FrameState<FutureFramesContext>>) -> Self {
            FrameInfo::new(
                value.raw.clone(),
                value.timestamp,
                value.idx,
                value.raw.fps(),
            )
        }
    }
}

pub mod iter {
    //! This provides an easy interface to iterate over the [Frame]s in a pipeline
    //! with a state stored throughout
    //!
    //! This basically a cut down version of <https://github.com/Farmadupe/vid_dup_finder_lib/blob/main/vid_frame_iter>

    use std::{iter::FusedIterator, sync::Arc};

    use gst::{CoreError, MessageView};

    use super::*;

    #[derive(Debug)]
    pub struct SampleIter<VC: VideoContext> {
        /// The state of the iterator, storing key information about the
        /// pipeline
        pub state: Arc<PipeState<VC>>,

        // The amount of time to wait for a frame before assuming there are
        // none left.
        pub timeout: gst::ClockTime,

        /// Whether the last frame has been returned
        pub fused: bool,
    }

    pub type SampleWithState<VC> = (Arc<PipeState<VC>>, Result<Frame, glib::Error>);

    impl<VC: VideoContext> SampleIter<VC> {
        pub fn new<S: ToString>(
            pipeline: gst::Pipeline,
            sink: S,
            offset: Option<f64>,
            context: VC,
        ) -> Result<Self, glib::Error> {
            Ok(Self {
                state: Arc::new(PipeState::new(pipeline, sink, offset, context)?),
                timeout: 30 * gst::ClockTime::SECOND,
                fused: false,
            })
        }

        pub fn zip_state(self) -> impl Iterator<Item = SampleWithState<VC>> {
            let state = self.state.clone();
            std::iter::repeat(state).zip(self)
        }

        pub fn pause(&self) -> Result<(), glib::Error> {
            self.state.pause()
        }

        pub fn play(&self) -> Result<(), glib::Error> {
            self.state.play()
        }

        /// Seek to the given position in the file, passing the 'accurate' flag to gstreamer.
        /// If you want to make large jumps in a video file this may be faster than setting a
        /// very low framerate (because with a low framerate, gstreamer still decodes every frame).
        pub fn seek_accurate(&self, time: f64) -> Result<(), glib::Error> {
            self.state.seek_accurate(time)
        }

        fn try_find_error(bus: &gst::Bus) -> Option<glib::Error> {
            bus.pop_filtered(&[gst::MessageType::Error, gst::MessageType::Warning])
                .filter(|msg| matches!(msg.view(), MessageView::Error(_) | MessageView::Warning(_)))
                .map(into_glib_error)
        }
    }

    impl<VC: VideoContext> FusedIterator for SampleIter<VC> {}
    impl<VC: VideoContext> Iterator for SampleIter<VC> {
        type Item = Result<Frame, glib::Error>;

        fn next(&mut self) -> Option<Self::Item> {
            // Required for FusedIterator
            if self.fused {
                return None;
            }

            let bus = self.state.bus();

            // Get access to the appsink element.
            let appsink = self.state.get_sink();

            // If any error/warning occurred, then return it now.
            if let Some(error) = Self::try_find_error(&bus) {
                return Some(Err(error));
            }

            let sample = appsink.try_pull_sample(self.timeout);
            match sample {
                //If a frame was extracted then return it.
                Some(sample) => Some(Ok(sample.into())),

                None => {
                    // Make sure no more frames can be drawn if next is called again
                    self.fused = true;

                    //if no sample was returned then we might have hit the timeout.
                    //If so check for any possible error being written into the log
                    //at that time
                    let ret = match Self::try_find_error(&bus) {
                        Some(error) => Some(Err(error)),
                        _ => {
                            if !appsink.is_eos() {
                                Some(Err(glib::Error::new(
                                    CoreError::TooLazy,
                                    "Gstreamer timed out",
                                )))

                            // Otherwise we hit EOS and nothing else suspicious happened
                            } else {
                                None
                            }
                        }
                    };

                    if let Err(e) = self.state.close() {
                        panic!("{e:?}");
                    }

                    ret
                }
            }
        }
    }
}

fn change_state_blocking(
    pipeline: &gst::Pipeline,
    new_state: gst::State,
) -> Result<(), glib::Error> {
    let timeout = 10 * gst::ClockTime::SECOND;

    let state_change_error = match pipeline.set_state(new_state) {
        Ok(StateChangeSuccess::Success | StateChangeSuccess::NoPreroll) => return Ok(()),
        Ok(StateChangeSuccess::Async) => {
            let (result, _curr, _pending) = pipeline.state(timeout);
            match result {
                Ok(StateChangeSuccess::Success | StateChangeSuccess::NoPreroll) => return Ok(()),

                //state change failed within timeout. Treat as error
                Ok(StateChangeSuccess::Async) => None,
                Err(e) => Some(e),
            }
        }

        Err(e) => Some(e),
    };

    // If there was any error then return that.
    // If no error but timed out then say so.
    // If no error and no timeout then any report will do.
    let error: glib::Error =
        match get_bus_errors(&pipeline.bus().expect("failed to get gst bus")).next() {
            Some(e) => e,
            _ => {
                if let Some(_e) = state_change_error {
                    glib::Error::new(gst::CoreError::TooLazy, "Gstreamer State Change Error")
                } else {
                    glib::Error::new(gst::CoreError::TooLazy, "Internal Gstreamer error")
                }
            }
        };

    // Before returning, close down the pipeline to prevent memory leaks.
    // But if the pipeline can't close, cause a panic (preferable to memory leak)
    match change_state_blocking(pipeline, gst::State::Null) {
        Ok(()) => Err(error),
        Err(e) => panic!("{e:?}"),
    }
}

/// Drain all messages from the bus, keeping track of eos and error.
/// (This prevents messages piling up and causing memory leaks)
fn get_bus_errors(bus: &gst::Bus) -> impl Iterator<Item = glib::Error> + '_ {
    let errs_warns = [gst::MessageType::Error, gst::MessageType::Warning];

    std::iter::from_fn(move || bus.pop_filtered(&errs_warns).map(into_glib_error))
}

fn into_glib_error(msg: gst::Message) -> glib::Error {
    match msg.view() {
        MessageView::Error(e) => e.error(),
        MessageView::Warning(w) => w.error(),
        _ => {
            panic!("Only Warning and Error messages can be converted into GstreamerError")
        }
    }
}
