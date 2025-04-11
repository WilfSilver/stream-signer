use futures::future::BoxFuture;

use crate::video::{ChunkSigner, FrameState, Signer};

pub trait Controller<S: Signer>: Send + Sync {
    fn get_chunks(&self, state: FrameState) -> BoxFuture<Vec<ChunkSigner<S>>>;
}
