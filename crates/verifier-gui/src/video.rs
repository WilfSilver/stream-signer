use std::thread;

use common_gui::video::VideoPlayer;
use druid::{Data, Event, Lens, LifeCycle, LifeCycleCtx, Widget};
use futures::{executor::block_on, StreamExt};
use identity_iota::{core::FromJson, credential::Subject, did::DID};
use serde_json::json;
use stream_signer::{gst, video::FramerateOption, SignFile, SignPipeline};
use testlibs::{
    client::{get_client, get_resolver},
    identity::TestIdentity,
    issuer::TestIssuer,
};

use crate::state::AppData;

#[derive(Clone, Data, Default, Lens)]
pub struct VideoOptions {
    pub src: String,
    pub signfile: String,
}

pub struct VerifyPlayer {}

impl VideoPlayer<VideoOptions> for VerifyPlayer {
    fn spawn_player(&self, event_sink: druid::ExtEventSink, options: VideoOptions) {
        thread::spawn(move || block_on(Self::watch_video(event_sink, options)));
    }
}

impl VerifyPlayer {
    pub fn new() -> Self {
        VerifyPlayer {}
    }

    async fn watch_video(event_sink: druid::ExtEventSink, options: VideoOptions) {
        gst::init().expect("Failed gstreamer");

        let client = get_client();
        let issuer = TestIssuer::new(client.clone())
            .await
            .expect("Failed to create issuer");

        // We have to still create the identities
        let _identity = TestIdentity::new(&issuer, |id| {
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

        let resolver = get_resolver(client);

        let pipe = SignPipeline::build_from_path(&options.src)
            .expect("Invalid path given")
            .with_frame_rate(FramerateOption::Auto)
            .build()
            .expect("Failed to build pipeline");

        let signfile = SignFile::from_file(options.signfile).expect("Failed to read signfile");

        let iter = pipe
            .verify(resolver, &signfile)
            .expect("Failed to start verifying");

        iter.for_each(|frame| async {
            let Ok(info) = frame else {
                return;
            };

            let frame = info.info.clone();

            event_sink.add_idle_callback(move |data: &mut AppData| {
                data.video.curr_frame = Some(frame);
            });
        })
        .await;
    }
}

impl Widget<VideoOptions> for VerifyPlayer {
    fn event(
        &mut self,
        _ctx: &mut druid::EventCtx,
        _event: &Event,
        _data: &mut VideoOptions,
        _env: &druid::Env,
    ) {
    }

    fn update(
        &mut self,
        _ctx: &mut druid::UpdateCtx,
        _old_data: &VideoOptions,
        _data: &VideoOptions,
        _env: &druid::Env,
    ) {
    }

    fn layout(
        &mut self,
        _ctx: &mut druid::LayoutCtx,
        bc: &druid::BoxConstraints,
        _data: &VideoOptions,
        _env: &druid::Env,
    ) -> druid::Size {
        bc.max()
    }

    fn lifecycle(
        &mut self,
        _ctx: &mut LifeCycleCtx,
        _event: &LifeCycle,
        _data: &VideoOptions,
        _env: &druid::Env,
    ) {
    }

    fn paint(&mut self, _ctx: &mut druid::PaintCtx, _data: &VideoOptions, _env: &druid::Env) {}
}
