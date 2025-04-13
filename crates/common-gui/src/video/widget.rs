use std::marker::PhantomData;

use druid::{
    Code, Data, Event, LifeCycle, LifeCycleCtx, UnitPoint, Widget, WidgetExt, widget::ZStack,
};
use futures::executor;

use crate::{showif::ShowIfExt, state::VideoState};

use super::{VideoFeed, bar::make_bar, frame::FrameWidget, play_btn::make_play_btn};

pub struct VideoWidget<F: VideoFeed<VideoState<T>>, T: Clone + Data> {
    feed: F,
    inner: ZStack<VideoState<T>>,
    _phantom: PhantomData<T>,
}

impl<F: VideoFeed<VideoState<T>>, T: Clone + Data> VideoWidget<F, T> {
    pub fn new<O: Widget<VideoState<T>> + 'static>(feed: F, overlay_extra: Option<O>) -> Self {
        let mut gui_stack = ZStack::new(FrameWidget::default().lens(VideoState::curr_frame));

        if let Some(extra) = overlay_extra {
            gui_stack = gui_stack.with_aligned_child(extra, UnitPoint::TOP_RIGHT);
        }

        gui_stack = gui_stack
            .with_centered_child(
                make_play_btn()
                    .lens(VideoState::playing)
                    .show_if(VideoState::show_overlay),
            )
            .with_aligned_child(
                make_bar()
                    .lens(VideoState::progress)
                    .show_if(VideoState::show_overlay),
                UnitPoint::BOTTOM_LEFT,
            );

        VideoWidget {
            inner: gui_stack,
            feed,
            _phantom: PhantomData,
        }
    }

    async fn flip_playing(&mut self, data: &mut VideoState<T>) {
        data.playing = !data.ctrl.flip().await;
    }
}

impl<F: VideoFeed<VideoState<T>>, T: Clone + Data> Widget<VideoState<T>> for VideoWidget<F, T> {
    fn event(
        &mut self,
        ctx: &mut druid::EventCtx,
        event: &Event,
        data: &mut VideoState<T>,
        env: &druid::Env,
    ) {
        match event {
            // We use animation frame to get focus on start
            Event::AnimFrame(_) => ctx.request_focus(),
            Event::KeyDown(id) if id.code == Code::Space => {
                data.moved();
                executor::block_on(self.flip_playing(data));
                ctx.request_update(); // It sometimes doesn't believe the data has been changed
                ctx.set_handled();
            }
            Event::KeyDown(id) if id.code != Code::Enter => {
                data.moved();
            }
            Event::MouseMove(_) => {
                data.moved();
            }
            _ => {}
        }

        self.inner.event(ctx, event, data, env);
        self.feed.handle_event(ctx, event, data, env);
    }

    fn update(
        &mut self,
        ctx: &mut druid::UpdateCtx,
        old_data: &VideoState<T>,
        data: &VideoState<T>,
        env: &druid::Env,
    ) {
        if !old_data.same(data) {
            ctx.request_paint();
        }
        self.inner.update(ctx, old_data, data, env);
    }

    fn layout(
        &mut self,
        ctx: &mut druid::LayoutCtx,
        bc: &druid::BoxConstraints,
        data: &VideoState<T>,
        env: &druid::Env,
    ) -> druid::Size {
        self.inner.layout(ctx, bc, data, env);

        bc.max()
    }

    fn lifecycle(
        &mut self,
        ctx: &mut LifeCycleCtx,
        event: &LifeCycle,
        data: &VideoState<T>,
        env: &druid::Env,
    ) {
        if let LifeCycle::WidgetAdded = event {
            // Sneakily use this to gain focus
            ctx.request_anim_frame();

            let event_sink = ctx.get_external_handle();
            self.feed
                .spawn_player(event_sink, data.ctrl.clone(), data.clone());
        }

        self.inner.lifecycle(ctx, event, data, env);
    }

    fn paint(&mut self, ctx: &mut druid::PaintCtx, data: &VideoState<T>, env: &druid::Env) {
        self.inner.paint(ctx, data, env);
    }
}
