use std::marker::PhantomData;

use druid::{Data, Event, LifeCycle, LifeCycleCtx, Rect, RenderContext, Size, Widget};

use crate::state::VideoState;

use super::{VideoPlayer, overlay::VideoOverlay};

pub struct VideoWidget<C: VideoPlayer<T>, T: Clone + Data> {
    overlay: VideoOverlay,
    inner: C,
    _phantom: PhantomData<T>,
}

impl<C: VideoPlayer<T>, T: Clone + Data> VideoWidget<C, T> {
    pub fn new(inner: C) -> Self {
        VideoWidget {
            overlay: VideoOverlay::default(),
            inner,
            _phantom: PhantomData,
        }
    }
}

impl<C: VideoPlayer<VideoState<T>>, T: Clone + Data> Widget<VideoState<T>>
    for VideoWidget<C, VideoState<T>>
{
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
            _ => {}
        }

        self.overlay.event(ctx, event, data, env);
        self.inner.event(ctx, event, data, env);
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
        self.overlay.update(ctx, old_data, data, env);
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
        self.overlay.layout(ctx, bc, data, env);

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
            self.inner
                .spawn_player(event_sink, self.overlay.state().clone(), data.clone());
        }

        self.overlay.lifecycle(ctx, event, data, env);
        self.inner.lifecycle(ctx, event, data, env);
    }

    fn paint(&mut self, ctx: &mut druid::PaintCtx, data: &VideoState<T>, env: &druid::Env) {
        if let Some(image) = data.get_curr_image(ctx) {
            let info = data.curr_frame.as_ref().unwrap();
            let f_width = info.frame.width() as f64;
            let f_height = info.frame.height() as f64;

            let size = ctx.size();
            let s_width = size.width;
            let s_height = size.height;

            let h_scale = s_height / f_height;
            let w_scale = s_width / f_width;

            // We want to scale towards the smallest size, leaving black bars
            // everywhere else
            let scaler = w_scale.min(h_scale);
            let size = Size::new(f_width * scaler, f_height * scaler);
            // Center black bars
            let (x, y) = ((s_width - size.width) / 2., (s_height - size.height) / 2.);
            let rect = Rect::new(x, y, x + size.width, y + size.height);

            ctx.draw_image(&image, rect, druid::piet::InterpolationMode::Bilinear);
        }

        self.inner.paint(ctx, data, env);
        self.overlay.paint(ctx, data, env);
    }
}
