use std::{array, collections::VecDeque, sync::Arc, task::Poll};

use futures::Stream;
use pin_project::pin_project;

#[derive(Debug, PartialEq)]
pub enum Ahead<const LEN: usize, T> {
    Partial(Box<[Arc<T>]>),
    Full([Arc<T>; LEN]),
}

#[derive(Debug, PartialEq)]
pub enum Delayed<const LEN: usize, T> {
    Partial(Box<[Arc<T>]>),
    Full(Arc<T>, [Arc<T>; LEN]),
}

pub struct DelayBuffer<const LEN: usize, T> {
    buf: VecDeque<Arc<T>>,
}

impl<const LEN: usize, T> DelayBuffer<LEN, T> {
    pub fn new() -> Self {
        Self {
            buf: {
                let mut buf = VecDeque::new();
                buf.reserve_exact(LEN);
                buf
            },
        }
    }

    pub fn is_filled(&self) -> bool {
        self.buf.len() == LEN
    }

    pub fn enqueue(&mut self, item: T) -> Option<Arc<T>> {
        let old = if self.is_filled() {
            self.buf.pop_front()
        } else {
            None
        };

        self.buf.push_back(Arc::new(item));

        old
    }

    pub fn get(&self) -> Ahead<LEN, T> {
        if self.is_filled() {
            Ahead::Full(array::from_fn::<_, LEN, _>(|i| {
                self.buf.get(i).cloned().unwrap()
            }))
        } else {
            Ahead::Partial({
                let mut v = Vec::new();
                v.reserve_exact(self.buf.len());

                self.buf.iter().cloned().for_each(|it| v.push(it));
                v.into_boxed_slice()
            })
        }
    }
}

impl<const LEN: usize, T> Default for DelayBuffer<LEN, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const LEN: usize, T> Clone for DelayBuffer<LEN, T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Self {
            buf: {
                let mut v = VecDeque::new();
                v.reserve_exact(LEN);
                v
            },
        }
    }
}

impl<const LEN: usize, T> Unpin for DelayBuffer<LEN, T> {}

#[pin_project]
pub struct DelayedStream<const LEN: usize, T, Src>
where
    Src: Stream<Item = T>,
{
    #[pin]
    source: Src,
    buffer: DelayBuffer<LEN, T>,
}

impl<const LEN: usize, T, Src> DelayedStream<LEN, T, Src>
where
    Src: Stream<Item = T>,
{
    pub fn new(source: Src) -> Self {
        Self {
            source,
            buffer: DelayBuffer::new(),
        }
    }
}

impl<const LEN: usize, T, Src> Stream for DelayedStream<LEN, T, Src>
where
    Src: Stream<Item = T>,
    T: Unpin,
{
    type Item = Delayed<LEN, T>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let p_self = self.project();
        let source = p_self.source;

        match source.poll_next(cx) {
            Poll::Ready(Some(it)) => {
                let Some(latest) = p_self.buffer.enqueue(it) else {
                    let items = match p_self.buffer.get() {
                        Ahead::Partial(items) => items,
                        Ahead::Full(items) => Box::new(items) as Box<[Arc<_>]>,
                    };

                    return Poll::Ready(Some(Delayed::Partial(items)));
                };

                let future = match p_self.buffer.get() {
                    Ahead::Partial(_) => unreachable!(),
                    Ahead::Full(future) => future,
                };
                Poll::Ready(Some(Delayed::Full(latest, future)))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures::{stream, StreamExt};
    use tokio::{pin, time::sleep};

    use super::DelayedStream;

    #[tokio::test]
    async fn test_queue() {
        let mut i = 0;
        let stream = stream::repeat_with(|| {
            i += 1;
            i
        })
        .take(100)
        .then(|i| async move {
            sleep(Duration::from_millis(25)).await;
            i
        });

        const LEN: usize = 10;

        let delayed = DelayedStream::<LEN, _, _>::new(stream);

        let filtered = delayed.filter_map(|a| async move {
            match a {
                super::Delayed::Partial(_) => None,
                super::Delayed::Full(init, fut) => Some((init, fut)),
            }
        });

        pin!(filtered);

        while let Some(thing) = filtered.next().await {
            println!("Current Value: {thing:?}");
        }
    }
}
