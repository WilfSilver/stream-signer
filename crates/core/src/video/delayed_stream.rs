use std::{collections::VecDeque, sync::Arc, task::Poll};

use futures::Stream;
use pin_project::pin_project;

#[derive(Debug, PartialEq)]
pub enum Delayed<T> {
    Partial(Box<[Arc<T>]>),
    Full(Arc<T>, Box<[Arc<T>]>),
}

pub struct DelayBuffer<T> {
    buf: VecDeque<Arc<T>>,
    len: usize,
}

impl<T> DelayBuffer<T> {
    pub fn new(len: usize) -> Self {
        Self {
            buf: {
                let mut buf = VecDeque::new();
                buf.reserve_exact(len);
                buf
            },
            len,
        }
    }

    pub fn is_filled(&self) -> bool {
        self.buf.len() == self.len
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

    pub fn dequeue(&mut self) -> Option<Arc<T>> {
        self.buf.pop_front()
    }

    /// This returns the current items as a boxed slice.
    ///
    /// NOTE: This does not check if the buffer is full yet
    pub fn get_future_items(&self) -> Box<[Arc<T>]> {
        let mut v = Vec::new();
        v.reserve_exact(self.buf.len());

        self.buf.iter().cloned().for_each(|it| v.push(it));

        v.into_boxed_slice()
    }
}

impl<T> Clone for DelayBuffer<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Self {
            buf: {
                let mut v = VecDeque::new();
                v.reserve_exact(self.len);
                v
            },
            len: self.len,
        }
    }
}

impl<T> Unpin for DelayBuffer<T> {}

#[pin_project]
pub struct DelayedStream<T, Src>
where
    Src: Stream<Item = T>,
{
    #[pin]
    source: Src,
    buffer: DelayBuffer<T>,
}

impl<T, Src> DelayedStream<T, Src>
where
    Src: Stream<Item = T>,
{
    pub fn new(len: usize, source: Src) -> Self {
        Self {
            source,
            buffer: DelayBuffer::new(len),
        }
    }
}

impl<T, Src> Stream for DelayedStream<T, Src>
where
    Src: Stream<Item = T>,
    T: Unpin,
{
    type Item = Delayed<T>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let p_self = self.project();
        let source = p_self.source;

        match source.poll_next(cx) {
            Poll::Ready(Some(it)) => {
                let Some(latest) = p_self.buffer.enqueue(it) else {
                    let items = p_self.buffer.get_future_items();

                    return Poll::Ready(Some(Delayed::Partial(items)));
                };

                // NOTE: We don't have to worry about this not being full
                // as it is unreachable at this point
                let future = p_self.buffer.get_future_items();
                Poll::Ready(Some(Delayed::Full(latest, future)))
            }
            Poll::Ready(None) => {
                let latest = p_self
                    .buffer
                    .dequeue()
                    .map(|latest| Delayed::Full(latest, p_self.buffer.get_future_items()));
                Poll::Ready(latest)
            }
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

        let delayed = DelayedStream::<_, _>::new(LEN, stream);

        let filtered = delayed.filter_map(|a| async move {
            match a {
                super::Delayed::Partial(_) => None,
                super::Delayed::Full(init, fut) => Some((init, fut)),
            }
        });

        pin!(filtered);

        let mut i = 0;
        while let Some(thing) = filtered.next().await {
            println!("Thing {:?}", thing);
            i += 1;
            assert_eq!(*thing.0, i);
            for (j, v) in thing.1.iter().enumerate() {
                assert_eq!(**v, i + j + 1)
            }
        }
    }
}
