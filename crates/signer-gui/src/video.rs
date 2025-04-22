use std::{
    fs::File,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
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
    file::Timestamp, gst, video::pipeline::FramerateOption, SignFile, SignPipeline,
};
use testlibs::{client::get_client, identity::TestIdentity, issuer::TestIssuer};

use crate::{
    controller::SignController,
    state::{AppData, View},
};

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

        let sign_border_length = Duration::from_millis(500);
        let time_since_last_sign = info.time.start() - data.options.last_sign;
        if time_since_last_sign < sign_border_length {
            let sbl = sign_border_length.as_secs_f64();
            let rect = ctx.size().to_rect();
            ctx.stroke(
                rect,
                &Color::rgba(1., 0., 0., (sbl - time_since_last_sign.as_secs_f64()) / sbl),
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
        // let last_sign = Arc::new(Mutex::new(Timestamp::default()));
        let ctrl = SignController::new(signer, event_sink.clone(), options.sign_ctrl, ctrl);
        let signfile = pipe
            .sign_with(ctrl)
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
