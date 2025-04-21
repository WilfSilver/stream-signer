use std::path::Path;

use crate::video::{iter::FrameIter, StreamError};

use super::{PipeInitiator, SignPipelineBuilder};

/// This is a wrapper type around gstreamer's [gst::Pipeline] providing functions to
/// sign and verify a stream.
#[derive(Debug)]
pub struct SignPipeline {
    init: PipeInitiator,
}

impl SignPipeline {
    pub(crate) fn new(init: PipeInitiator) -> Self {
        Self { init }
    }

    /// Uses the builder API to create a Pipeline
    pub fn build_from_path<P: AsRef<Path>>(path: &P) -> Option<SignPipelineBuilder> {
        SignPipelineBuilder::from_path(path)
    }

    pub fn build<'a, S: ToString>(uri: S) -> SignPipelineBuilder<'a> {
        SignPipelineBuilder::from_uri(uri)
    }

    /// This consumes the object and converts it into an iterator over every
    /// frame with the given [gst::Sample] type.
    ///
    /// Parts of this function was inspired by [`vid_frame_iter`](https://github.com/Farmadupe/vid_dup_finder_lib/blob/main/vid_frame_iter)
    pub fn try_into_iter<VC>(self, context: VC) -> Result<FrameIter<VC>, StreamError> {
        let pipeline = FrameIter::new(self.init, context)?;

        pipeline.play()?;
        Ok(pipeline)
    }
}

#[cfg(feature = "verifying")]
pub use verifying::*;

#[cfg(feature = "verifying")]
mod verifying {
    use identity_iota::prelude::Resolver;

    use futures::{stream, Stream, StreamExt};
    use std::{pin::Pin, sync::Arc, time::Instant};

    use crate::{
        spec::MAX_CHUNK_LENGTH,
        utils::{Delayed, DelayedStream},
        video::manager::verification::{self, SigVideoContext},
        SignFile,
    };

    pub use crate::video::verify::{FrameWithSignatures, InvalidSignatureError, SignatureState};

    use super::*;

    impl SignPipeline {
        // TODO: Write documentation :)
        pub fn verify(
            self,
            resolver: Arc<Resolver>,
            signfile: SignFile,
        ) -> Result<
            impl Stream<Item = Result<Pin<Box<FrameWithSignatures>>, StreamError>>,
            StreamError,
        > {
            let context = SigVideoContext::new(signfile, resolver);

            let iter = self.try_into_iter(context)?;
            let video_state = iter.state.clone();
            let synced = video_state.unsync_clock();

            let mut iter = iter.enumerate().peekable();

            let buf_capacity = match iter.peek() {
                Some(first) => first
                    .1
                    .as_ref()
                    .map_err(|e| e.clone())?
                    .frame
                    .fps()
                    .convert_to_frames(MAX_CHUNK_LENGTH),
                None => 0,
            };

            let delayed = DelayedStream::<_, _>::new(buf_capacity, stream::iter(iter));

            let res = delayed
                .zip(stream::iter(std::iter::repeat(video_state)))
                .filter_map(|(d_info, state)| async {
                    match d_info {
                        Delayed::Partial(_) => None,
                        Delayed::Full(a, fut) => Some(((a.0, a.1.clone(), fut), state)),
                    }
                })
                .then(move |(frame_state, video_state)| async move {
                    let start = Instant::now();
                    let manager = match verification::Manager::new(video_state, frame_state) {
                        Ok(m) => m,
                        Err(e) => return Err(e),
                    };

                    let fps = manager.fps();
                    let info = manager.get_frame_state().await;

                    let sigs = manager.verify_signatures().await;

                    let res = Box::pin(FrameWithSignatures { state: info, sigs });
                    if synced {
                        // NOTE: This code looks a bit weird, mostly because
                        // there are various issues with the simple answers:
                        // - Just awaiting the sleep blocks all other
                        //   tokio::spawn, defeating the purpose of them
                        // - Using executor::block_on breaks down due to the
                        //   potential chance of < 1ms times used
                        tokio::spawn(fps.sleep_for_rest(start.elapsed()))
                            .await
                            .expect("Failed to wait for sleep to finish");
                    }

                    Ok(res)
                });

            Ok(res)
        }
    }
}

#[cfg(feature = "signing")]
mod signing {
    use futures::{Stream, StreamExt};
    use std::{collections::VecDeque, future::Future, sync::Arc};
    use tokio::sync::{
        mpsc::{self, UnboundedSender},
        Mutex,
    };
    use tokio_stream::wrappers::UnboundedReceiverStream;

    use crate::{
        file::SignedInterval,
        spec::MAX_CHUNK_LENGTH,
        video::{
            frame::FrameWithAudio,
            manager::sign,
            sign::{
                AsyncFnMutController, ChunkSigner, Controller, FnMutController, MultiController,
                SingleController,
            },
            FrameState, Signer, SigningError,
        },
    };

    use self::sign::SigningContext;

    use super::*;

    impl SignPipeline {
        /// Signs the current built video writing to the sign_file by calling
        /// the provided [Controller::get_chunks] for every frame with its
        /// timeframe and rgb frame.
        ///
        /// From the output of this, it will then sign the chunks defined by
        /// [ChunkSigner] concurrently as to not interfere with the video
        /// playback.
        ///
        /// For example if you wanted to sign in 100ms second intervals
        ///
        /// ```no_run
        /// # use std::error::Error;
        /// # use identity_iota::{core::FromJson, credential::Subject, did::DID};
        /// # use serde_json::json;
        /// # use testlibs::{
        /// #     client::get_client,
        /// #     identity::TestIdentity,
        /// #     issuer::TestIssuer,
        /// #     test_video, videos,
        /// # };
        /// #
        /// # #[tokio::main]
        /// # async fn main() -> Result<(), Box<dyn Error>> {
        /// use std::sync::Arc;
        /// use stream_signer::{video::{sign, Signer}, SignPipeline, SignFile, TryStreamExt};
        ///
        /// stream_signer::gst::init()?;
        ///
        /// # let client = get_client();
        /// # let issuer = TestIssuer::new(client.clone()).await?;
        ///
        /// # let identity = TestIdentity::new(&issuer, |id| {
        /// #     Subject::from_json_value(json!({
        /// #       "id": id.as_str(),
        /// #       "name": "Alice",
        /// #       "degree": {
        /// #         "type": "BachelorDegree",
        /// #         "name": "Bachelor of Science and Arts",
        /// #       },
        /// #       "GPA": "4.0",
        /// #     })).unwrap()
        /// # })
        /// # .await?;
        ///
        /// let pipeline = SignPipeline::build("https://example.com/video_feed").build()?;
        ///
        /// let signer = Arc::new(identity);
        ///
        /// let sign_file = pipeline.sign_with(sign::IntervalController::build(signer, 100))
        ///     .expect("Failed to start stream")
        ///     .try_collect::<SignFile>()
        ///     .await
        ///     .expect("Failed to look at frame");
        ///
        /// sign_file.write("./my_signatures.ssrt").expect("Failed to write signfile");
        ///
        /// # Ok(())
        /// # }
        /// ```
        pub fn sign_with<C, S>(
            self,
            controller: C,
        ) -> Result<impl Stream<Item = Result<SignedInterval, SigningError>>, StreamError>
        where
            S: Signer + 'static,
            C: Controller<S> + 'static,
        {
            let (tx, rx) = mpsc::unbounded_channel();
            let res = UnboundedReceiverStream::new(rx);

            let context = SigningContext::new(controller);

            let iter = self.try_into_iter(context)?;

            tokio::spawn(Self::sign_with_thread(iter, tx));

            Ok(res)
        }

        /// Internally used function which runs the thread for calling the
        /// [Controller] and then spawning the signing thread which then feeds
        /// back to the stream
        async fn sign_with_thread<C, S>(
            iter: FrameIter<SigningContext<S, C>>,
            sender: UnboundedSender<Result<SignedInterval, SigningError>>,
        ) -> Result<(), StreamError>
        where
            S: Signer + 'static,
            C: Controller<S> + 'static,
        {
            let mut iter = iter.zip_state().enumerate().peekable();

            let buf_capacity = match iter.peek() {
                Some(first) => first
                    .1
                     .1
                    .as_ref()
                    .map_err(|e| e.clone())?
                    .frame
                    .fps()
                    .convert_to_frames(MAX_CHUNK_LENGTH),
                None => 0,
            };

            let mut buf: VecDeque<FrameWithAudio> = VecDeque::new();
            buf.reserve_exact(buf_capacity);
            let frame_buffer = Arc::new(Mutex::new(buf));

            for constructor in iter.zip(std::iter::repeat(frame_buffer)) {
                if sender.is_closed() {
                    return Ok(());
                }

                let manager = match sign::manage(constructor).await {
                    Ok(m) => m,
                    Err(e) => {
                        if let Err(e) = sender.send(Err(e)) {
                            println!("Error when sending (exiting thread): {e:?}");
                            return Ok(());
                        }
                        continue;
                    }
                };

                let chunks = manager.request_chunks().await;

                let sender = sender.clone();
                tokio::spawn(chunks.for_each_concurrent(3, move |message| {
                    if let Err(e) = sender.send(message) {
                        println!("Error when sending (exiting thread): {e:?}");
                    }
                    std::future::ready(())
                }));
            }

            Ok(())
        }

        /// This is a basic wrapper for [MultiController], which then calls
        /// [Self::sign_with]
        ///
        /// For example if you wanted to sign in 100ms second intervals
        ///
        /// ```no_run
        /// # use std::error::Error;
        /// # use identity_iota::{core::FromJson, credential::Subject, did::DID};
        /// # use serde_json::json;
        /// # use testlibs::{
        /// #     client::get_client,
        /// #     identity::TestIdentity,
        /// #     issuer::TestIssuer,
        /// #     test_video, videos,
        /// # };
        /// #
        /// # #[tokio::main]
        /// # async fn main() -> Result<(), Box<dyn Error>> {
        /// use std::sync::Arc;
        /// use stream_signer::{video::{sign, Signer}, SignPipeline, SignFile, TryStreamExt};
        ///
        /// stream_signer::gst::init()?;
        ///
        /// # let client = get_client();
        /// # let issuer = TestIssuer::new(client.clone()).await?;
        ///
        /// # let alice_identity = TestIdentity::new(&issuer, |id| {
        /// #     Subject::from_json_value(json!({
        /// #       "id": id.as_str(),
        /// #       "name": "Alice",
        /// #       "degree": {
        /// #         "type": "BachelorDegree",
        /// #         "name": "Bachelor of Science and Arts",
        /// #       },
        /// #       "GPA": "4.0",
        /// #     })).unwrap()
        /// # })
        /// # .await?;
        ///
        /// # let bob_identity = TestIdentity::new(&issuer, |id| {
        /// #     Subject::from_json_value(json!({
        /// #       "id": id.as_str(),
        /// #       "name": "Alice",
        /// #       "degree": {
        /// #         "type": "BachelorDegree",
        /// #         "name": "Bachelor of Science and Arts",
        /// #       },
        /// #       "GPA": "4.0",
        /// #     })).unwrap()
        /// # })
        /// # .await?;
        ///
        /// let pipeline = SignPipeline::build("https://example.com/video_feed").build()?;
        ///
        /// let alice_signer = Arc::new(alice_identity);
        /// let bob_signer = Arc::new(bob_identity);
        ///
        /// let sign_file = pipeline.sign_with_all(vec![
        ///     Box::new(sign::IntervalController::build(alice_signer, 100)),
        ///     Box::new(sign::IntervalController::build(bob_signer, 100)),
        /// ])
        ///     .expect("Failed to start stream")
        ///     .try_collect::<SignFile>()
        ///     .await
        ///     .expect("Failed to look at frame");
        ///
        /// sign_file.write("./my_signatures.ssrt").expect("Failed to write signfile");
        ///
        /// # Ok(())
        /// # }
        /// ```
        #[inline]
        pub fn sign_with_all<S: Signer + 'static>(
            self,
            controllers: Vec<Box<dyn SingleController<S> + Sync + Send + 'static>>,
        ) -> Result<impl Stream<Item = Result<SignedInterval, SigningError>>, StreamError> {
            self.sign_with::<MultiController<S>, _>(controllers.into())
        }

        /// Basic wrapper around [FnMutController] and calling [Self::sign_with].
        ///
        /// If the function returns some [ChunkSigner], it will use the information to
        /// sign the chunk form the `start` property to the current timestamp.
        ///
        /// For example if you wanted to sign a video in 100ms second intervals you
        /// could do the following
        ///
        /// ```no_run
        /// # use std::error::Error;
        /// # use identity_iota::{core::FromJson, credential::Subject, did::DID};
        /// # use serde_json::json;
        /// # use testlibs::{
        /// #     client::get_client,
        /// #     identity::TestIdentity,
        /// #     issuer::TestIssuer,
        /// #     test_video, videos,
        /// # };
        ///
        /// #
        /// # #[tokio::main]
        /// # async fn main() -> Result<(), Box<dyn Error>> {
        /// use std::sync::Arc;
        /// use stream_signer::{video::{ChunkSigner, Signer}, SignPipeline, SignFile, TryStreamExt};
        ///
        /// stream_signer::gst::init()?;
        ///
        /// # let client = get_client();
        /// # let issuer = TestIssuer::new(client.clone()).await?;
        ///
        /// # let identity = TestIdentity::new(&issuer, |id| {
        /// #     Subject::from_json_value(json!({
        /// #       "id": id.as_str(),
        /// #       "name": "Alice",
        /// #       "degree": {
        /// #         "type": "BachelorDegree",
        /// #         "name": "Bachelor of Science and Arts",
        /// #       },
        /// #       "GPA": "4.0",
        /// #     })).unwrap()
        /// # })
        /// # .await?;
        ///
        /// let pipeline = SignPipeline::build("https://example.com/video_feed").build()?;
        ///
        /// let signer = Arc::new(identity);
        ///
        /// let mut is_first = true;
        /// let sign_file = pipeline.sign(move |info| {
        ///   // ...
        ///   if !info.time.is_start() && info.time.multiple_of(100) {
        ///     let res = vec![
        ///       ChunkSigner::new(info.time.start() - 100, signer.clone(), None, is_first),
        ///     ];
        ///     is_first = false;
        ///     res
        ///   } else {
        ///     vec![]
        ///   }
        /// })
        /// .expect("Failed to start stream")
        /// .try_collect::<SignFile>()
        /// .await
        /// .expect("Failed to look at frame");
        ///
        /// sign_file.write("./my_signatures.ssrt").expect("Failed to write signfile");
        ///
        /// # Ok(())
        /// # }
        /// ```
        #[inline]
        pub fn sign<S, F>(
            self,
            sign_with: F,
        ) -> Result<impl Stream<Item = Result<SignedInterval, SigningError>>, StreamError>
        where
            S: Signer + 'static,
            F: FnMut(FrameState) -> Vec<ChunkSigner<S>> + Sync + Send + 'static,
        {
            self.sign_with::<FnMutController<S, _>, _>(sign_with.into())
        }

        /// Basic wrapper around [AsyncFnMutController] and calling [Self::sign_with].
        #[inline]
        pub fn sign_async<S, F, FUT>(
            self,
            sign_with: F,
        ) -> Result<impl Stream<Item = Result<SignedInterval, SigningError>>, StreamError>
        where
            S: Signer + 'static,
            F: FnMut(FrameState) -> FUT + Sync + Send + 'static,
            FUT: Future<Output = Vec<ChunkSigner<S>>> + Send + 'static,
        {
            self.sign_with::<AsyncFnMutController<S, _, _>, S>(sign_with.into())
        }
    }
}
