use std::path::Path;

use crate::time::ONE_SECOND_MILLIS;

use super::{builder::SignPipelineBuilder, iter::FrameIter, manager::PipeInitiator, StreamError};

pub const MAX_CHUNK_LENGTH: usize = 10 * ONE_SECOND_MILLIS as usize;
pub const MIN_CHUNK_LENGTH: usize = 50;

/// This is a wrapper type around gstreamer's [Pipeline] providing functions to
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
    use glib::object::{Cast, ObjectExt};
    use gst::prelude::GstBinExt;
    use identity_iota::prelude::Resolver;

    use futures::{stream, Stream, StreamExt};
    use std::{pin::Pin, time::Instant};

    use crate::{
        utils::{Delayed, DelayedStream},
        video::manager::verification::{self, SigVideoContext},
        SignFile,
    };

    pub use crate::video::verify::{InvalidSignatureError, SignatureState, VerifiedFrame};

    use super::*;

    impl SignPipeline {
        // TODO: Write documentation :)
        pub fn verify<'a>(
            self,
            resolver: &'a Resolver,
            signfile: &'a SignFile,
        ) -> Result<
            impl Stream<Item = Result<Pin<Box<VerifiedFrame>>, StreamError>> + use<'a>,
            StreamError,
        > {
            let synced = self.set_clock_unsynced();

            let context = SigVideoContext::new(signfile, resolver);

            let iter = self.try_into_iter(context)?;
            let video_state = iter.state.clone();
            let mut iter = iter.enumerate().peekable();

            let buf_capacity = match iter.peek() {
                Some(first) => first
                    .1
                    .as_ref()
                    .map_err(|e| e.clone())?
                    .fps()
                    .convert_to_frames(MAX_CHUNK_LENGTH),
                None => 0,
            };

            let delayed = DelayedStream::<_, _>::new(buf_capacity, stream::iter(iter));

            let res = delayed
                .filter_map(|d_info| async {
                    match d_info {
                        Delayed::Partial(_) => None,
                        Delayed::Full(a, fut) => Some((a.0, a.1.clone(), fut)),
                    }
                })
                .zip(stream::iter(std::iter::repeat(video_state)))
                .then(move |(frame_state, video_state)| async move {
                    let now = Instant::now();
                    let manager = match verification::Manager::new(video_state, frame_state) {
                        Ok(m) => m,
                        Err(e) => return Err(e),
                    };

                    let fps = manager.fps();
                    let info = manager.get_frame_state().await;

                    let sigs = manager.verify_signatures().await;

                    let res = Box::pin(VerifiedFrame { state: info, sigs });

                    if synced {
                        fps.sleep_for_rest(now.elapsed()).await;
                    }

                    Ok(res)
                });

            Ok(res)
        }

        /// Sets the `sync` property in the `sink` to be false so that we
        /// go through the frames as fast as possible and returns the value
        /// it was set to
        fn set_clock_unsynced(&self) -> bool {
            let appsink = self
                .init
                .pipe
                .raw()
                .by_name(&self.init.video_sink)
                .expect("Sink element not found")
                .downcast::<gst_app::AppSink>()
                .expect("Sink element is expected to be an appsink!");
            let sync = appsink.property("sync");
            appsink.set_property("sync", false);

            sync
        }
    }
}

#[cfg(feature = "signing")]
mod signing {
    use futures::{stream, FutureExt, Stream, StreamExt};
    use std::{
        collections::VecDeque,
        future::{self, Future},
        pin::Pin,
        sync::Arc,
    };
    use tokio::sync::Mutex;

    use crate::{
        file::SignedInterval,
        video::{sign::Controller, Frame, FrameState, SigningError},
    };

    use self::sign::SigningContext;

    pub use super::super::{manager::sign, sign::ChunkSigner, Signer};
    use super::*;

    impl SignPipeline {
        /// Signs the video with a given controller which manages when to sign
        /// a chunk. Similar to [Self::sign], but allows more generalised behaviour.
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
        pub fn sign_with<C, S: Signer + 'static>(
            self,
            mut controller: C,
        ) -> Result<impl Stream<Item = Result<SignedInterval, SigningError>>, StreamError>
        where
            C: Controller<S> + 'static,
        {
            // let controller = Arc::new(Mutex::new(controller));
            self.sign_async(move |info| {
                controller.get_chunk(&info).map(|r| match r {
                    Some(res) => vec![res],
                    None => vec![],
                })
            })
        }

        /// Extension of [Self::sign_with], allowing multiple controllers of
        /// the same type
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
        pub fn sign_with_all<C, S: Signer + 'static>(
            self,
            controllers: Vec<Box<dyn Controller<S>>>,
        ) -> Result<impl Stream<Item = Result<SignedInterval, SigningError>>, StreamError> {
            let controllers = Arc::new(Mutex::new(controllers));
            self.sign_async(move |info| {
                let controllers = controllers.clone();

                async move {
                    let mut controllers = controllers.lock().await;
                    stream::iter(controllers.iter_mut())
                        .then(move |c| c.get_chunk(&info))
                        .filter(|x| future::ready(Option::is_some(x)))
                        .map(Option::unwrap)
                        .collect::<Vec<_>>()
                        .await
                    // .collect::<Vec<_>>()
                }
            })
        }

        /// Signs the current built video writing to the sign_file by calling
        /// the provided `sign_with` function for every frame with its timeframe
        /// and rgb frame.
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
        /// let sign_file = pipeline.sign(|info| {
        ///   // ...
        ///   if !info.time.is_start() && info.time.multiple_of(100) {
        ///     let res = vec![
        ///       ChunkSigner::new(info.time.start() - 100, signer.clone(), is_first),
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
        pub fn sign<S, F, ITER>(
            self,
            sign_with: F,
        ) -> Result<impl Stream<Item = Result<SignedInterval, SigningError>>, StreamError>
        where
            S: Signer + 'static,
            F: FnMut(FrameState) -> ITER,
            ITER: IntoIterator<Item = ChunkSigner<S>>,
            <ITER as IntoIterator>::IntoIter: Send,
        {
            let sign_with = Arc::new(Mutex::new(sign_with));
            self.sign_async(move |frame| {
                let sign_with = sign_with.clone();
                async move {
                    let mut sign_with = sign_with.lock().await;
                    sign_with(frame)
                }
            })
        }

        pub fn sign_async<S, F, ITER, FUT>(
            self,
            sign_with: F,
        ) -> Result<impl Stream<Item = Result<SignedInterval, SigningError>>, StreamError>
        where
            S: Signer + 'static,
            F: FnMut(FrameState) -> FUT,
            FUT: Future<Output = ITER>,
            ITER: IntoIterator<Item = ChunkSigner<S>>,
            <ITER as IntoIterator>::IntoIter: Send,
        {
            let sign_with = Mutex::new(sign_with);
            let context = SigningContext { sign_with };

            let mut iter = self
                .try_into_iter(context)?
                .zip_state()
                .enumerate()
                .peekable();

            let buf_capacity = match iter.peek() {
                Some(first) => first
                    .1
                     .1
                    .as_ref()
                    .map_err(|e| e.clone())?
                    .fps()
                    .convert_to_frames(MAX_CHUNK_LENGTH),
                None => 0,
            };

            let mut buf: VecDeque<Frame> = VecDeque::new();
            buf.reserve_exact(buf_capacity);
            let frame_buffer = Arc::new(Mutex::new(buf));

            let res = stream::iter(iter.zip(std::iter::repeat(frame_buffer)))
                .then(sign::manage)
                .then(|manager| async {
                    match manager {
                        Ok(m) => m.request_chunks().await,
                        Err(e) => Box::pin(stream::iter([Err(e)]))
                            as Pin<Box<dyn Stream<Item = _> + Send>>,
                    }
                })
                .flatten();

            Ok(res)
        }
    }
}

#[cfg(test)]
mod tests {
    use futures::{future, StreamExt, TryStreamExt};
    use identity_iota::{core::FromJson, credential::Subject, did::DID};
    use serde_json::json;
    use std::{error::Error, sync::Arc};
    use testlibs::{
        client::{get_client, get_resolver},
        identity::TestIdentity,
        issuer::TestIssuer,
        test_video, videos,
    };

    use super::*;

    use crate::{
        spec::Coord,
        video::{
            sign::{self, Controller},
            verify::SignatureState,
            SigOperationError, SigningError,
        },
        SignFile,
    };

    async fn sign_and_verify_int<F, C>(get_controller: F) -> Result<(), Box<dyn Error>>
    where
        F: Fn(Arc<TestIdentity>) -> C,
        C: Controller<TestIdentity> + 'static,
    {
        gst::init()?;

        let client = get_client();
        let issuer = TestIssuer::new(client.clone()).await?;
        let resolver = get_resolver(client);

        let identity = TestIdentity::new(&issuer, |id| {
            Subject::from_json_value(json!({
              "id": id.as_str(),
              "name": "Alice",
              "degree": {
                "type": "BachelorDegree",
                "name": "Bachelor of Science and Arts",
              },
              "GPA": "4.0",
            }))
            .expect("Invalid subject")
        })
        .await?;

        let filepath = test_video(videos::BIG_BUNNY);

        let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

        let signfile = pipe
            .sign_with(get_controller(Arc::new(identity)))?
            .try_collect::<SignFile>()
            .await?;

        let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

        let mut count = 0;
        pipe.verify(&resolver, &signfile)?
            .for_each(|v| {
                count += 1;

                let v = match v {
                    Ok(v) => v,
                    Err(e) => {
                        panic!("Frame was invalid: {e}");
                    }
                };

                for s in &v.sigs {
                    assert!(
                        matches!(s, SignatureState::Verified(_)),
                        "{s:?} resolved correctly"
                    );
                }

                future::ready(())
            })
            .await;

        assert!(count > 0, "We verified some chunks");

        Ok(())
    }

    #[tokio::test]
    async fn sign_and_verify() -> Result<(), Box<dyn Error>> {
        sign_and_verify_int(|i| sign::IntervalController::build(i, 100)).await
    }

    async fn sign_and_verify_multi(
        alice_chunk_size: u32,
        bob_chunk_size: u32,
    ) -> Result<(), Box<dyn Error>> {
        gst::init()?;

        let client = get_client();
        let issuer = TestIssuer::new(client.clone()).await?;
        let resolver = get_resolver(client);

        let alice_identity = TestIdentity::new(&issuer, |id| {
            Subject::from_json_value(json!({
              "id": id.as_str(),
              "name": "Alice",
              "degree": {
                "type": "BachelorDegree",
                "name": "Bachelor of Science and Arts",
              },
              "GPA": "4.0",
            }))
            .expect("Invalid subject")
        })
        .await?;

        let bob_identity = TestIdentity::new(&issuer, |id| {
            Subject::from_json_value(json!({
              "id": id.as_str(),
              "name": "Bob",
              "degree": {
                "type": "BachelorDegree",
                "name": "Bachelor of Science and Arts",
              },
              "GPA": "4.0",
            }))
            .expect("Invalid subject")
        })
        .await?;

        let filepath = test_video(videos::BIG_BUNNY);

        let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

        let alice_signfile = pipe
            .sign_with(sign::IntervalController::build(
                Arc::new(alice_identity),
                alice_chunk_size,
            ))?
            .try_collect::<SignFile>()
            .await?;

        let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

        let bob_signfile = pipe
            .sign_with(sign::IntervalController::build(
                Arc::new(bob_identity),
                bob_chunk_size,
            ))?
            .try_collect::<SignFile>()
            .await?;

        let mut signfile = alice_signfile;
        signfile.extend(bob_signfile.iter());

        let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

        let mut count = 0;
        pipe.verify(&resolver, &signfile)?
            .for_each(|v| {
                count += 1;

                let v = match v {
                    Ok(v) => v,
                    Err(e) => {
                        panic!("Frame was invalid: {e}");
                    }
                };

                for s in &v.sigs {
                    assert!(
                        matches!(s, SignatureState::Verified(_)),
                        "{s:?} verified correctly"
                    );
                }

                future::ready(())
            })
            .await;

        assert!(count > 0, "We verified some chunks");

        Ok(())
    }

    async fn sign_and_verify_multi_together(
        alice_chunk_size: u32,
        bob_chunk_size: u32,
    ) -> Result<(), Box<dyn Error>> {
        gst::init()?;

        let client = get_client();
        let issuer = TestIssuer::new(client.clone()).await?;
        let resolver = get_resolver(client);

        let alice_identity = TestIdentity::new(&issuer, |id| {
            Subject::from_json_value(json!({
              "id": id.as_str(),
              "name": "Alice",
              "degree": {
                "type": "BachelorDegree",
                "name": "Bachelor of Science and Arts",
              },
              "GPA": "4.0",
            }))
            .expect("Invalid subject")
        })
        .await?;

        let bob_identity = TestIdentity::new(&issuer, |id| {
            Subject::from_json_value(json!({
              "id": id.as_str(),
              "name": "Bob",
              "degree": {
                "type": "BachelorDegree",
                "name": "Bachelor of Science and Arts",
              },
              "GPA": "4.0",
            }))
            .expect("Invalid subject")
        })
        .await?;

        let filepath = test_video(videos::BIG_BUNNY);

        let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

        let signfile = pipe
            .sign_with_all::<sign::IntervalController<TestIdentity>, TestIdentity>(vec![
                Box::new(sign::IntervalController::build(
                    Arc::new(alice_identity),
                    alice_chunk_size,
                )),
                Box::new(sign::IntervalController::build(
                    Arc::new(bob_identity),
                    bob_chunk_size,
                )),
            ])?
            .try_collect::<SignFile>()
            .await?;

        let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

        let mut count = 0;
        pipe.verify(&resolver, &signfile)?
            .for_each(|v| {
                count += 1;

                let v = match v {
                    Ok(v) => v,
                    Err(e) => {
                        panic!("Frame was invalid: {e}");
                    }
                };

                for s in &v.sigs {
                    assert!(
                        matches!(s, SignatureState::Verified(_)),
                        "{s:?} verified correctly"
                    );
                }

                future::ready(())
            })
            .await;

        assert!(count > 0, "We verified some chunks");

        Ok(())
    }

    #[tokio::test]
    async fn sign_and_verify_multi_same() -> Result<(), Box<dyn Error>> {
        sign_and_verify_multi(100, 100).await?;
        sign_and_verify_multi_together(100, 100).await
    }

    #[tokio::test]
    async fn sign_and_verify_multi_diff() -> Result<(), Box<dyn Error>> {
        sign_and_verify_multi(100, 179).await?;
        sign_and_verify_multi_together(100, 179).await
    }

    #[tokio::test]
    async fn sign_too_large_chunk() -> Result<(), Box<dyn Error>> {
        gst::init()?;

        let client = get_client();
        let issuer = TestIssuer::new(client.clone()).await?;

        let identity = TestIdentity::new(&issuer, |id| {
            Subject::from_json_value(json!({
              "id": id.as_str(),
              "name": "Alice",
              "degree": {
                "type": "BachelorDegree",
                "name": "Bachelor of Science and Arts",
              },
              "GPA": "4.0",
            }))
            .expect("Invalid subject")
        })
        .await?;

        let filepath = test_video(videos::BIG_BUNNY_LONG);

        let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

        let res = pipe
            .sign_with(sign::IntervalController::build(
                Arc::new(identity),
                MAX_CHUNK_LENGTH as u32 + 1,
            ))?
            .try_collect::<Vec<_>>()
            .await;

        const LENGTH: usize = MAX_CHUNK_LENGTH + 1;

        assert!(
            matches!(
                res,
                Err(SigningError::Operation(
                    SigOperationError::InvalidChunkSize(LENGTH)
                ))
            ),
            "{:?} reported invalid chunk size of {LENGTH}",
            res.map(|mut v| v.pop())
        );

        Ok(())
    }

    #[tokio::test]
    async fn sign_too_small_chunk() -> Result<(), Box<dyn Error>> {
        gst::init()?;

        let client = get_client();
        let issuer = TestIssuer::new(client.clone()).await?;

        let identity = TestIdentity::new(&issuer, |id| {
            Subject::from_json_value(json!({
              "id": id.as_str(),
              "name": "Alice",
              "degree": {
                "type": "BachelorDegree",
                "name": "Bachelor of Science and Arts",
              },
              "GPA": "4.0",
            }))
            .expect("Invalid subject")
        })
        .await?;

        let filepath = test_video(videos::BIG_BUNNY);

        let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

        let res = pipe
            .sign_with(sign::IntervalController::build(Arc::new(identity), 4))?
            .try_collect::<Vec<_>>()
            .await;

        const LENGTH: usize = 4;

        assert!(
            matches!(
                res,
                Err(SigningError::Operation(
                    SigOperationError::InvalidChunkSize(LENGTH)
                ))
            ),
            "{:?} reported an invalid chunk size of {LENGTH}",
            res.map(|mut v| v.pop())
        );

        Ok(())
    }

    #[tokio::test]
    async fn sign_and_verify_embedding() -> Result<(), Box<dyn Error>> {
        sign_and_verify_int(|i| {
            sign::IntervalController::build(i, 100)
                .with_embedding(Coord::new(10, 10), Coord::new(100, 100))
        })
        .await
    }

    #[tokio::test]
    async fn sign_with_too_large_embedding() -> Result<(), Box<dyn Error>> {
        gst::init()?;

        let client = get_client();
        let issuer = TestIssuer::new(client.clone()).await?;

        let identity = Arc::new(
            TestIdentity::new(&issuer, |id| {
                Subject::from_json_value(json!({
                  "id": id.as_str(),
                  "name": "Alice",
                  "degree": {
                    "type": "BachelorDegree",
                    "name": "Bachelor of Science and Arts",
                  },
                  "GPA": "4.0",
                }))
                .expect("Invalid subject")
            })
            .await?,
        );

        let filepath = test_video(videos::BIG_BUNNY);

        for size in [(1000, 50), (50, 1000), (1000, 1000)]
            .into_iter()
            .map(Coord::from)
        {
            let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

            let ctrl = sign::IntervalController::build(identity.clone(), 100)
                .with_embedding(Coord::new(0, 0), size);

            let res = pipe.sign_with(ctrl)?.try_collect::<SignFile>().await;

            const POS: Coord = Coord::new(0, 0);

            assert!(
                matches!(
                    res,
                    Err(SigningError::Operation(SigOperationError::InvalidCrop(
                        POS, esize
                    ))) if esize == size
                ),
                "{:?} responded as an invalid crop of position {POS:?} and size {size:?}",
                res.map(|s| s.iter().next())
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn sign_with_too_small_embedding() -> Result<(), Box<dyn Error>> {
        gst::init()?;

        let client = get_client();
        let issuer = TestIssuer::new(client.clone()).await?;

        let identity = Arc::new(
            TestIdentity::new(&issuer, |id| {
                Subject::from_json_value(json!({
                  "id": id.as_str(),
                  "name": "Alice",
                  "degree": {
                    "type": "BachelorDegree",
                    "name": "Bachelor of Science and Arts",
                  },
                  "GPA": "4.0",
                }))
                .expect("Invalid subject")
            })
            .await?,
        );

        let filepath = test_video(videos::BIG_BUNNY);

        for size in [(0, 50), (50, 0), (0, 0)].into_iter().map(Coord::from) {
            let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

            let ctrl = sign::IntervalController::build(identity.clone(), 100)
                .with_embedding(Coord::new(0, 0), size);

            let res = pipe.sign_with(ctrl)?.try_collect::<SignFile>().await;

            const POS: Coord = Coord::new(0, 0);
            assert!(
                matches!(
                    res,
                    Err(SigningError::Operation(SigOperationError::InvalidCrop(
                        POS, esize
                    ))) if esize == size
                ),
                "{:?} responded as an invalid crop of position {POS:?} and size {size:?}",
                res.map(|s| s.iter().next())
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn sign_with_too_large_position() -> Result<(), Box<dyn Error>> {
        gst::init()?;

        let client = get_client();
        let issuer = TestIssuer::new(client.clone()).await?;

        let identity = Arc::new(
            TestIdentity::new(&issuer, |id| {
                Subject::from_json_value(json!({
                  "id": id.as_str(),
                  "name": "Alice",
                  "degree": {
                    "type": "BachelorDegree",
                    "name": "Bachelor of Science and Arts",
                  },
                  "GPA": "4.0",
                }))
                .expect("Invalid subject")
            })
            .await?,
        );

        let filepath = test_video(videos::BIG_BUNNY);

        for pos in [(1000, 50), (50, 1000), (1000, 1000)]
            .into_iter()
            .map(Coord::from)
        {
            let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

            let ctrl = sign::IntervalController::build(identity.clone(), 100)
                .with_embedding(pos, Coord::new(1, 1));

            let res = pipe.sign_with(ctrl)?.try_collect::<SignFile>().await;

            const SIZE: Coord = Coord::new(1, 1);
            assert!(
                matches!(
                    res,
                    Err(SigningError::Operation(SigOperationError::InvalidCrop(
                        epos, SIZE
                    ))) if epos == pos
                ),
                "{:?} responded as an invalid crop of position {pos:?} and size {SIZE:?}",
                res.map(|s| s.iter().next())
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn verify_with_invalid_embedding() -> Result<(), Box<dyn Error>> {
        gst::init()?;

        let client = get_client();
        let issuer = TestIssuer::new(client.clone()).await?;
        let resolver = get_resolver(client);

        let identity = TestIdentity::new(&issuer, |id| {
            Subject::from_json_value(json!({
              "id": id.as_str(),
              "name": "Alice",
              "degree": {
                "type": "BachelorDegree",
                "name": "Bachelor of Science and Arts",
              },
              "GPA": "4.0",
            }))
            .expect("Invalid subject")
        })
        .await?;

        let filepath = test_video(videos::BIG_BUNNY);

        let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

        let signs = pipe
            .sign_with(sign::IntervalController::build(Arc::new(identity), 100))?
            .try_collect::<Vec<_>>()
            .await?;

        for size in [
            (0, 50),
            (50, 0),
            (0, 0),
            (1000, 50),
            (50, 1000),
            (1000, 1000),
        ]
        .into_iter()
        .map(Coord::from)
        {
            // Secretly just change the size as technically we won't get to the
            // verification stage
            let signfile = signs
                .iter()
                .cloned()
                .map(|mut i| {
                    i.val.size = size;
                    i
                })
                .collect::<SignFile>();

            let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

            let mut count = 0;
            pipe.verify(&resolver, &signfile)?
                .for_each(|v| {
                    count += 1;

                    let v = match v {
                        Ok(v) => v,
                        Err(e) => {
                            panic!("Frame was invalid: {e}");
                        }
                    };

                    for s in &v.sigs {
                        assert!(
                            matches!(
                                s,
                                SignatureState::Invalid(
                                    InvalidSignatureError::Operation(
                                        SigOperationError::InvalidCrop(
                                            Coord { x: 0, y: 0},
                                            esize,
                                        ),
                                    ),
                                ) if *esize == size
                            ),
                            "{s:?} marks itself as an invalid crop with size {size:?}"
                        );
                    }

                    future::ready(())
                })
                .await;

            assert!(count > 0, "We verified some chunks");
        }

        Ok(())
    }

    #[tokio::test]
    async fn verify_with_invalid_pos() -> Result<(), Box<dyn Error>> {
        gst::init()?;

        let client = get_client();
        let issuer = TestIssuer::new(client.clone()).await?;
        let resolver = get_resolver(client);

        let identity = TestIdentity::new(&issuer, |id| {
            Subject::from_json_value(json!({
              "id": id.as_str(),
              "name": "Alice",
              "degree": {
                "type": "BachelorDegree",
                "name": "Bachelor of Science and Arts",
              },
              "GPA": "4.0",
            }))
            .expect("Invalid subject")
        })
        .await?;

        let filepath = test_video(videos::BIG_BUNNY);

        let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

        let signs = pipe
            .sign_with(sign::IntervalController::build(Arc::new(identity), 100))?
            .try_collect::<Vec<_>>()
            .await?;

        for pos in [(1000, 50), (50, 1000), (1000, 1000)]
            .into_iter()
            .map(Coord::from)
        {
            // Secretly just change the pos as technically we won't get to the
            // verification stage
            let signfile = signs
                .iter()
                .cloned()
                .map(|mut i| {
                    i.val.size = Coord::new(1, 1);
                    i.val.pos = pos;
                    i
                })
                .collect::<SignFile>();

            let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

            let mut count = 0;
            pipe.verify(&resolver, &signfile)?
                .for_each(|v| {
                    count += 1;

                    let v = match v {
                        Ok(v) => v,
                        Err(e) => {
                            panic!("Frame was invalid: {e}");
                        }
                    };

                    for s in &v.sigs {
                        assert!(
                            matches!(
                                s,
                                SignatureState::Invalid(
                                    InvalidSignatureError::Operation(
                                        SigOperationError::InvalidCrop(
                                            epos,
                                            Coord { x: 1, y: 1},
                                        ),
                                    ),
                                ) if *epos == pos
                            ),
                            "{s:?} marks itself as an invalid crop with position {pos:?}"
                        );
                    }

                    future::ready(())
                })
                .await;

            assert!(count > 0, "We verified some chunks");
        }

        Ok(())
    }

    #[tokio::test]
    async fn verify_with_invalid_chunk_length() -> Result<(), Box<dyn Error>> {
        gst::init()?;

        // TODO: This is like really slow
        let client = get_client();
        let issuer = TestIssuer::new(client.clone()).await?;
        let resolver = get_resolver(client);

        let identity = TestIdentity::new(&issuer, |id| {
            Subject::from_json_value(json!({
              "id": id.as_str(),
              "name": "Alice",
              "degree": {
                "type": "BachelorDegree",
                "name": "Bachelor of Science and Arts",
              },
              "GPA": "4.0",
            }))
            .expect("Invalid subject")
        })
        .await?;

        let filepath = test_video(videos::BIG_BUNNY_LONG);

        let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

        let signs = pipe
            .sign_with(sign::IntervalController::build(Arc::new(identity), 100))?
            .try_collect::<Vec<_>>()
            .await?;

        for length in [MIN_CHUNK_LENGTH - 1, MAX_CHUNK_LENGTH + 1] {
            // Secretly just change the length as technically we won't get to the
            // verification stage
            let signfile = signs
                .iter()
                .cloned()
                .map(|mut i| {
                    i.stop = i.start + length as u32;
                    i
                })
                .collect::<SignFile>();

            let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

            let mut count = 0;
            pipe.verify(&resolver, &signfile)?
                .for_each(|v| {
                    count += 1;

                    let v = match v {
                        Ok(v) => v,
                        Err(e) => {
                            panic!("Frame was invalid: {e}");
                        }
                    };

                    for s in &v.sigs {
                        assert!(
                            matches!(
                                s,
                                SignatureState::Invalid(
                                    InvalidSignatureError::Operation(
                                        SigOperationError::InvalidChunkSize(
                                            elength,
                                        ),
                                    ),
                                ) if *elength == length
                            ),
                            "{s:?} marks itself as an invalid chunk length with length {length}ms"
                        );
                    }

                    future::ready(())
                })
                .await;

            assert!(count > 0, "We verified some chunks");
        }

        Ok(())
    }

    #[tokio::test]
    async fn verify_unresolvable() -> Result<(), Box<dyn Error>> {
        gst::init()?;

        let client = get_client();
        let issuer = TestIssuer::new(client.clone()).await?;

        let identity = TestIdentity::new(&issuer, |id| {
            Subject::from_json_value(json!({
              "id": id.as_str(),
              "name": "Alice",
              "degree": {
                "type": "BachelorDegree",
                "name": "Bachelor of Science and Arts",
              },
              "GPA": "4.0",
            }))
            .expect("Invalid subject")
        })
        .await?;

        let filepath = test_video(videos::BIG_BUNNY);

        let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

        let signfile = pipe
            .sign_with(sign::IntervalController::build(Arc::new(identity), 100))?
            .try_collect::<SignFile>()
            .await?;

        // Use a separate client for verification so that it has no knowledge
        // of the created identity
        let vclient = get_client();
        let resolver = get_resolver(vclient);
        let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

        let mut count = 0;
        pipe.verify(&resolver, &signfile)?
            .for_each(|v| {
                count += 1;

                let v = match v {
                    Ok(v) => v,
                    Err(e) => {
                        panic!("Frame was invalid: {e}");
                    }
                };

                for s in &v.sigs {
                    assert!(
                        matches!(s, SignatureState::Unresolved(_)),
                        "{s:?} was unresolved"
                    );
                }

                future::ready(())
            })
            .await;

        assert!(count > 0, "We verified some chunks");

        Ok(())
    }

    // TODO:
    // - Sign + Verify with start offset
    // - Sign + Verify with range
}
