use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
    thread,
};

use common_gui::{state::VideoState, video::VideoPlayer};
use druid::{Code, Color, Data, Event, Lens, LifeCycle, LifeCycleCtx, RenderContext, Widget};
use futures::{executor::block_on, TryStreamExt};
use identity_iota::{
    core::FromJson,
    credential::Subject,
    did::DID,
    storage::{JwkMemStore, KeyIdMemstore},
};
use serde_json::json;
use stream_signer::{
    file::Timestamp,
    gst,
    tests::{client::get_client, identity::TestIdentity, issuer::TestIssuer},
    video::{ChunkSigner, FramerateOption, MAX_CHUNK_LENGTH},
    SignFile, SignPipeline,
};

use crate::state::{AppData, View};

#[derive(Clone, Data, Default, Lens)]
pub struct VideoOptions {
    pub src: String,
    pub output: String,
}

pub struct SignPlayer {
    sign_info: Arc<(AtomicBool, AtomicU32)>,
}

impl SignPlayer {
    pub fn new() -> Self {
        let sign_last_chunk = Arc::new((AtomicBool::new(false), AtomicU32::new(0)));

        SignPlayer {
            sign_info: sign_last_chunk,
        }
    }

    async fn watch_video(
        event_sink: druid::ExtEventSink,
        sign_last_chunk: Arc<(AtomicBool, AtomicU32)>,
        options: VideoOptions,
    ) {
        gst::init().expect("Failed gstreamer");

        let client = get_client();
        let issuer = TestIssuer::new(client.clone())
            .await
            .expect("Failed to create issuer");
        // let resolver = get_resolver(client);

        let identity = TestIdentity::new(&issuer, |id| {
            Subject::from_json_value(json!({
              "id": id.as_str(),
              "name": "Alice",
              "degree": {
                "type": "BachelorDegree",
                "name": "Bachelor of Science and Arts",
              },
              "GPA": "4.0",
            }))
            .expect("Invalid subject")
        })
        .await
        .expect("Failed to create identity");

        let pipe = SignPipeline::build_from_path(&options.src)
            .expect("Invalid path given")
            .with_frame_rate(FramerateOption::Auto)
            .build()
            .expect("Failed to build pipeline");

        let signer = identity
            .gen_signer_info()
            .expect("Failed to gen signer info");

        // Cache of the last sign, also stored in the AtomicU32 (but stored as
        // timestamp)
        let mut last_sign: Timestamp = 0.into();
        let signfile = pipe
            .sign::<JwkMemStore, KeyIdMemstore, _, _>(|info| {
                let time = info.time;

                event_sink.add_idle_callback(move |data: &mut AppData| {
                    data.video.update_frame(info);
                });

                // We want to sign when requested, or if the next frame is going to be past the
                // maximum chunk signing length
                let next_frame_time = *(time.start() - last_sign) as f64 + time.frame_duration();
                if sign_last_chunk.0.load(Ordering::Relaxed)
                    || next_frame_time >= MAX_CHUNK_LENGTH as f64
                {
                    let res = vec![ChunkSigner::new(last_sign, &signer, last_sign == 0.into())];

                    sign_last_chunk.0.store(false, Ordering::Relaxed);
                    last_sign = time.start();
                    sign_last_chunk.1.store(last_sign.into(), Ordering::Relaxed);

                    res
                } else {
                    vec![]
                }
            })
            .expect("Failed to initiate video")
            .try_collect::<SignFile>()
            .await
            .expect("Failed to sign frame");

        // TODO: Have a button to return to the main menu
        event_sink.add_idle_callback(move |data: &mut AppData| {
            signfile
                .write(&data.video.options.output)
                .expect("Failed to write signfile");
            data.view = View::MainMenu;
        });
    }
}

type State = VideoState<VideoOptions>;

impl VideoPlayer<State> for SignPlayer {
    fn spawn_player(&self, event_sink: druid::ExtEventSink, state: State) {
        let sign_info = self.sign_info.clone();
        thread::spawn(move || block_on(Self::watch_video(event_sink, sign_info, state.options)));
    }
}

impl Widget<State> for SignPlayer {
    fn event(
        &mut self,
        ctx: &mut druid::EventCtx,
        event: &Event,
        _data: &mut State,
        _env: &druid::Env,
    ) {
        if let Event::KeyDown(id) = event {
            if id.code == Code::Space {
                self.sign_info.0.store(true, Ordering::Relaxed);
                ctx.set_handled();
            }
        }
    }

    fn update(
        &mut self,
        _ctx: &mut druid::UpdateCtx,
        _old_data: &State,
        _data: &State,
        _env: &druid::Env,
    ) {
    }

    fn layout(
        &mut self,
        _ctx: &mut druid::LayoutCtx,
        bc: &druid::BoxConstraints,
        _data: &State,
        _env: &druid::Env,
    ) -> druid::Size {
        bc.max()
    }

    fn lifecycle(
        &mut self,
        _ctx: &mut LifeCycleCtx,
        _event: &LifeCycle,
        _data: &State,
        _env: &druid::Env,
    ) {
    }

    fn paint(&mut self, ctx: &mut druid::PaintCtx, data: &State, _env: &druid::Env) {
        let Some(info) = &data.curr_frame else {
            return;
        };

        let sign_border_length = 500;
        let time_since_last_sign = info.time.start() - self.sign_info.1.load(Ordering::Relaxed);
        if time_since_last_sign < sign_border_length.into() {
            let sbl = sign_border_length as f64;
            let rect = ctx.size().to_rect();
            ctx.stroke(
                rect,
                &Color::rgba(1., 0., 0., (sbl - *time_since_last_sign as f64) / sbl),
                50.,
            );
        }
    }
}
