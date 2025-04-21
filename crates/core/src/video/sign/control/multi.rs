use futures::{future::BoxFuture, stream, FutureExt, StreamExt};

use crate::video::{ChunkSigner, FrameState, Signer};

use super::{Controller, SingleController};

/// This type allows for multiple [SingleController] of with the same [Signer]
/// to be used as one [Controller].
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
/// let controller = sign::MultiController(vec![
///     Box::new(sign::IntervalController::build(alice_signer, 100)),
///     Box::new(sign::IntervalController::build(bob_signer, 100)),
/// ]);
///
/// let sign_file = pipeline.sign_with(controller)
///     .expect("Failed to start stream")
///     .try_collect::<SignFile>()
///     .await
///     .expect("Failed to look at frame");
///
/// // ...
///
/// # Ok(())
/// # }
/// ```
pub struct MultiController<S: Signer + 'static>(
    pub Vec<Box<dyn SingleController<S> + Sync + Send>>,
);

impl<S: Signer + 'static> From<Vec<Box<dyn SingleController<S> + Sync + Send>>>
    for MultiController<S>
{
    fn from(value: Vec<Box<dyn SingleController<S> + Sync + Send>>) -> Self {
        MultiController(value)
    }
}

impl<S: Signer + 'static> Controller<S> for MultiController<S> {
    #[inline]
    fn get_chunks(&self, state: FrameState) -> BoxFuture<Vec<ChunkSigner<S>>> {
        stream::iter(self.0.iter())
            .filter_map(move |ctrl| ctrl.get_chunk(&state))
            .collect::<Vec<_>>()
            .boxed()
    }
}
