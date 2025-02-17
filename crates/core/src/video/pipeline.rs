use glib::object::Cast;
use gstreamer::prelude::GstBinExt;
use gstreamer::Pipeline;
use identity_credential::credential::Credential;
use image::GenericImageView;
use pqc_dilithium::Keypair;
use std::collections::{HashMap, VecDeque};
use std::path::Path;

use crate::{
    file::{SignedChunk, Timestamp},
    spec::Coord,
    spec::CredentialId,
    time::ONE_SECOND_MILLIS,
    video::frame_iter::ImageFns,
};

#[cfg(feature = "signing")]
pub use super::sign::SignInfo;
use super::{
    frame_iter::{RgbFrame, VideoFrame, VideoFrameIter},
    framerate::Framerate,
    FrameInfo, SignPipelineBuilder, VideoError,
};
#[cfg(feature = "verifying")]
use pqc_dilithium::verify;

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
    pub fn builder<P: AsRef<Path>>(uri: String) -> SignPipelineBuilder {
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
    /// use sign_streamer::{SignPipeline};
    ///
    /// let pipeline = SignPipeline::builder("https://example.com/video_feed").build()?;
    /// let credential: Credential = ...;
    /// let keypair: Keypair = ...;
    ///
    /// let sign_file = pipeline.sign_chunks(100, credential, keypair).collect::<SignFile>();
    ///
    /// sign_file.write("./my_signatures.srt")
    ///
    /// ```
    pub fn sign_chunks<'a, T: Into<Timestamp>>(
        &self,
        length: T,
        cred: Credential,
        keypair: &'a Keypair,
    ) -> Result<impl Iterator<Item = SignedChunk> + 'a, VideoError> {
        let length = length.into();

        // TODO: Maybe change the id system (Because the id is a URL)...
        let cred_ref = CredentialId {
            id: cred.id.clone().unwrap().to_string(),
        };

        self.sign(move |info| {
            if !info.time.is_start() && info.time % length == 0 {
                if info.time.contains(length) {
                    vec![SignInfo::new(
                        info.time.start() - length,
                        cred.clone(),
                        keypair,
                    )]
                } else {
                    vec![SignInfo::new(
                        info.time.start() - length,
                        cred_ref.clone(),
                        keypair,
                    )]
                }
            } else {
                vec![]
            }
        })
    }

    /// Signs the current built video writing to the sign_file by calling
    /// the provided `sign_with` function for every frame with its timeframe
    /// and rgb frame.
    ///
    /// If the function returns some [SignInfo], it will use the information to
    /// sign the chunk form the `start` property to the current timestamp.
    ///
    /// For example if you wanted to sign a video in 100ms second intervals you
    /// could do the following
    ///
    /// ```
    /// use sign_streamer::{SignPipeline};
    ///
    /// let pipeline = SignPipeline::builder("https://example.com/video_feed").build()?;
    ///
    /// let sign_file = pipeline.sign(|info| {
    ///   ...
    ///   if !info.time.is_start() && info.time.multiple_of(100) {
    ///     vec![
    ///       SignInfo::new(time - 100, my_credential, my_keypair),
    ///     ]
    ///   } else {
    ///     vec![]
    ///   }
    /// }).collect::<SignFile>();
    ///
    /// sign_file.write("./my_signatures.srt")
    /// ```
    pub fn sign<'a, F, T>(
        &self,
        sign_with: F,
    ) -> Result<impl Iterator<Item = SignedChunk> + 'a, VideoError>
    where
        F: Fn(FrameInfo<'_>) -> T,
        T: IntoIterator<Item = SignInfo<'a>>,
    {
        let fps = self.fps.unwrap_or_default();
        let buf_capacity = fps.convert_to_frames(MAX_CHUNK_LENGTH);
        let mut frame_buffer: VecDeque<RgbFrame> = VecDeque::with_capacity(buf_capacity);

        let iter = self
            .try_iter()?
            .enumerate()
            .map(|(i, frame)| -> Result<_, VideoError> {
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

                    let i = frame_buffer
                        .len()
                        .checked_sub(i - excess_frames - start.into_frames(fps, self.start_offset))
                        .ok_or(VideoError::OutOfRange((start, timestamp)))?;

                    // TODO: Potentially optimise by grouping sign info with same
                    // bounds
                    // TODO: Include audio
                    let mut frames_buf: Vec<u8> = Vec::with_capacity(
                        size.x as usize * size.y as usize * (frame_buffer.len() - i),
                    );
                    for f in frame_buffer.range(i..frame_buffer.len()) {
                        // TODO: Deal with the sign crop and stuff
                        frames_buf.extend_from_slice(
                            f.as_flat().as_slice(), //.ok_or(VideoError::OutOfRange((start, timestamp)))?,
                        );
                    }

                    let signature = si.sign(&frames_buf, size);
                    match chunks.get_mut(&start) {
                        Some(c) => c.val.push(signature),
                        None => {
                            chunks
                                .insert(start, SignedChunk::new(start, timestamp, vec![signature]));
                        }
                    }
                }

                Ok(chunks.into_values())
            });

        let res = iter
            .collect::<Result<Vec<_>, VideoError>>()?
            .into_iter()
            .flatten();

        Ok(res)
    }
}
