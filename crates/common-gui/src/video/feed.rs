use std::{
    marker::PhantomData,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Poll, Waker},
};

use druid::{Env, Event, EventCtx, ExtEventSink, Lens, LensExt};
use stream_signer::{gst::glib::property::PropertySet, video::Pipeline};
use tokio::{runtime, sync::Mutex};

#[derive(Debug, Default, Clone)]
pub struct PlayerCtrl {
    paused: Arc<AtomicBool>,
    waker: Arc<Mutex<Option<Waker>>>,
}

impl PlayerCtrl {
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub async fn flip(&mut self) -> bool {
        let paused = self.paused.load(Ordering::Relaxed);
        self.paused.set(!paused);

        if let Some(waker) = self.waker.lock().await.take() {
            waker.wake();
        }

        !paused
    }

    pub async fn wait_if_paused(&mut self, pipe: &Pipeline) {
        if self.is_paused() {
            pipe.pause().expect("Failed to pause stream");
            self.await;
            pipe.play().expect("Failed to play stream");
        }
    }
}

impl Future for PlayerCtrl {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        if self.paused.load(Ordering::Relaxed) {
            runtime::Handle::current().block_on(async {
                let mut waker = self.waker.lock().await;
                *waker = Some(cx.waker().clone());
            });
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}

pub trait VideoFeed<T> {
    fn spawn_player(&self, event_sink: ExtEventSink, ctrl: PlayerCtrl, initial_state: T);

    fn handle_event(&mut self, ctx: &mut EventCtx, event: &Event, data: &mut T, env: &Env);
}

pub struct VideoFeedLens<A, B, L: Lens<A, B>, F: VideoFeed<B>> {
    lens: L,
    child: F,
    _phantom: PhantomData<(A, B)>,
}

impl<A, B: Clone, L: Lens<A, B>, F: VideoFeed<B>> VideoFeed<A> for VideoFeedLens<A, B, L, F> {
    fn spawn_player(&self, event_sink: ExtEventSink, ctrl: PlayerCtrl, initial_state: A) {
        self.child
            .spawn_player(event_sink, ctrl, self.lens.get(&initial_state))
    }

    fn handle_event(&mut self, ctx: &mut EventCtx, event: &Event, data: &mut A, env: &Env) {
        let child = &mut self.child;
        self.lens
            .with_mut(data, |data| child.handle_event(ctx, event, data, env));
    }
}

pub trait VideoFeedExt<B> {
    fn lens<A>(self, lens: impl Lens<A, B>) -> impl VideoFeed<A>;
}

impl<B: Clone, F: VideoFeed<B>> VideoFeedExt<B> for F {
    fn lens<A>(self, lens: impl Lens<A, B>) -> impl VideoFeed<A> {
        VideoFeedLens {
            lens,
            child: self,
            _phantom: PhantomData,
        }
    }
}
