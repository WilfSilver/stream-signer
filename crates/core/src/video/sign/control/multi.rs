use futures::{future::BoxFuture, stream, FutureExt, StreamExt};

use crate::video::{ChunkSigner, FrameState, Signer};

use super::{Controller, SingleController};

pub struct MultiController<S: Signer + 'static>(Vec<Box<dyn SingleController<S> + Sync + Send>>);

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
