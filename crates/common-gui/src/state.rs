use std::time::{self, Duration};

use druid::{Data, Lens, PaintCtx, RenderContext, piet::CairoImage};
use stream_signer::{
    time::Timestamp,
    utils::TimeRange,
    video::{Frame, FrameState},
};

use crate::video::PlayerCtrl;

const HIDE_TIMEOUT: Duration = Duration::from_millis(2000);

/// This is here to act as a simple caching layer, making it quicker to
/// when running [Data::same]
#[derive(Clone, Data, Debug)]
pub struct FrameWithIdx {
    #[data(ignore)]
    pub frame: Frame,
    #[data(ignore)]
    pub time: TimeRange,
    pub idx: usize,
}

impl FrameWithIdx {
    pub fn get_curr_image(&self, ctx: &mut PaintCtx<'_, '_, '_>) -> CairoImage {
        ctx.make_image(
            self.frame.width() as usize,
            self.frame.height() as usize,
            self.frame.raw_buffer(),
            druid::piet::ImageFormat::Rgb,
        )
        .expect("Could not create buffer")
    }
}

impl From<&FrameState> for FrameWithIdx {
    fn from(value: &FrameState) -> Self {
        let idx = value.frame_idx();
        Self {
            frame: value.frame.clone(),
            time: value.time,
            idx,
        }
    }
}

impl From<FrameState> for FrameWithIdx {
    fn from(value: FrameState) -> Self {
        let idx = value.frame_idx();
        Self {
            frame: value.frame,
            time: value.time,
            idx,
        }
    }
}

#[derive(Clone, Data, Lens)]
pub struct VideoState<T>
where
    T: Clone + Data,
{
    pub curr_frame: Option<FrameWithIdx>,
    pub options: T,

    /// The time the cursor or something last moved so we can calculate when to
    /// show the overlay
    #[data(eq)]
    pub last_move: time::Instant,

    /// The duration of the whole video, which is only set once we start playing
    #[data(eq)]
    pub duration: Option<Timestamp>,

    /// This is directly calculated by curr_frame.time.start() / duration
    #[data(ignore)]
    pub progress: f64,

    /// Cache of the state of `ctrl` to make it easier to use within widget
    #[data(ignore)]
    pub playing: bool,
    #[data(ignore)]
    pub ctrl: PlayerCtrl,
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

    pub fn update_frame(&mut self, frame: FrameState) {
        self.duration = Some(frame.video.duration);
        self.progress = f64::from(frame.time.start()) / f64::from(frame.video.duration);
        self.curr_frame = Some(frame.into());
    }

    pub fn moved(&mut self) {
        self.last_move = time::Instant::now();
    }

    pub fn show_overlay(&self) -> bool {
        self.last_move.elapsed() < HIDE_TIMEOUT
    }
}

impl<T: Clone + Data + Default> Default for VideoState<T> {
    fn default() -> Self {
        Self {
            curr_frame: None,
            options: T::default(),

            playing: true,
            last_move: time::Instant::now(),
            duration: None,
            ctrl: PlayerCtrl::default(),
            progress: 0.,
        }
    }
}
