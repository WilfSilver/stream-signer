use std::{fs::File, ops::Deref, sync::Arc};

use common_gui::video::VideoPlayer;
use druid::{
    piet::RenderContext,
    widget::{Label, List, Painter, Scroll},
    BoxConstraints, Color, Data, Event, Lens, LifeCycle, LifeCycleCtx, PaintCtx, Widget, WidgetExt,
};
use futures::StreamExt;
use identity_iota::{core::FromJson, credential::Subject, did::DID};
use im::Vector;
use serde_json::json;
use stream_signer::{
    gst,
    video::{FramerateOption, SignatureState, UnverifiedSignature},
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
            (SignatureState::Unverified(s), SignatureState::Unverified(o)) => s.signer == o.signer,
            (SignatureState::Unresolved(_), SignatureState::Unresolved(_)) => true,
            (SignatureState::Verified(s), SignatureState::Verified(o)) => s == o,
            _ => false,
        };

        // println!("Same: {res} : {self:?} | {other:?}");

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

pub struct VerifyPlayer {
    pub sig_list: Box<dyn Widget<VideoOptions>>,
}

impl VideoPlayer<VideoOptions> for VerifyPlayer {
    fn spawn_player(&self, event_sink: druid::ExtEventSink, options: VideoOptions) {
        tokio::spawn(Self::watch_video(event_sink, options));
    }
}

impl VerifyPlayer {
    pub fn new() -> Self {
        let sig_list = Box::new(
            Scroll::new(
                List::new(|| {
                    Label::new(|item: &SigState, _env: &_| {
                        let name = match item.deref() {
                            SignatureState::Invalid(_) => "Invalid".to_string(),
                            SignatureState::Unresolved(_) => "Could not resolve".to_string(),
                            SignatureState::Verified(i)
                            | SignatureState::Unverified(UnverifiedSignature {
                                signer: i,
                                error: _,
                            }) => i.creds()[0]
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

                        println!("Label: {}", &name);

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
                                SignatureState::Invalid(_) | SignatureState::Unverified(_) => {
                                    Color::rgb(1., 0., 0.)
                                }
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
            .lens(VideoOptions::sigs),
        );

        VerifyPlayer { sig_list }
    }

    async fn watch_video(event_sink: druid::ExtEventSink, options: VideoOptions) {
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
            .verify(resolver, &signfile)
            .expect("Failed to start verifying");

        iter.for_each(|frame| async {
            let Ok(info) = frame else {
                return;
            };

            event_sink.add_idle_callback(move |data: &mut AppData| {
                data.video.curr_frame = Some(info.info.clone());
                data.video.options.sigs =
                    Vector::from_iter(info.sigs.iter().cloned().map(SigState));
            });
        })
        .await;

        // TODO: Have a button to return to the main menu
        event_sink.add_idle_callback(move |data: &mut AppData| {
            data.view = View::MainMenu;
        });
    }
}

impl Widget<VideoOptions> for VerifyPlayer {
    fn event(
        &mut self,
        ctx: &mut druid::EventCtx,
        event: &Event,
        data: &mut VideoOptions,
        env: &druid::Env,
    ) {
        self.sig_list.event(ctx, event, data, env);
    }

    fn update(
        &mut self,
        ctx: &mut druid::UpdateCtx,
        old_data: &VideoOptions,
        data: &VideoOptions,
        env: &druid::Env,
    ) {
        self.sig_list.update(ctx, old_data, data, env);
    }

    fn layout(
        &mut self,
        ctx: &mut druid::LayoutCtx,
        bc: &druid::BoxConstraints,
        data: &VideoOptions,
        env: &druid::Env,
    ) -> druid::Size {
        let margin = druid::Size::new(10., 10.);
        let inner_padding = BoxConstraints::new(bc.min() + margin, bc.max() - margin);
        self.sig_list.layout(ctx, &inner_padding, data, env);

        bc.max()
    }

    fn lifecycle(
        &mut self,
        ctx: &mut LifeCycleCtx,
        event: &LifeCycle,
        data: &VideoOptions,
        env: &druid::Env,
    ) {
        self.sig_list.lifecycle(ctx, event, data, env);
    }

    fn paint(&mut self, ctx: &mut druid::PaintCtx, data: &VideoOptions, env: &druid::Env) {
        self.sig_list.paint(ctx, data, env);
    }
}
