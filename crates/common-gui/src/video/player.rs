use std::{
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Poll, Waker},
};

use druid::{
    Data, Lens, LensExt, Widget,
    widget::{LensWrap, WidgetWrapper},
};
use futures::executor;
use stream_signer::{gst::glib::property::PropertySet, time::Timestamp, video::Pipeline};
use tokio::sync::Mutex;

use crate::state::VideoState;

#[derive(Debug, Default, Clone)]
pub struct Playing {
    paused: Arc<AtomicBool>,
    waker: Arc<Mutex<Option<Waker>>>,
}

impl Playing {
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub async fn flip(&mut self) {
        let paused = self.paused.load(Ordering::Relaxed);
        self.paused.set(!paused);

        if let Some(waker) = self.waker.lock().await.take() {
            println!("Calling waker");
            waker.wake();
        }
    }

    pub async fn wait_if_paused(&mut self, pipe: &Pipeline) {
        if self.is_paused() {
            pipe.pause().expect("Failed to pause stream");
            self.await;
            pipe.play().expect("Failed to play stream");
        }
    }
}

impl Future for Playing {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        println!("Polling...");
        if self.paused.load(Ordering::Relaxed) {
            executor::block_on(async {
                let mut waker = self.waker.lock().await;
                *waker = Some(cx.waker().clone());
            });
            println!("Paused");
            Poll::Pending
        } else {
            println!("Ready");
            Poll::Ready(())
        }
    }
}

#[derive(Debug, Default)]
pub struct PlayerState {
    pub playing: Playing,
    pub duration: Option<Timestamp>,
    pub pos: Timestamp,
}

impl PlayerState {
    pub fn set_duration(&mut self, duration: Timestamp) {
        self.duration = Some(duration);
    }

    pub fn set_pos(&mut self, pos: Timestamp) {
        self.pos = pos;
    }
}

pub trait VideoPlayer<T>: Widget<T> {
    fn spawn_player(
        &self,
        event_sink: druid::ExtEventSink,
        state: Arc<Mutex<PlayerState>>,
        initial_state: T,
    );
}

impl<T, L, W> VideoPlayer<VideoState<T>> for LensWrap<VideoState<T>, T, L, W>
where
    T: Sized + Data,
    W: VideoPlayer<T>,
    L: Lens<VideoState<T>, T>,
{
    fn spawn_player(
        &self,
        event_sink: druid::ExtEventSink,
        state: Arc<Mutex<PlayerState>>,
        initial_state: VideoState<T>,
    ) {
        self.wrapped()
            .spawn_player(event_sink, state, self.lens().get(&initial_state));
    }
}
