use druid::{Data, Event, LifeCycle, LifeCycleCtx, Rect, RenderContext, Size, Widget};

use crate::state::FrameWithIdx;

#[derive(Debug, Default)]
pub struct FrameWidget {}

impl Widget<Option<FrameWithIdx>> for FrameWidget {
    fn event(
        &mut self,
        _ctx: &mut druid::EventCtx,
        _event: &Event,
        _data: &mut Option<FrameWithIdx>,
        _env: &druid::Env,
    ) {
    }

    fn update(
        &mut self,
        ctx: &mut druid::UpdateCtx,
        old_data: &Option<FrameWithIdx>,
        data: &Option<FrameWithIdx>,
        _env: &druid::Env,
    ) {
        if !old_data.same(data) {
            ctx.request_paint();
        }
    }

    fn layout(
        &mut self,
        _ctx: &mut druid::LayoutCtx,
        bc: &druid::BoxConstraints,
        _data: &Option<FrameWithIdx>,
        _env: &druid::Env,
    ) -> druid::Size {
        bc.max()
    }

    fn lifecycle(
        &mut self,
        _ctx: &mut LifeCycleCtx,
        _event: &LifeCycle,
        _data: &Option<FrameWithIdx>,
        _env: &druid::Env,
    ) {
    }

    fn paint(&mut self, ctx: &mut druid::PaintCtx, data: &Option<FrameWithIdx>, _env: &druid::Env) {
        if let Some(info) = data {
            let image = info.get_curr_image(ctx);

            let f_width = info.info.frame.width() as f64;
            let f_height = info.info.frame.height() as f64;

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
    }
}
