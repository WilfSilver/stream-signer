use futures::future::BoxFuture;

use crate::video::{FrameState, Signer, sign::ChunkSignerBuilder};

/// This trait signifies an structure which can be used to control how a
/// signature is signed
///
/// ## Example
///
/// If you wish to create a more basic controller like the
/// [super::IntervalController], signing in chunks you can do as follows:
///
/// In this case you may also want to look at implementing
/// [super::SingleController] as well so it can be used in
/// [super::MultiController]
///
/// ```
/// use std::{sync::{atomic::{AtomicBool, Ordering}, Arc}, time::Duration};
/// use futures::{future::{self, BoxFuture}, FutureExt};
/// use stream_signer::video::{sign::{Controller, Signer, ChunkSignerBuilder}, ChunkSigner, FrameState};
///
/// struct MyController<S: Signer> {
///     pub signer: Arc<S>,
///     pub interval: Duration,
///     /// Whether this the first signature used when signing so that we
///     /// can smartly determine whether to use a definition or presentation
///     is_start: AtomicBool,
/// }
///
/// impl<S: Signer + 'static> Controller<S> for MyController<S> {
///     fn get_chunks(&self, state: FrameState) -> BoxFuture<Vec<ChunkSignerBuilder<S>>> {
///         future::ready({
///             let time = state.time;
///             if (!time.is_start() && (time % self.interval).is_first()) || state.is_last {
///                 let start = state
///                     .time
///                     .start()
///                     .checked_sub(self.interval)
///                     .unwrap_or_default()
///                     .into();
///
///                 let mut res = ChunkSigner::build(start, self.signer.clone())
///                     .with_is_ref(!self.is_start.load(Ordering::Relaxed));
///
///                 self.is_start.store(true, Ordering::Relaxed);
///
///                 vec![res]
///             } else {
///                 vec![]
///             }
///         }).boxed()
///     }
/// }
/// ```
pub trait Controller<S: Signer>: Send + Sync {
    /// This function is called for every frame, given the [FrameState] with
    /// any information that might help the signing process
    ///
    /// The function can then choose to return some amount of [ChunkSignerBuilder]
    /// which builds a [crate::video::ChunkSigner] which finally defines a [crate::spec::ChunkSignature]
    /// that ends at the current frame.
    ///
    /// The [crate::spec::ChunkSignature] is then generated and returned from
    /// the [crate::video::SignPipeline::sign_with] function.
    ///
    /// NOTE: This function is called on the same thread as the frame
    /// consumption and so should not take longer than the time a frame is
    /// visible for
    fn get_chunks(&self, state: FrameState) -> BoxFuture<Vec<ChunkSignerBuilder<S>>>;
}
