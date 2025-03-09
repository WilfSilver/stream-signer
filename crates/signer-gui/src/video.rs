use std::{sync::Arc, thread};

use druid::{piet::CairoImage, Data, PaintCtx, Rect, RenderContext, Size, Widget};
use futures::executor::block_on;
use identity_iota::{
    core::FromJson,
    credential::Subject,
    did::DID,
    storage::{JwkMemStore, KeyIdMemstore},
};
use serde_json::json;
use stream_signer::{
    gstreamer,
    tests::{client::get_client, identity::TestIdentity, issuer::TestIssuer},
    video::{ChunkSigner, GenericImageView, ImageFns, RgbFrame},
    SignFile, SignPipeline,
};

use crate::state::{AppData, VideoOptions, View};

pub trait StateWithFrame: Data {
    fn get_curr_frame(&self) -> &Option<Frame>;

    fn get_curr_image(&self, ctx: &mut PaintCtx<'_, '_, '_>) -> Option<CairoImage> {
        match self.get_curr_frame() {
            Some(frame) => Some(
                ctx.make_image(
                    frame.width,
                    frame.height,
                    &frame.buf,
                    druid::piet::ImageFormat::Rgb,
                )
                .expect("Could not create buffer"),
            ),
            None => None,
        }
    }
}

// TODO: Add index to make it easier for PartialEq
#[derive(Clone, PartialEq)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub buf: Box<[u8]>,
}

impl From<&RgbFrame> for Frame {
    fn from(value: &RgbFrame) -> Self {
        let (width, height) = value.dimensions();

        let buf = value.as_flat().as_slice().to_owned().into_boxed_slice();

        Frame {
            width: width as usize,
            height: height as usize,
            buf,
        }
    }
}

impl Data for Frame {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

#[derive(Default)]
pub struct VideoWidget {}

impl VideoWidget {
    pub fn new(event_sink: Arc<druid::ExtEventSink>, options: VideoOptions) -> Self {
        thread::spawn(move || block_on(Self::watch_video(event_sink, options)));
        VideoWidget::default()
    }

    async fn watch_video(event_sink: Arc<druid::ExtEventSink>, options: VideoOptions) {
        gstreamer::init().expect("Failed gstreamer");

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

        let pipe = SignPipeline::builder(&options.url)
            .build()
            .expect("Failed to build pipeline");

        let signer = identity
            .gen_signer_info()
            .expect("Failed to gen signer info");

        let mut is_start = true;
        let signfile = pipe
            .sign::<JwkMemStore, KeyIdMemstore, _, _>(|info| {
                let frame: Frame = info.frame.into();

                event_sink.add_idle_callback(move |data: &mut AppData| {
                    data.video.curr_frame = Some(frame);
                });

                let length = 100.into();
                if !info.time.is_start() && info.time % length == 0 {
                    let res = vec![ChunkSigner::new(
                        info.time.start() - length,
                        &signer,
                        !is_start,
                    )];
                    is_start = false;
                    res
                } else {
                    vec![]
                }
            })
            .await
            .expect("Failed to sign pipeline")
            .collect::<SignFile>();

        // TODO: Have a button to return to the main menu
        event_sink.add_idle_callback(move |data: &mut AppData| {
            signfile
                .write(&data.signfile)
                .expect("Failed to write signfile");
            data.view = View::MainMenu;
        });
    }
}

impl<S: StateWithFrame> Widget<S> for VideoWidget {
    fn event(
        &mut self,
        _ctx: &mut druid::EventCtx,
        _event: &druid::Event,
        _data: &mut S,
        _env: &druid::Env,
    ) {
    }

    fn update(&mut self, ctx: &mut druid::UpdateCtx, old_data: &S, data: &S, _env: &druid::Env) {
        if !old_data.same(data) {
            ctx.request_paint();
        }
    }

    fn layout(
        &mut self,
        _ctx: &mut druid::LayoutCtx,
        bc: &druid::BoxConstraints,
        _data: &S,
        _env: &druid::Env,
    ) -> druid::Size {
        bc.max()
        // bc.constrain(
        //     data.get_curr_frame()
        //         .as_ref()
        //         .map(|f| (f.width as f64, f.height as f64))
        //         .unwrap_or((100., 100.)),
        // )
    }

    fn lifecycle(
        &mut self,
        _ctx: &mut druid::LifeCycleCtx,
        _event: &druid::LifeCycle,
        _data: &S,
        _env: &druid::Env,
    ) {
    }

    fn paint(&mut self, ctx: &mut druid::PaintCtx, data: &S, _env: &druid::Env) {
        if let Some(image) = data.get_curr_image(ctx) {
            let frame = data.get_curr_frame().as_ref().unwrap();
            let f_width = frame.width as f64;
            let f_height = frame.height as f64;

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
