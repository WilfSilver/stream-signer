use std::{fs::File, ops::Deref, sync::Arc};

use common_gui::video::{PlayerCtrl, VideoFeed};
use druid::{
    piet::RenderContext,
    widget::{Label, List, Painter, Scroll},
    Color, Data, Event, ExtEventSink, Lens, PaintCtx, Widget, WidgetExt,
};
use futures::StreamExt;
use identity_iota::{core::FromJson, credential::Subject, did::DID};
use im::Vector;
use serde_json::json;
use stream_signer::{
    gst,
    video::{builder::FramerateOption, verify::SignatureState},
    SignFile, SignPipeline,
};
use testlibs::{
    client::{get_resolver, MemClient},
    identity::TestIdentity,
    issuer::TestIssuer,
};
use tokio::sync::Mutex;

use crate::state::{AppData, View};

/// Basic wrapper state so we can implement Data
#[derive(Debug, Clone)]
pub struct SigState(SignatureState);

impl Data for SigState {
    fn same(&self, other: &Self) -> bool {
        let res = match (self.deref(), other.deref()) {
            (SignatureState::Invalid(_), SignatureState::Invalid(_)) => true,
            (
                SignatureState::Unverified {
                    subject: s,
                    error: _,
                },
                SignatureState::Unverified {
                    subject: o,
                    error: _,
                },
            ) => s == o,
            (SignatureState::Unresolved(_), SignatureState::Unresolved(_)) => true,
            (SignatureState::Verified(s), SignatureState::Verified(o)) => s == o,
            _ => false,
        };

        res
    }
}

impl Deref for SigState {
    type Target = SignatureState;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Data, Default, Lens)]
pub struct VideoOptions {
    pub src: String,
    pub signfile: String,
    pub sigs: Vector<SigState>,
}

pub fn make_sig_list_overlay() -> impl Widget<VideoOptions> {
    Scroll::new(
        List::new(|| {
            Label::new(|item: &SigState, _env: &_| {
                let name = match item.deref() {
                    SignatureState::Invalid(_) => "Invalid".to_string(),
                    SignatureState::Unresolved(_) => "Could not resolve".to_string(),
                    SignatureState::Verified(i)
                    | SignatureState::Unverified {
                        subject: i,
                        error: _,
                    } => i.creds()[0]
                        .credential
                        .credential_subject
                        .first()
                        .unwrap()
                        .properties
                        .get("name")
                        .and_then(|v| v.as_str())
                        .map(ToString::to_string)
                        .unwrap_or("Unknown".to_string()),
                };

                name
            })
            // .align_vertical(UnitPoint::LEFT)
            .padding(10.)
            // .expand()
            .fix_size(150., 50.)
            .background(Painter::new(
                |ctx: &mut PaintCtx, data: &SigState, _env: &_| {
                    let color = match data.deref() {
                        SignatureState::Verified(_) => Color::rgb(0., 1., 0.),
                        SignatureState::Invalid(_)
                        | SignatureState::Unverified {
                            error: _,
                            subject: _,
                        } => Color::rgb(1., 0., 0.),
                        SignatureState::Unresolved(_) => Color::rgb(0.5, 0.5, 0.),
                    };

                    let bounds = ctx.size().to_rect();
                    ctx.fill(bounds, &color);
                },
            ))
        })
        .with_spacing(10.)
        .align_right(),
    )
    .vertical()
    .lens(VideoOptions::sigs)
}

pub struct VerifyFeed;

impl VideoFeed<VideoOptions> for VerifyFeed {
    fn spawn_player(&self, event_sink: ExtEventSink, ctrl: PlayerCtrl, options: VideoOptions) {
        tokio::spawn(Self::watch_video(event_sink, ctrl, options));
    }

    fn handle_event(
        &mut self,
        _ctx: &mut druid::EventCtx,
        _event: &Event,
        _data: &mut VideoOptions,
        _env: &druid::Env,
    ) {
    }
}

impl VerifyFeed {
    async fn watch_video(event_sink: ExtEventSink, ctrl: PlayerCtrl, options: VideoOptions) {
        gst::init().expect("Failed gstreamer");

        let client = {
            let f = File::open("client.json").unwrap();
            serde_json::from_reader::<_, MemClient>(f).unwrap()
        };

        let client = Arc::new(Mutex::new(client));
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
            .verify(&resolver, &signfile)
            .expect("Failed to start verifying");

        iter.for_each(|frame| {
            let event_sink = event_sink.clone();
            let mut ctrl = ctrl.clone();
            async move {
                let Ok(info) = frame else {
                    return;
                };

                // Wait until playing
                // We don't want to have the state locked while awaiting
                // the pause to stop
                ctrl.wait_if_paused(&info.state.pipe).await;

                event_sink.add_idle_callback(move |data: &mut AppData| {
                    data.video.update_frame(info.state.clone());
                    data.video.options.sigs =
                        Vector::from_iter(info.sigs.iter().cloned().map(SigState));
                });
            }
        })
        .await;

        // TODO: Have a button to return to the main menu
        event_sink.add_idle_callback(|data: &mut AppData| {
            data.view = View::MainMenu;
        });
    }
}
