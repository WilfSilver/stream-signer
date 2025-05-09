use std::future::{self, Future};

use futures::{FutureExt, future::BoxFuture};
use tokio::sync::Mutex;

use crate::video::{FrameState, Signer, sign::ChunkSignerBuilder};

use super::Controller;

/// A wrapper for a basic callback function ([Fn]). The callback function is
/// called for every frame and is a replacement for [Controller::get_chunks].
///
/// This allows you to have near complete control over the signing process
/// relatively easily.
///
/// For example:
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
/// use std::time::Duration;
/// use stream_signer::{video::{sign::{self, ChunkSignerBuilder}, ChunkSigner, Signer}, SignPipeline, SignFile, TryStreamExt};
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
/// let controller = sign::FnController(move |state| {
///   let chunk_end = state.time.crosses_interval(Duration::from_millis(100));
///   if !state.time.is_start() && chunk_end.is_some() {
///     let res = vec![
///       ChunkSigner::build(chunk_end.unwrap() - Duration::from_millis(100), signer.clone()),
///     ];
///     res
///   } else {
///     vec![]
///   }
/// });
///
/// let sign_file = pipeline.sign_with(controller)
/// .expect("Failed to start stream")
/// .try_collect::<SignFile>()
/// .await
/// .expect("Failed to look at frame");
///
/// // ...
///
/// # Ok(())
/// # }
/// ```
pub struct FnController<S, F>(pub F)
where
    S: Signer + 'static,
    F: Fn(FrameState) -> Vec<ChunkSignerBuilder<S>> + Send + Sync;

impl<S, F> From<F> for FnController<S, F>
where
    S: Signer + 'static,
    F: Fn(FrameState) -> Vec<ChunkSignerBuilder<S>> + Send + Sync,
{
    fn from(value: F) -> Self {
        FnController(value)
    }
}

impl<S, F> Controller<S> for FnController<S, F>
where
    S: Signer + 'static,
    F: Fn(FrameState) -> Vec<ChunkSignerBuilder<S>> + Send + Sync,
{
    #[inline]
    fn get_chunks(&self, state: FrameState) -> BoxFuture<Vec<ChunkSignerBuilder<S>>> {
        future::ready(self.0(state)).boxed()
    }
}

/// Similar to [FnController] but allows the controller to return a future.
///
/// NOTE: This function is called on the same thread to the video playing and
/// so should not take longer than the time it takes for a single frame to be
/// visible for.
///
/// For example:
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
/// use futures::future;
/// use std::sync::Arc;
/// use std::time::Duration;
/// use stream_signer::{video::{sign::{self, ChunkSignerBuilder}, ChunkSigner, Signer}, SignPipeline, SignFile, TryStreamExt};
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
/// let controller = sign::AsyncFnController(move |state| {
///   let signer = signer.clone();
///   async move {
///     let chunk_end = state.time.crosses_interval(Duration::from_millis(100));
///     if !state.time.is_start() && chunk_end.is_some() {
///       let res = vec![
///         ChunkSigner::build(chunk_end.unwrap() - Duration::from_millis(100), signer.clone()),
///       ];
///
///       // ... await ...
///
///       res
///     } else {
///       vec![]
///     }
///   }
/// });
///
/// let sign_file = pipeline.sign_with(controller)
/// .expect("Failed to start stream")
/// .try_collect::<SignFile>()
/// .await
/// .expect("Failed to look at frame");
///
/// // ...
///
/// # Ok(())
/// # }
/// ```
pub struct AsyncFnController<S, F, FUT>(pub F)
where
    S: Signer + 'static,
    F: Fn(FrameState) -> FUT + Send + Sync,
    FUT: Future<Output = Vec<ChunkSignerBuilder<S>>> + Send;

impl<S, F, FUT> From<F> for AsyncFnController<S, F, FUT>
where
    S: Signer + 'static,
    F: Fn(FrameState) -> FUT + Send + Sync,
    FUT: Future<Output = Vec<ChunkSignerBuilder<S>>> + Send,
{
    fn from(value: F) -> Self {
        AsyncFnController(value)
    }
}

impl<S, F, FUT> Controller<S> for AsyncFnController<S, F, FUT>
where
    S: Signer + 'static,
    F: Fn(FrameState) -> FUT + Send + Sync,
    FUT: Future<Output = Vec<ChunkSignerBuilder<S>>> + Send,
{
    #[inline]
    fn get_chunks(&self, state: FrameState) -> BoxFuture<Vec<ChunkSignerBuilder<S>>> {
        self.0(state).boxed()
    }
}

/// Similar to [FnController] but has [FnMut] as the underlying trait,
/// utilising [Mutex] to be able to be sent between threads.
///
/// NOTE: This function is called on the same thread to the video playing and
/// so should not take longer than the time it takes for a single frame to be
/// visible for.
///
/// For example:
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
/// use futures::future;
/// use std::sync::Arc;
/// use std::time::Duration;
/// use stream_signer::{video::{sign::{self, ChunkSignerBuilder}, ChunkSigner, Signer}, SignPipeline, SignFile, TryStreamExt};
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
/// let controller = sign::FnMutController::new(move |state| {
///   let chunk_end = state.time.crosses_interval(Duration::from_millis(100));
///   if !state.time.is_start() && chunk_end.is_some() {
///     let res = vec![
///       ChunkSigner::build(chunk_end.unwrap() - Duration::from_millis(100), signer.clone())
///         .with_is_ref(!is_first),
///     ];
///     is_first = false;
///
///     res
///   } else {
///     vec![]
///   }
/// });
///
/// let sign_file = pipeline.sign_with(controller)
/// .expect("Failed to start stream")
/// .try_collect::<SignFile>()
/// .await
/// .expect("Failed to look at frame");
///
/// // ...
///
/// # Ok(())
/// # }
/// ```
pub struct FnMutController<S, F>(pub Mutex<F>)
where
    S: Signer + 'static,
    F: FnMut(FrameState) -> Vec<ChunkSignerBuilder<S>> + Send + Sync;

impl<S, F> From<F> for FnMutController<S, F>
where
    S: Signer + 'static,
    F: FnMut(FrameState) -> Vec<ChunkSignerBuilder<S>> + Send + Sync,
{
    fn from(value: F) -> Self {
        FnMutController(Mutex::new(value))
    }
}

impl<S, F> FnMutController<S, F>
where
    S: Signer + 'static,
    F: FnMut(FrameState) -> Vec<ChunkSignerBuilder<S>> + Send + Sync,
{
    pub fn new(func: F) -> Self {
        Self(Mutex::new(func))
    }
}

impl<S, F> Controller<S> for FnMutController<S, F>
where
    S: Signer + 'static,
    F: FnMut(FrameState) -> Vec<ChunkSignerBuilder<S>> + Sync + Send,
{
    #[inline]
    fn get_chunks(&self, state: FrameState) -> BoxFuture<Vec<ChunkSignerBuilder<S>>> {
        async move {
            let mut func = self.0.lock().await;
            func(state)
        }
        .boxed()
    }
}

/// Combination of [FnMutController] and [AsyncFnController] allowing for
/// mutable functions return asynchronous results.
pub struct AsyncFnMutController<S, F, FUT>(pub Mutex<F>)
where
    S: Signer + 'static,
    F: FnMut(FrameState) -> FUT,
    FUT: Future<Output = Vec<ChunkSignerBuilder<S>>> + Send;

impl<S, F, FUT> From<F> for AsyncFnMutController<S, F, FUT>
where
    S: Signer + 'static,
    F: FnMut(FrameState) -> FUT,
    FUT: Future<Output = Vec<ChunkSignerBuilder<S>>> + Send,
{
    fn from(value: F) -> Self {
        AsyncFnMutController(Mutex::new(value))
    }
}

impl<S, F, FUT> Controller<S> for AsyncFnMutController<S, F, FUT>
where
    S: Signer + 'static,
    F: FnMut(FrameState) -> FUT + Sync + Send,
    FUT: Future<Output = Vec<ChunkSignerBuilder<S>>> + Send,
{
    #[inline]
    fn get_chunks(&self, state: FrameState) -> BoxFuture<Vec<ChunkSignerBuilder<S>>> {
        async move {
            let mut func = self.0.lock().await;
            func(state).await
        }
        .boxed()
    }
}
