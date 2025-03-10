use futures::future::join_all;
use futures::{stream, Stream, StreamExt};
use gst::Pipeline;
use identity_iota::prelude::Resolver;
use identity_iota::storage::JwkStorageDocumentError;
use identity_iota::verification::jws::{JwsAlgorithm, VerificationInput};
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::spec::SignatureInfo;
use crate::video::Delayed;
use crate::{
    file::{SignedChunk, Timestamp},
    spec::Coord,
    time::ONE_SECOND_MILLIS,
};
use crate::{CredentialStore, SignFile};

#[cfg(feature = "signing")]
pub use super::sign::ChunkSigner;

#[cfg(feature = "verifying")]
use super::verify::{SignatureState, VerifiedFrame};

use super::{
    frame::Framerate, sample_iter::SampleIter, Frame, FrameInfo, SignPipelineBuilder, VideoError,
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
    pub fn build_from_path<P: AsRef<Path>>(path: &P) -> Option<SignPipelineBuilder> {
        SignPipelineBuilder::from_path(path)
    }

    pub fn build<'a, S: ToString>(uri: S) -> SignPipelineBuilder<'a> {
        SignPipelineBuilder::from_uri(uri)
    }

    /// This consumes the object and converts it into an iterator over every
    /// frame with the given [VideoFrame] type.
    ///
    /// Parts of this function was inspired by [`vid_frame_iter`](https://github.com/Farmadupe/vid_dup_finder_lib/blob/main/vid_frame_iter)
    pub fn try_into_iter(self) -> Result<SampleIter, VideoError> {
        let pipeline = SampleIter::new(self.pipe);
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
    pub fn try_iter(&self) -> Result<SampleIter, VideoError> {
        let pipeline = SampleIter::new(self.pipe.clone());
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

    fn frames_to_buffer<'a, I: Iterator<Item = &'a Frame>>(
        buf_len: usize,
        start_idx: usize,
        size: Coord,
        it: I,
    ) -> Vec<u8> {
        let mut frames_buf: Vec<u8> =
            Vec::with_capacity(size.x as usize * size.y as usize * (buf_len - start_idx));

        for f in it {
            // TODO: Deal with the sign crop and stuff
            frames_buf.extend_from_slice(f.raw_buffer());
        }

        frames_buf
    }

    // TODO: Swap to take in object
    fn get_buffer_for(
        &self,
        size: Coord,
        frame_buffer: &VecDeque<Frame>,
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
        let fps = self.fps.unwrap_or_default();

        let iter = self.try_iter()?.map(|r| r.map(Frame::from)).enumerate();
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
                    let frame: Frame = match &frame.1 {
                        Ok(f) => f.clone().into(),
                        Err(e) => return Err(e.clone().into()),
                    };

                    let size = Coord::new(frame.width(), frame.height());

                    let (timestamp, _) = Timestamp::from_frames(i, fps, self.start_offset);

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
                                            // TODO: Investigate why its end_frame - 1
                                            vec![frame_ref].into_iter().chain(
                                                buf_ref[0..end_frame - 1].iter().map(|a| {
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
        K: KeyBound + Sync,
        I: KeyIdBound + Sync,
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
        K: KeyBound + 'a + Sync,
        I: KeyIdBound + 'a + Sync,
        F: FnMut(FrameInfo) -> ITER,
        ITER: IntoIterator<Item = ChunkSigner<'a, K, I>>,
    {
        let fps = self.fps.unwrap_or_default();
        let buf_capacity = fps.convert_to_frames(MAX_CHUNK_LENGTH);
        let mut frame_buffer: VecDeque<Frame> = VecDeque::with_capacity(buf_capacity);

        let mut futures: Vec<(Timestamp, _)> = vec![];

        // TODO: Somehow swap to iter/stream
        for (i, frame) in self.try_iter()?.enumerate() {
            let frame: Frame = frame?.into();
            if frame_buffer.len() == buf_capacity {
                frame_buffer.pop_front();
            }
            let size = Coord::new(frame.width(), frame.height());

            let (timestamp, excess_frames) = Timestamp::from_frames(i, fps, self.start_offset);

            let sign_info = sign_with(FrameInfo::new(frame.clone(), timestamp, i, fps));

            frame_buffer.push_back(frame);
            let mut chunks: HashMap<
                Timestamp,
                Vec<Pin<Box<dyn Future<Output = Result<SignatureInfo, JwkStorageDocumentError>>>>>,
            > = HashMap::new();
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

                let fut = Box::pin(si.sign(frames_buf, size));

                match chunks.get_mut(&start) {
                    Some(c) => c.push(fut),
                    None => {
                        chunks.insert(start, vec![fut]);
                    }
                }
            }
            futures.push((timestamp, chunks));
        }

        let res = join_all(futures.into_iter().map(|(end, chunks)| async move {
            join_all(chunks.into_iter().map(|(start, futures)| async move {
                join_all(futures).await.into_iter().try_fold(
                    SignedChunk::new(start, end, vec![]),
                    |mut curr, signature| match signature {
                        Ok(sig) => {
                            curr.val.push(sig);
                            Ok(curr)
                        }
                        Err(e) => Err(e),
                    },
                )
            }))
            .await
        }))
        .await
        .into_iter()
        .flatten()
        .collect::<Result<Vec<SignedChunk>, _>>()?;

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
        test_video, videos,
    };
    use std::error::Error;

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
            .sign_chunks(100, &identity.gen_signer_info()?)
            .await?
            .collect::<SignFile>();

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
                            (true, format!("Signature resolved correctly"))
                        }
                    };

                    assert!(is_ok, "{msg}");
                }

                future::ready(())
            })
            .await;

        Ok(())
    }
}
