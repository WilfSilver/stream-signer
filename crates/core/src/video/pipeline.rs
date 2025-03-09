use futures::{stream, Stream, StreamExt};
use glib::object::Cast;
use gstreamer::prelude::GstBinExt;
use gstreamer::Pipeline;
use identity_iota::prelude::Resolver;
use identity_iota::verification::jws::{JwsAlgorithm, VerificationInput};
use image::GenericImageView;
use std::collections::{HashMap, VecDeque};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::video::Delayed;
use crate::{
    file::{SignedChunk, Timestamp},
    spec::Coord,
    time::ONE_SECOND_MILLIS,
    video::frame_iter::ImageFns,
};
use crate::{CredentialStore, SignFile};

#[cfg(feature = "signing")]
pub use super::sign::ChunkSigner;

#[cfg(feature = "verifying")]
use super::verify::{SignatureState, VerifiedFrame};

use super::{
    frame_iter::{RgbFrame, VideoFrame, VideoFrameIter},
    framerate::Framerate,
    FrameInfo, SignPipelineBuilder, VideoError,
};
use super::{DelayedStream, KeyBound, KeyIdBound, SignerInfo};

pub const MAX_CHUNK_LENGTH: usize = 10 * ONE_SECOND_MILLIS as usize;

/// This is a wrapper type around gstreamer's [Pipeline] providing functions to
/// sign and verify a stream.
#[derive(Debug)]
pub struct SignPipeline {
    pipe: Pipeline,
    start_offset: Option<f64>,
    fps: Option<Framerate<usize>>, // TODO: Get from pipeline
}

impl SignPipeline {
    pub fn new(pipe: Pipeline, start_offset: Option<f64>, fps: Option<Framerate<usize>>) -> Self {
        Self {
            pipe,
            start_offset,
            fps,
        }
    }

    /// Uses the builder API to create a Pipeline
    pub fn builder<S: AsRef<str>>(uri: S) -> SignPipelineBuilder {
        SignPipelineBuilder::from_uri(uri)
    }

    /// This consumes the object and converts it into an iterator over every
    /// frame with the given [VideoFrame] type.
    ///
    /// Parts of this function was inspired by [`vid_frame_iter`](https://github.com/Farmadupe/vid_dup_finder_lib/blob/main/vid_frame_iter)
    pub fn try_into_iter<RF: VideoFrame>(self) -> Result<VideoFrameIter<RF>, VideoError> {
        let pipeline = VideoFrameIter::<RF> {
            pipeline: self.pipe,
            fused: false,
            _phantom: std::marker::PhantomData,
        };
        pipeline.pause()?;

        if let Some(skip_amount) = self.start_offset {
            pipeline.seek_accurate(skip_amount)?;
        }

        pipeline.play()?;
        Ok(pipeline)
    }

    /// This tries to create an iterator through all the frames in the pipeline
    /// converting it to a given [VideoFrame] type.
    ///
    /// This does not consume the pipeline, instead opting to clone it
    ///
    /// Parts of this function was inspired by [`vid_frame_iter`](https://github.com/Farmadupe/vid_dup_finder_lib/blob/main/vid_frame_iter)
    pub fn try_iter<RF: VideoFrame>(&self) -> Result<VideoFrameIter<RF>, VideoError> {
        let appsink = self
            .pipe
            .by_name("sink")
            .expect("Sink element not found")
            .downcast::<gstreamer_app::AppSink>()
            .expect("Sink element is expected to be an appsink!");

        // Tell the appsink what format we want.
        // This can be set after linking the two objects, because format negotiation between
        // both elements will happen during pre-rolling of the pipeline.
        appsink.set_caps(Some(
            &gstreamer::Caps::builder("video/x-raw")
                .field("format", RF::gst_video_format().to_str())
                .build(),
        ));

        let pipeline = VideoFrameIter::<RF> {
            pipeline: self.pipe.clone(),
            fused: false,
            _phantom: std::marker::PhantomData,
        };
        pipeline.pause()?;

        if let Some(skip_amount) = self.start_offset {
            pipeline.seek_accurate(skip_amount)?;
        }

        pipeline.play()?;
        Ok(pipeline)
    }

    fn get_start_frame(
        &self,
        buf_len: usize,
        fps: Framerate<usize>,
        excess: usize,
        start: Timestamp,
        at: Timestamp,
        i: usize,
    ) -> Result<usize, VideoError> {
        buf_len
            .checked_sub(i - excess - start.into_frames(fps, self.start_offset))
            .ok_or(VideoError::OutOfRange(start, at))
    }

    fn frames_to_buffer<'a, I: Iterator<Item = &'a RgbFrame>>(
        buf_len: usize,
        start_idx: usize,
        size: Coord,
        it: I,
    ) -> Vec<u8> {
        let mut frames_buf: Vec<u8> =
            Vec::with_capacity(size.x as usize * size.y as usize * (buf_len - start_idx));

        for f in it {
            // TODO: Deal with the sign crop and stuff
            frames_buf.extend_from_slice(
                f.as_flat().as_slice(), //.ok_or(VideoError::OutOfRange((start, timestamp)))?,
            );
        }

        frames_buf
    }

    // TODO: Swap to take in object
    fn get_buffer_for(
        &self,
        size: Coord,
        frame_buffer: &VecDeque<RgbFrame>,
        fps: Framerate<usize>,
        frame_idx: usize,
        excess: usize,
        start: Timestamp,
        at: Timestamp,
    ) -> Result<Vec<u8>, VideoError> {
        let start_idx =
            self.get_start_frame(frame_buffer.len(), fps, excess, start, at, frame_idx)?;

        // TODO: Potentially optimise by grouping sign info with same
        // bounds
        // TODO: Include audio

        Ok(Self::frames_to_buffer(
            frame_buffer.len(),
            start_idx,
            size,
            frame_buffer.range(start_idx..frame_buffer.len()),
        ))
    }
}

#[cfg(feature = "verifying")]
impl SignPipeline {
    // TODO: Swap to iterator
    pub fn verify<'a>(
        &'a self,
        resolver: Resolver,
        signfile: &'a SignFile,
        // TODO: Change to FrameError (separating VideoError + FrameError)
    ) -> Result<impl Stream<Item = Result<Pin<Box<VerifiedFrame>>, VideoError>> + use<'a>, VideoError>
    {
        const BUF_CAPACITY: usize = MAX_CHUNK_LENGTH * 10;

        let fps = self.fps.unwrap_or_default();

        let iter = self.try_iter::<RgbFrame>()?.enumerate();
        let delayed = DelayedStream::<_, _>::new(MAX_CHUNK_LENGTH, stream::iter(iter));

        let state_cache: Arc<Mutex<HashMap<(Timestamp, &Vec<u8>), SignatureState>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // TODO: THERE ARE WAYYY TOO MANY CLONES AHHHH
        // TODO: This must be passed in
        let credentials = Arc::new(Mutex::new(CredentialStore::new(resolver)));
        let res = delayed
            .filter_map(|d_info| async {
                match d_info {
                    Delayed::Partial(_) => None,
                    Delayed::Full(a, fut) => Some((a.0, a, fut)),
                }
            })
            .then(move |(i, frame, buffer)| {
                let credentials = credentials.clone();
                let state_cache = state_cache.clone();
                async move {
                    let frame = match &frame.1 {
                        Ok(f) => f.clone(), // TODO: Probs try and improve this
                        Err(e) => return Err(e.clone().into()),
                    };

                    let size = Coord::new(frame.width(), frame.height());

                    let (timestamp, excess_frames) =
                        Timestamp::from_frames(i, fps, self.start_offset);

                    let buf_ref = &buffer;

                    let sigs_iter = stream::iter(signfile.get_signatures_at(timestamp).into_iter())
                        .map(|sig| {
                            let credentials = credentials.clone();
                            let state_cache = state_cache.clone();
                            let frame_ref = &frame;
                            async move {
                                let start = sig.range.start;
                                let end = sig.range.end;
                                let sig = sig.signature;
                                let cache_key = (start, &sig.signature);

                                let mut state_cache = state_cache.lock().await;

                                // Due to us having to pass to the output of the iterator
                                let state = match state_cache.get(&cache_key) {
                                    Some(s) => s.clone(),
                                    None => {
                                        // If there is nothing in the cache we
                                        // will assume this is the first frame
                                        // for which it is signed for
                                        let start_frame = 0;

                                        // TODO: Check if the end frame is after 10 second mark
                                        let end_frame = end.into_frames(fps, self.start_offset)
                                            - start.into_frames(fps, self.start_offset);

                                        let frames_buf = Self::frames_to_buffer(
                                            buf_ref.len(),
                                            start_frame,
                                            size,
                                            // TODO: This potentially also needs to include the
                                            // current frame
                                            vec![frame_ref].into_iter().chain(
                                                buf_ref[0..end_frame].iter().map(|a| {
                                                    // TODO: Check this
                                                    // It is safe to unwrap here due to the previous
                                                    // checks on the frames
                                                    let res = a.1.as_ref().unwrap();
                                                    res
                                                }),
                                            ),
                                        );

                                        let mut credentials = credentials.lock().await;

                                        let signer =
                                            credentials.normalise(sig.presentation.clone()).await;

                                        let state = SignatureState::from_signer(
                                            signer,
                                            VerificationInput {
                                                alg: JwsAlgorithm::EdDSA,
                                                signing_input: frames_buf.into_boxed_slice(),
                                                decoded_signature: sig
                                                    .signature
                                                    .clone()
                                                    .into_boxed_slice(),
                                            },
                                        );
                                        state_cache.insert(cache_key, state.clone());
                                        state
                                    }
                                };

                                Ok(state)
                            }
                        });

                    let signatures = sigs_iter
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
                        .await;

                    match signatures {
                        Ok(sigs) => Ok(Box::pin(VerifiedFrame { frame, sigs })),
                        Err(e) => Err(e),
                    }
                }
            });

        Ok(res)
    }
}

#[cfg(feature = "signing")]
impl SignPipeline {
    /// Signs the video with common chunk duration, while also partially
    /// handling credential information such that only one definition is made
    /// of the credential.
    ///
    /// For example if you wanted to sign in 100ms second intervals
    ///
    /// ```
    /// # use stream_signer::{SignPipeline, SignFile};
    ///
    /// # let pipeline = SignPipeline::builder("https://example.com/video_feed").build()?;
    /// # let credential: Credential = ...;
    /// # let keypair: Keypair = ...;
    ///
    /// # let sign_file = pipeline.sign_chunks(100, credential, keypair)?.collect::<SignFile>();
    ///
    /// # sign_file.write("./my_signatures.srt");
    /// ```
    pub async fn sign_chunks<'a, T: Into<Timestamp>, K, I>(
        &self,
        length: T,
        signer: &'a SignerInfo<'a, K, I>,
    ) -> Result<impl Iterator<Item = SignedChunk> + 'a, VideoError>
    where
        K: KeyBound,
        I: KeyIdBound,
    {
        let length = length.into();
        let mut is_start = true;

        self.sign(move |info| {
            if !info.time.is_start() && info.time % length == 0 {
                let res = vec![ChunkSigner::new(
                    info.time.start() - length,
                    signer,
                    !is_start,
                )];
                is_start = false;
                res
            } else {
                vec![]
            }
        })
        .await
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
    /// ```
    /// # use stream_signer::{SignPipeline, ChunkSigner};
    ///
    /// # let pipeline = SignPipeline::builder("https://example.com/video_feed").build()?;
    ///
    /// # let sign_file = pipeline.sign(|info| {
    /// #   // ...
    /// #   if !info.time.is_start() && info.time.multiple_of(100) {
    /// #     vec![
    /// #       ChunkSigner::new(time - 100, my_credential, my_keypair),
    /// #     ]
    /// #   } else {
    /// #     vec![]
    /// #   }
    /// # }).collect::<SignFile>();
    ///
    /// # sign_file.write("./my_signatures.srt")
    /// ```
    pub async fn sign<'a, K, I, F, ITER>(
        &self,
        mut sign_with: F,
    ) -> Result<impl Iterator<Item = SignedChunk> + 'a, VideoError>
    where
        K: KeyBound + 'a,
        I: KeyIdBound + 'a,
        F: FnMut(FrameInfo<'_>) -> ITER,
        ITER: IntoIterator<Item = ChunkSigner<'a, K, I>>,
    {
        let fps = self.fps.unwrap_or_default();
        let buf_capacity = fps.convert_to_frames(MAX_CHUNK_LENGTH);
        let mut frame_buffer: VecDeque<RgbFrame> = VecDeque::with_capacity(buf_capacity);

        let mut res: Vec<SignedChunk> = vec![];

        // TODO: Somehow swap to iter
        for (i, frame) in self.try_iter()?.enumerate() {
            let frame: RgbFrame = frame?;
            if frame_buffer.len() == buf_capacity {
                frame_buffer.pop_front();
            }
            let size = Coord::new(frame.width(), frame.height());

            let (timestamp, excess_frames) = Timestamp::from_frames(i, fps, self.start_offset);
            let sign_info = sign_with(FrameInfo::new(&frame, timestamp, i, fps));

            frame_buffer.push_back(frame);
            let mut chunks: HashMap<Timestamp, SignedChunk> = HashMap::default();
            for si in sign_info.into_iter() {
                // TODO: Add protections if the timeframe is too short
                let start = si.start;
                let frames_buf = self.get_buffer_for(
                    size,
                    &frame_buffer,
                    fps,
                    i,
                    excess_frames,
                    start,
                    timestamp,
                )?;

                let signature = si.sign(&frames_buf, size).await?;
                match chunks.get_mut(&start) {
                    Some(c) => c.val.push(signature),
                    None => {
                        chunks.insert(start, SignedChunk::new(start, timestamp, vec![signature]));
                    }
                }
            }
            res.extend(chunks.into_values());
        }

        Ok(res.into_iter())
    }
}

#[cfg(test)]
mod tests {
    use futures::future;
    use identity_iota::{core::FromJson, credential::Subject, did::DID};
    use serde_json::json;

    use super::*;

    use crate::tests::{
        client::{get_client, get_resolver},
        identity::TestIdentity,
        issuer::TestIssuer,
    };
    use std::error::Error;

    #[tokio::test]
    async fn sign_and_verify() -> Result<(), Box<dyn Error>> {
        println!("Hi there");
        gstreamer::init()?;
        println!("Initiated");

        let client = get_client();
        let issuer = TestIssuer::new(client.clone()).await?;
        let resolver = get_resolver(client);

        println!("Resolver trained");

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

        println!("Identity trained");

        let pipe =
            // SignPipeline::builder("./tests/videos/Big_Buck_Bunny_360_10s_1MB.mp4").build()?;
            SignPipeline::builder("https://test-videos.co.uk/vids/bigbuckbunny/mp4/av1/360/Big_Buck_Bunny_360_10s_1MB.mp4").build()?;

        println!("Pipeline built");

        let signfile = pipe
            .sign_chunks(100, &identity.gen_signer_info()?)
            .await?
            .collect::<SignFile>();

        println!("Signing completed");

        let pipe =
            // SignPipeline::builder("./tests/videos/Big_Buck_Bunny_360_10s_1MB.mp4").build()?;
            SignPipeline::builder("https://test-videos.co.uk/vids/bigbuckbunny/mp4/av1/360/Big_Buck_Bunny_360_10s_1MB.mp4").build()?;

        pipe.verify(resolver, &signfile)?
            .for_each(|v| {
                println!("uhhh");
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
                            (true, format!("Signature resolved correctly"))
                        }
                    };

                    assert!(is_ok, "{msg}");
                }

                future::ready(())
            })
            .await;

        assert!(false);

        println!("Verifying completed");

        Ok(())
    }
}
