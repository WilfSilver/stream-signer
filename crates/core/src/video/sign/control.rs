use std::{
    future::{self, Future},
    ops::Range,
    pin::Pin,
    sync::Arc,
};

use futures::FutureExt;

use crate::{file::Timestamp, spec::Coord, utils::TimeRange, video::FrameState};

use super::{ChunkSigner, Signer};

pub trait Controller<S: Signer> {
    fn get_chunk(
        &mut self,
        state: &FrameState,
    ) -> Pin<Box<dyn Future<Output = Option<ChunkSigner<S>>>>>;
}

pub trait SyncController<S: Signer> {
    fn get_chunk(&mut self, state: &FrameState) -> Option<ChunkSigner<S>>;
}

impl<S: Signer + 'static, C: SyncController<S>> Controller<S> for C {
    fn get_chunk(
        &mut self,
        state: &FrameState,
    ) -> Pin<Box<dyn Future<Output = Option<ChunkSigner<S>>>>> {
        future::ready(self.get_chunk(state)).boxed()
    }
}

#[derive(Debug)]
pub struct Embedding {
    pub pos: Coord,
    pub size: Coord,
}

#[derive(Debug)]
pub struct IntervalController<S: Signer> {
    pub embedding: Option<Embedding>,
    pub range: Option<Range<Timestamp>>,
    pub signer: Arc<S>,
    pub interval: Timestamp,

    is_start: bool,
}

impl<S: Signer> IntervalController<S> {
    pub fn build<T: Into<Timestamp>>(signer: Arc<S>, interval: T) -> Self {
        Self {
            embedding: None,
            range: None,
            signer,
            interval: interval.into(),
            is_start: true,
        }
    }

    pub fn with_embedding(mut self, pos: Coord, size: Coord) -> Self {
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

impl<S: Signer> SyncController<S> for IntervalController<S> {
    fn get_chunk(&mut self, state: &FrameState) -> Option<ChunkSigner<S>> {
        let time = self.convert_time(state.time)?;

        if !time.is_start() && (time % self.interval).is_first() {
            let mut res = ChunkSigner::new(
                state.time.start() - self.interval,
                self.signer.clone(),
                !self.is_start,
            );

            if let Some(emb) = &self.embedding {
                res = res.with_embedding(emb.pos, emb.size)
            }

            self.is_start = true;
            Some(res)
        } else {
            None
        }
    }
}
