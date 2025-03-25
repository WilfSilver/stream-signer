use gst::Pipeline;
use std::path::Path;

use crate::time::ONE_SECOND_MILLIS;

use super::{manager::iter::FrameIter, SignPipelineBuilder, StreamError};

pub const MAX_CHUNK_LENGTH: usize = 10 * ONE_SECOND_MILLIS as usize;

/// This is a wrapper type around gstreamer's [Pipeline] providing functions to
/// sign and verify a stream.
#[derive(Debug)]
pub struct SignPipeline {
    pipe: Pipeline,
    start_offset: Option<f64>,
    sink: String,
}

impl SignPipeline {
    pub(crate) fn new(pipe: Pipeline, start_offset: Option<f64>, sink: String) -> Self {
        Self {
            pipe,
            start_offset,
            sink,
        }
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
        let pipeline = FrameIter::new(self.pipe, &self.sink, self.start_offset, context)?;

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

    use crate::video::manager::verification::SigVideoContext;
    use crate::video::FrameError;
    use crate::{video::manager::verification, SignFile};

    // TODO: Move FrameManager out of here and then you can use *
    pub use crate::video::verify::{
        InvalidSignatureError, SignatureState, UnverifiedSignature, VerifiedFrame,
    };

    use super::super::{Delayed, DelayedStream};
    use super::*;

    impl SignPipeline {
        // TODO: Write documentation :)
        pub fn verify(
            self,
            resolver: Resolver,
            signfile: &SignFile,
        ) -> Result<
            impl Stream<Item = Result<Pin<Box<VerifiedFrame>>, FrameError>> + use<'_>,
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
                    let frame_state = manager.frame.clone();

                    let signatures = manager.verify_signatures().await;

                    let res = match signatures {
                        Ok(sigs) => Ok(Box::pin(VerifiedFrame {
                            info: frame_state.into(),
                            sigs,
                        })),
                        Err(e) => Err(e),
                    };

                    if synced {
                        fps.sleep_for_rest(now.elapsed()).await;
                    }

                    res
                });

            Ok(res)
        }

        /// Sets the `sync` property in the `sink` to be false so that we
        /// go through the frames as fast as possible and returns the value
        /// it was set to
        fn set_clock_unsynced(&self) -> bool {
            let appsink = self
                .pipe
                .by_name(&self.sink)
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
    use futures::{stream, Stream, StreamExt};
    use std::{collections::VecDeque, pin::Pin, sync::Arc};
    use tokio::sync::Mutex;

    use crate::{
        file::{SignedChunk, Timestamp},
        video::{Frame, FrameError, FrameInfo},
    };

    use self::sign::SigningContext;

    pub use super::super::{manager::sign, sign::ChunkSigner, Signer};
    use super::*;

    impl SignPipeline {
        /// Signs the video with common chunk duration, while also partially
        /// handling credential information such that only one definition is made
        /// of the credential.
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
        ///
        /// #
        /// # #[tokio::main]
        /// # async fn main() -> Result<(), Box<dyn Error>> {
        /// use std::sync::Arc;
        /// use stream_signer::{video::Signer, SignPipeline, SignFile, TryStreamExt};
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
        /// # let presentation = identity.build_presentation()?;
        ///
        /// let pipeline = SignPipeline::build("https://example.com/video_feed").build()?;
        ///
        /// let signer = Arc::new(identity);
        ///
        /// let sign_file = pipeline.sign_chunks(100, signer)
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
        ///
        pub fn sign_chunks<T: Into<Timestamp>, S: Signer + 'static>(
            self,
            length: T,
            signer: Arc<S>,
        ) -> Result<impl Stream<Item = Result<SignedChunk, FrameError>>, StreamError> {
            let length = length.into();
            let mut is_start = true;

            self.sign(move |info| {
                if !info.time.is_start() && info.time % length == 0 {
                    let res = vec![ChunkSigner::new(
                        info.time.start() - length,
                        signer.clone(),
                        !is_start,
                    )];
                    is_start = false;
                    res
                } else {
                    vec![]
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
        /// # let presentation = identity.build_presentation()?;
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
        ) -> Result<impl Stream<Item = Result<SignedChunk, FrameError>>, StreamError>
        where
            S: Signer + 'static,
            F: FnMut(FrameInfo) -> ITER,
            ITER: IntoIterator<Item = ChunkSigner<S>>,
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

    use crate::{video::verify::SignatureState, SignFile};

    #[tokio::test]
    async fn sign_and_verify() -> Result<(), Box<dyn Error>> {
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
            .sign_chunks(100, Arc::new(identity))?
            .try_collect::<SignFile>()
            .await?;

        let pipe = SignPipeline::build_from_path(&filepath).unwrap().build()?;

        pipe.verify(resolver, &signfile)?
            .for_each(|v| {
                let v = match v {
                    Ok(v) => v,
                    Err(e) => {
                        // Assert false
                        assert!(false, "Frame was invalid: {e}");
                        return future::ready(());
                    }
                };

                for s in &v.sigs {
                    let (is_ok, msg) = match s {
                        SignatureState::Invalid(e) => {
                            (false, format!("Signature was invalid: {e:?}"))
                        }
                        SignatureState::Unverified(e) => {
                            (false, format!("Signature was unverified: {e:?}"))
                        }
                        SignatureState::Unresolved(e) => {
                            (false, format!("Signature could not resolve: {e:?}"))
                        }
                        SignatureState::Verified(_) => {
                            (true, "Signature resolved correctly".to_string())
                        }
                    };

                    assert!(is_ok, "{msg}");
                }

                future::ready(())
            })
            .await;

        Ok(())
    }

    // TODO:
    // - Try to sign chunk > MAX_BUFFER_SIZE or too short
    // - Try to verify chunk > MAX_BUFFER_SIZE or too short
    // - Try to sign + verify embedding
    //   - Sign with embedding that goes out of range
    // - Try to verify multiple signatures
    // - Sign with multiple signatures (same intersections + different intersections)
    // - Sign with two different calls and signatures then add them together and verify
    // - Verify with stuff which doesn't resolve
    // - Verify with invalid signatures
    // - Verify with invalid issuer????
}
