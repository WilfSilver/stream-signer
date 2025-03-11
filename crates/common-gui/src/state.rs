use druid::{Data, Lens, PaintCtx, RenderContext, piet::CairoImage};
use stream_signer::video::FrameInfo;

#[derive(Clone, Lens)]
pub struct VideoState<T>
where
    T: Clone + Data,
{
    pub curr_frame: Option<FrameInfo>,
    pub options: T,
}

impl<T: Clone + Data> VideoState<T> {
    pub fn get_curr_image(&self, ctx: &mut PaintCtx<'_, '_, '_>) -> Option<CairoImage> {
        self.curr_frame.as_ref().map(|info| {
            ctx.make_image(
                info.frame.width() as usize,
                info.frame.height() as usize,
                info.frame.raw_buffer(),
                druid::piet::ImageFormat::Rgb,
            )
            .expect("Could not create buffer")
        })
    }

    pub fn update_frame(&mut self, frame: FrameInfo) {
        self.curr_frame = Some(frame);
    }
}

impl<T: Clone + Data> Data for VideoState<T> {
    fn same(&self, other: &Self) -> bool {
        self.options.same(&other.options)
            && match (&self.curr_frame, &other.curr_frame) {
                (Some(sf), Some(of)) => sf.idx() == of.idx(),
                (None, None) => true,
                _ => false,
            }
    }
}

impl<T: Clone + Data + Default> Default for VideoState<T> {
    fn default() -> Self {
        Self {
            curr_frame: None,
            options: T::default(),
        }
    }
}
