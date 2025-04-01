use std::sync::Arc;

use druid::{Code, Data, Event, LifeCycle, LifeCycleCtx, Widget};
use futures::executor;
use tokio::sync::Mutex;

use crate::state::VideoState;

use super::PlayerState;

pub struct VideoOverlay {
    state: Arc<Mutex<PlayerState>>,
}

impl VideoOverlay {
    pub fn state(&self) -> Arc<Mutex<PlayerState>> {
        self.state.clone()
    }
}

impl Default for VideoOverlay {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(PlayerState::default())),
        }
    }
}

impl<T: Clone + Data> Widget<VideoState<T>> for VideoOverlay {
    fn event(
        &mut self,
        ctx: &mut druid::EventCtx,
        event: &Event,
        _data: &mut VideoState<T>,
        _env: &druid::Env,
    ) {
        match event {
            Event::KeyDown(id) if id.code == Code::Space => {
                executor::block_on(async {
                    let mut state = self.state.lock().await;
                    state.playing.flip().await;
                });
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn update(
        &mut self,
        _ctx: &mut druid::UpdateCtx,
        _old_data: &VideoState<T>,
        _data: &VideoState<T>,
        _env: &druid::Env,
    ) {
    }

    fn layout(
        &mut self,
        _ctx: &mut druid::LayoutCtx,
        bc: &druid::BoxConstraints,
        _data: &VideoState<T>,
        _env: &druid::Env,
    ) -> druid::Size {
        bc.max()
    }

    fn lifecycle(
        &mut self,
        _ctx: &mut LifeCycleCtx,
        _event: &LifeCycle,
        _data: &VideoState<T>,
        _env: &druid::Env,
    ) {
        // TODO: Show a button please
    }

    fn paint(&mut self, _ctx: &mut druid::PaintCtx, _data: &VideoState<T>, _env: &druid::Env) {}
}

impl Drop for VideoOverlay {
    fn drop(&mut self) {
        executor::block_on(async {
            let mut state = self.state.lock().await;
            if state.playing.is_paused() {
                state.playing.flip().await;
            }
        })
    }
}
