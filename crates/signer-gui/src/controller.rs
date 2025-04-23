use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use common_gui::video::PlayerCtrl;
use druid::ExtEventSink;
use futures::{future::BoxFuture, FutureExt};
use stream_signer::{
    spec::MAX_CHUNK_LENGTH,
    time::Timestamp,
    video::{sign::Controller, ChunkSigner, FrameState},
};
use testlibs::identity::TestIdentity;
use tokio::sync::Mutex;

use crate::state::AppData;

pub struct SignController {
    last_sign: Mutex<Timestamp>,
    signer: Arc<TestIdentity>,
    event_sink: ExtEventSink,
    sign_ctrl: Arc<AtomicBool>,
    player_ctrl: Mutex<PlayerCtrl>,
}

impl SignController {
    pub fn new(
        signer: Arc<TestIdentity>,
        event_sink: ExtEventSink,
        sign_ctrl: Arc<AtomicBool>,
        player_ctrl: PlayerCtrl,
    ) -> Self {
        Self {
            last_sign: Timestamp::default().into(),
            signer,
            event_sink,
            sign_ctrl,
            player_ctrl: player_ctrl.into(),
        }
    }
}

impl Controller<TestIdentity> for SignController {
    fn get_chunks(&self, state: FrameState) -> BoxFuture<Vec<ChunkSigner<TestIdentity>>> {
        async move {
            let time = state.time;

            // Wait until playing
            // We don't want to have the state locked while awaiting
            // the pause to stop
            self.player_ctrl
                .lock()
                .await
                .wait_if_paused(&state.pipe)
                .await;

            let is_last = state.is_last;
            self.event_sink
                .add_idle_callback(move |data: &mut AppData| {
                    data.video.update_frame(state);
                });

            let mut last_sign = self.last_sign.lock().await;

            // We want to sign when requested, or if the next frame is going to be past the
            // maximum chunk signing length
            let next_frame_time = time.start() - *last_sign + time.frame_duration();
            if self.sign_ctrl.load(Ordering::Relaxed)
                || next_frame_time >= MAX_CHUNK_LENGTH
                || is_last
            {
                let res = ChunkSigner::new(*last_sign, self.signer.clone())
                    .with_is_ref(*last_sign != Timestamp::ZERO);

                self.sign_ctrl.store(false, Ordering::Relaxed);
                *last_sign = time.start();

                self.event_sink
                    .add_idle_callback(move |data: &mut AppData| {
                        data.video.options.last_sign = time.start();
                    });

                vec![res]
            } else {
                Vec::new()
            }
        }
        .boxed()
    }
}
