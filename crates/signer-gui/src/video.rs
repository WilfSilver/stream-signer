use std::{
    fs::File,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use std::io::prelude::*;

use common_gui::{
    state::VideoState,
    video::{PlayerCtrl, VideoFeed},
};
use druid::{
    Code, Color, Data, Event, ExtEventSink, Lens, LifeCycle, LifeCycleCtx, RenderContext, Widget,
};
use futures::TryStreamExt;
use identity_iota::{
    core::{FromJson, ToJson},
    credential::Subject,
    did::DID,
};
use serde_json::json;
use stream_signer::{
    file::Timestamp,
    gst,
    video::{builder::FramerateOption, ChunkSigner, MAX_CHUNK_LENGTH},
    SignFile, SignPipeline,
};
use testlibs::{client::get_client, identity::TestIdentity, issuer::TestIssuer};
use tokio::sync::Mutex;

use crate::state::{AppData, View};

#[derive(Clone, Data, Default, Lens)]
pub struct VideoOptions {
    pub src: String,
    pub output: String,
    #[data(eq)]
    pub last_sign: Timestamp,
    #[data(ignore)]
    pub sign_ctrl: Arc<AtomicBool>,
}

pub struct SignOverlay;

impl Widget<State> for SignOverlay {
    fn event(
        &mut self,
        _ctx: &mut druid::EventCtx,
        _event: &Event,
        _data: &mut State,
        _env: &druid::Env,
    ) {
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
        let time_since_last_sign = info.time.start() - data.options.last_sign;
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

pub struct SignFeed;

impl SignFeed {
    async fn watch_video(event_sink: ExtEventSink, ctrl: PlayerCtrl, options: VideoOptions) {
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

        let signer = Arc::new(identity);

        // Write the client to a file so we can access it later
        async {
            let c = client.lock().await;
            let mut output = File::create("client.json").unwrap();
            write!(output, "{}", c.to_json().unwrap()).unwrap();
        }
        .await;

        // Cache of the last sign, also stored in the AtomicU32 (but stored as
        // timestamp)
        let last_sign = Arc::new(Mutex::new(Timestamp::default()));
        let signfile = pipe
            .sign_async(|info| {
                let last_sign = last_sign.clone();
                let signer = signer.clone();
                let event_sink = event_sink.clone();
                let sign_ctrl = options.sign_ctrl.clone();
                let mut ctrl = ctrl.clone();

                async move {
                    let time = info.time;

                    // Wait until playing
                    // We don't want to have the state locked while awaiting
                    // the pause to stop
                    ctrl.wait_if_paused(&info.pipe).await;

                    event_sink.add_idle_callback(move |data: &mut AppData| {
                        data.video.update_frame(info);
                    });

                    let mut last_sign = last_sign.lock().await;

                    // We want to sign when requested, or if the next frame is going to be past the
                    // maximum chunk signing length
                    let next_frame_time =
                        *(time.start() - *last_sign) as f64 + time.frame_duration();
                    if sign_ctrl.load(Ordering::Relaxed)
                        || next_frame_time >= MAX_CHUNK_LENGTH as f64
                    {
                        let res = vec![ChunkSigner::new(
                            *last_sign,
                            signer.clone(),
                            *last_sign != 0.into(),
                        )];

                        sign_ctrl.store(false, Ordering::Relaxed);
                        *last_sign = time.start();

                        event_sink.add_idle_callback(move |data: &mut AppData| {
                            data.video.options.last_sign = time.start();
                        });

                        res
                    } else {
                        vec![]
                    }
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

impl VideoFeed<VideoOptions> for SignFeed {
    fn spawn_player(&self, event_sink: ExtEventSink, ctrl: PlayerCtrl, initial: VideoOptions) {
        tokio::spawn(Self::watch_video(event_sink, ctrl, initial));
    }

    fn handle_event(
        &mut self,
        ctx: &mut druid::EventCtx,
        event: &Event,
        data: &mut VideoOptions,
        _env: &druid::Env,
    ) {
        match event {
            Event::KeyDown(id) if id.code == Code::Enter => {
                data.sign_ctrl.store(true, Ordering::Relaxed);
                ctx.set_handled();
            }
            _ => {}
        }
    }
}
