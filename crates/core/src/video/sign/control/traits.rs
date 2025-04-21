use futures::future::BoxFuture;

use crate::video::{ChunkSigner, FrameState, Signer};

/// This trait signifies an structure which can be used to control how a
/// signature is signed
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
