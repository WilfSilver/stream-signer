use futures::future::BoxFuture;

use crate::video::{ChunkSigner, FrameState, Signer};

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
/// use std::time::Duration;
/// use stream_signer::video::sign::{Controller, Signer};
///
/// struct MyController<S: Signer> {
///     pub Signer: Arc<S>,
///     pub interval: Duration;
///     /// Whether this the first signature used when signing so that we
///     /// can smartly determine whether to use a definition or presentation
///     is_start: AtomicBool,
/// }
///
/// impl<S: Signer + 'static> Controller<S> for MyController<S> {
///     fn get_chunks(&self, state: FrameState) -> BoxFuture<Vec<ChunkSigner<S>>> {
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
///                 let mut res = ChunkSigner::new(start, self.signer.clone())
///                     .with_is_ref(!self.is_start.load(Ordering::Relaxed));
///
///                 if let Some(emb) = &self.embedding {
///                     res = res.with_embedding(emb.pos, emb.size);
///                 }
///
///                 self.is_start.store(true, Ordering::Relaxed);
///
///                 vec![res]
///             } else {
///                 vec![]
///             }
///         })
///     }
/// }
/// ```
pub trait Controller<S: Signer>: Send + Sync {
    /// This function is called for every frame, given the [FrameState] with
    /// any information that might help the signing process
    ///
    /// The function can then choose to return some amount of [ChunkSigner]
    /// which defines a [crate::spec::ChunkSignature] that ends at the current
    /// frame.
    ///
    /// The [crate::spec::ChunkSignature] is then generated and returned from
    /// the [crate::video::SignPipeline::sign_with] function.
    ///
    /// NOTE: This function is called on the same thread as the frame
    /// consumption and so should not take longer than the time a frame is
    /// visible for
    fn get_chunks(&self, state: FrameState) -> BoxFuture<Vec<ChunkSigner<S>>>;
}
