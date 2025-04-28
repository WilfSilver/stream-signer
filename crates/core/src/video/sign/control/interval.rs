use std::{
    future,
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use futures::{FutureExt, future::BoxFuture};

use crate::{
    file::Timestamp,
    spec::Vec2u,
    utils::TimeRange,
    video::{ChunkSigner, FrameState, Signer, sign::ChunkSignerBuilder},
};

use super::{Controller, SingleController};

/// A simple structure to store the position and size of the vector as seen
/// in the [crate::spec::ChunkSignature]
#[derive(Debug)]
pub struct Embedding {
    pub pos: Vec2u,
    pub size: Vec2u,
}

/// A controller that creates chunks of a set size. Works in milliseconds due
/// to notes mentioned in [TimeRange::crosses_interval]
///
/// NOTE: the interval is not validated against the
/// [crate::spec::MAX_CHUNK_LENGTH] or [crate::spec::MIN_CHUNK_LENGTH], if
/// these are given it will cause issues within the signing process.
///
/// So, for example to sign the video in chunks of 100 milliseconds, the
/// following code can be used:
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
/// use std::time::Duration;
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
/// let controller = sign::IntervalController::build(signer, Duration::from_millis(100));
///
/// let sign_file = pipeline.sign_with(controller)
///     .expect("Failed to start stream")
///     .try_collect::<SignFile>()
///     .await
///     .expect("Failed to look at frame");
///
/// sign_file.write("./my_signatures.ssrt").expect("Failed to write signfile");
/// #
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct IntervalController<S: Signer> {
    /// The embedding size to then pass to [ChunkSigner], if [None] is given
    /// then the whole frame will be signed
    pub embedding: Option<Embedding>,
    /// The given time range a signature should apply to
    pub range: Option<Range<Timestamp>>,
    /// The audio channeles to pass to [ChunkSigner], if [None] is given then
    /// all channels will be signed
    pub channels: Option<Vec<usize>>,
    /// The signer, which can be shared over threads
    pub signer: Arc<S>,
    /// The length or size of the chunks, this should be between
    /// [crate::spec::MAX_CHUNK_LENGTH] and [crate::spec::MIN_CHUNK_LENGTH]
    pub interval: Duration,

    /// Whether this the first signature used when signing so we can smartly
    /// determine whether to use a definition or presentation
    is_start: AtomicBool,
}

impl<S: Signer> IntervalController<S> {
    pub fn build(signer: Arc<S>, interval: Duration) -> Self {
        Self {
            embedding: None,
            range: None,
            channels: None,
            signer,
            interval,
            is_start: true.into(),
        }
    }

    /// Sets the [Self::embedding]
    pub fn with_embedding(mut self, pos: Vec2u, size: Vec2u) -> Self {
        self.embedding = Some(Embedding { pos, size });
        self
    }

    /// Sets the [Self::channels]
    pub fn with_channels(mut self, channels: Vec<usize>) -> Self {
        self.channels = Some(channels);
        self
    }

    /// Sets the [Self::range]
    pub fn with_range(mut self, range: Range<Timestamp>) -> Self {
        self.range = Some(range);
        self
    }

    /// Converts the given time to be relative to [Self::range], if the
    /// time is outside the range, [None] will be returned
    fn convert_time(&self, time: TimeRange) -> Option<TimeRange> {
        match &self.range {
            Some(range) => {
                if time >= range.start && time <= range.end {
                    Some(time - range.start)
                } else {
                    None
                }
            }
            None => Some(time),
        }
    }
}

impl<S: Signer + 'static> Controller<S> for IntervalController<S> {
    #[inline]
    fn get_chunks(&self, state: FrameState) -> BoxFuture<Vec<ChunkSignerBuilder<S>>> {
        self.get_as_chunks(state)
    }
}

impl<S: Signer + 'static> SingleController<S> for IntervalController<S> {
    #[inline]
    fn get_chunk(&self, state: &FrameState) -> BoxFuture<Option<ChunkSignerBuilder<S>>> {
        let Some(time) = self.convert_time(state.time) else {
            return future::ready(None).boxed();
        };

        future::ready({
            let cross_point = time.crosses_interval(self.interval);
            // Note the is of is_last here means that the last chunk will
            // always overlap with another. This was mostly done to save time
            if (!(state.time - state.start_offset).is_start() && cross_point.is_some())
                || state.is_last
            {
                let chunk_end = cross_point.unwrap_or(state.time.end());
                let start = chunk_end
                    .checked_sub(self.interval)
                    .unwrap_or_default()
                    .into();

                let mut res = ChunkSigner::build(start, self.signer.clone())
                    .with_is_ref(!self.is_start.load(Ordering::Relaxed));

                if let Some(emb) = &self.embedding {
                    res = res.with_embedding(emb.pos, emb.size);
                }

                if let Some(channels) = &self.channels {
                    res = res.with_channels(channels.clone());
                }

                self.is_start.store(false, Ordering::Relaxed);
                Some(res)
            } else {
                None
            }
        })
        .boxed()
    }
}

// TODO:
// - Tests to make sure there is only one def
