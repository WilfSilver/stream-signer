use std::future::{self, Future};

use futures::{future::BoxFuture, FutureExt};
use tokio::sync::Mutex;

use crate::video::{ChunkSigner, FrameState, Signer};

use super::Controller;

pub struct FnController<S, F>(F)
where
    S: Signer + 'static,
    F: Fn(FrameState) -> Vec<ChunkSigner<S>> + Send + Sync;

impl<S, F> From<F> for FnController<S, F>
where
    S: Signer + 'static,
    F: Fn(FrameState) -> Vec<ChunkSigner<S>> + Send + Sync,
{
    fn from(value: F) -> Self {
        FnController(value)
    }
}

impl<S, F> Controller<S> for FnController<S, F>
where
    S: Signer + 'static,
    F: Fn(FrameState) -> Vec<ChunkSigner<S>> + Send + Sync,
{
    #[inline]
    fn get_chunks(&self, state: FrameState) -> BoxFuture<Vec<ChunkSigner<S>>> {
        future::ready(self.0(state)).boxed()
    }
}

pub struct AsyncFnController<S, F, FUT>(F)
where
    S: Signer + 'static,
    F: Fn(FrameState) -> FUT + Send + Sync,
    FUT: Future<Output = Vec<ChunkSigner<S>>> + Send;

impl<S, F, FUT> From<F> for AsyncFnController<S, F, FUT>
where
    S: Signer + 'static,
    F: Fn(FrameState) -> FUT + Send + Sync,
    FUT: Future<Output = Vec<ChunkSigner<S>>> + Send,
{
    fn from(value: F) -> Self {
        AsyncFnController(value)
    }
}

impl<S, F, FUT> Controller<S> for AsyncFnController<S, F, FUT>
where
    S: Signer + 'static,
    F: Fn(FrameState) -> FUT + Send + Sync,
    FUT: Future<Output = Vec<ChunkSigner<S>>> + Send,
{
    #[inline]
    fn get_chunks(&self, state: FrameState) -> BoxFuture<Vec<ChunkSigner<S>>> {
        self.0(state).boxed()
    }
}

pub struct FnMutController<S, F>(Mutex<F>)
where
    S: Signer + 'static,
    F: FnMut(FrameState) -> Vec<ChunkSigner<S>> + Send + Sync;

impl<S, F> From<F> for FnMutController<S, F>
where
    S: Signer + 'static,
    F: FnMut(FrameState) -> Vec<ChunkSigner<S>> + Send + Sync,
{
    fn from(value: F) -> Self {
        FnMutController(Mutex::new(value))
    }
}

impl<S, F> Controller<S> for FnMutController<S, F>
where
    S: Signer + 'static,
    F: FnMut(FrameState) -> Vec<ChunkSigner<S>> + Sync + Send,
{
    #[inline]
    fn get_chunks(&self, state: FrameState) -> BoxFuture<Vec<ChunkSigner<S>>> {
        async move {
            let mut func = self.0.lock().await;
            func(state)
        }
        .boxed()
    }
}

pub struct AsyncFnMutController<S, F, FUT>(Mutex<F>)
where
    S: Signer + 'static,
    F: FnMut(FrameState) -> FUT,
    FUT: Future<Output = Vec<ChunkSigner<S>>> + Send;

impl<S, F, FUT> From<F> for AsyncFnMutController<S, F, FUT>
where
    S: Signer + 'static,
    F: FnMut(FrameState) -> FUT,
    FUT: Future<Output = Vec<ChunkSigner<S>>> + Send,
{
    fn from(value: F) -> Self {
        AsyncFnMutController(Mutex::new(value))
    }
}

impl<S, F, FUT> Controller<S> for AsyncFnMutController<S, F, FUT>
where
    S: Signer + 'static,
    F: FnMut(FrameState) -> FUT + Sync + Send,
    FUT: Future<Output = Vec<ChunkSigner<S>>> + Send,
{
    #[inline]
    fn get_chunks(&self, state: FrameState) -> BoxFuture<Vec<ChunkSigner<S>>> {
        async move {
            let mut func = self.0.lock().await;
            func(state).await
        }
        .boxed()
    }
}
