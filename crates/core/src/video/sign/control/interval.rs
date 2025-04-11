use std::{
    future,
    ops::Range,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use futures::{future::BoxFuture, FutureExt};

use crate::{
    file::Timestamp,
    spec::Vec2u,
    utils::TimeRange,
    video::{ChunkSigner, FrameState, Signer},
};

use super::{Controller, SingleController};

#[derive(Debug)]
pub struct Embedding {
    pub pos: Vec2u,
    pub size: Vec2u,
}

#[derive(Debug)]
pub struct IntervalController<S: Signer> {
    pub embedding: Option<Embedding>,
    pub range: Option<Range<Timestamp>>,
    pub signer: Arc<S>,
    pub interval: Timestamp,

    is_start: AtomicBool,
}

impl<S: Signer> IntervalController<S> {
    pub fn build<T: Into<Timestamp>>(signer: Arc<S>, interval: T) -> Self {
        Self {
            embedding: None,
            range: None,
            signer,
            interval: interval.into(),
            is_start: true.into(),
        }
    }

    pub fn with_embedding(mut self, pos: Vec2u, size: Vec2u) -> Self {
        self.embedding = Some(Embedding { pos, size });
        self
    }

    pub fn with_range(mut self, range: Range<Timestamp>) -> Self {
        self.range = Some(range);
        self
    }

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
    fn get_chunks(&self, state: FrameState) -> BoxFuture<Vec<ChunkSigner<S>>> {
        self.get_as_chunks(state)
    }
}

impl<S: Signer + 'static> SingleController<S> for IntervalController<S> {
    #[inline]
    fn get_chunk(&self, state: &FrameState) -> BoxFuture<Option<ChunkSigner<S>>> {
        let Some(time) = self.convert_time(state.time) else {
            return future::ready(None).boxed();
        };

        future::ready({
            if !time.is_start() && (time % self.interval).is_first() {
                let mut res = ChunkSigner::new(
                    state.time.start() - self.interval,
                    self.signer.clone(),
                    !self.is_start.load(Ordering::Relaxed),
                );

                if let Some(emb) = &self.embedding {
                    res = res.with_embedding(emb.pos, emb.size)
                }

                self.is_start.store(true, Ordering::Relaxed);
                Some(res)
            } else {
                None
            }
        })
        .boxed()
    }
}
