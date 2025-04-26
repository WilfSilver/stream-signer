use futures::{future::BoxFuture, FutureExt};

use crate::video::{sign::ChunkSignerBuilder, FrameState, Signer};

/// This has a slightly different interface from [super::Controller] and
/// focuses on allowing to have multiple [SingleController]s to be added
/// together to create multiple controllers with [super::MultiController].
///
/// It is expected all traits also implement [super::Controller] and there
/// is [SingleController::get_as_chunks] to help with this e.g.
///
/// ```
/// use stream_signer::video::{sign::{control::{Controller, SingleController}, ChunkSignerBuilder}, FrameState, Signer};
/// use futures::{future::BoxFuture, FutureExt};
///
/// struct MyStruct;
///
/// impl<S: Signer + 'static> SingleController<S> for MyStruct {
///     fn get_chunk(&self, state: &FrameState) -> BoxFuture<Option<ChunkSignerBuilder<S>>> {
///         async {
///             todo!()
///         }
///         .boxed()
///     }
/// }
///
/// impl<S: Signer + 'static> Controller<S> for MyStruct {
///     fn get_chunks(&self, state: FrameState) -> BoxFuture<Vec<ChunkSignerBuilder<S>>> {
///         self.get_as_chunks(state)
///     }
/// }
/// ```
pub trait SingleController<S: Signer + 'static> {
    /// Returns a single chunk as an [Option], if a chunk is returned it is
    /// then used to sign the video
    fn get_chunk(&self, state: &FrameState) -> BoxFuture<Option<ChunkSignerBuilder<S>>>;

    /// A simple wrapper to convert [SingleController::get_chunk] to a vector
    /// so that the type can then implement [super::Controller]
    fn get_as_chunks(&self, state: FrameState) -> BoxFuture<Vec<ChunkSignerBuilder<S>>> {
        self.get_chunk(&state).map(opt_to_vec).boxed()
    }
}

fn opt_to_vec<T>(o: Option<T>) -> Vec<T> {
    match o {
        Some(v) => vec![v],
        None => Vec::new(),
    }
}
