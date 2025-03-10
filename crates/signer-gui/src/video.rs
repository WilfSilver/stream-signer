use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
    thread,
};

use druid::{
    piet::CairoImage, Code, Color, Data, Event, Lens, LifeCycle, LifeCycleCtx, PaintCtx, Rect,
    RenderContext, Size, Widget,
};
use futures::executor::block_on;
use identity_iota::{
    core::FromJson,
    credential::Subject,
    did::DID,
    storage::{JwkMemStore, KeyIdMemstore},
};
use serde_json::json;
use stream_signer::{
    file::Timestamp,
    gstreamer,
    tests::{client::get_client, identity::TestIdentity, issuer::TestIssuer},
    video::{
        ChunkSigner, FrameInfo, FramerateOption, GenericImageView, ImageFns, TimeRange,
        MAX_CHUNK_LENGTH,
    },
    SignFile, SignPipeline,
};

use crate::state::{AppData, View};

#[derive(Clone, Data, Default, Lens)]
pub struct VideoOptions {
    pub src: String,
    pub output: String,
}

#[derive(Clone, Data, Default, Lens)]
pub struct VideoData {
    pub curr_frame: Option<Frame>,
    pub options: VideoOptions,
}

impl VideoData {
    pub fn get_curr_image(&self, ctx: &mut PaintCtx<'_, '_, '_>) -> Option<CairoImage> {
        match &self.curr_frame {
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
    pub time: TimeRange,
    pub buf: Box<[u8]>,
}

impl<'a> From<FrameInfo<'a>> for Frame {
    fn from(value: FrameInfo<'a>) -> Self {
        let (width, height) = value.frame.dimensions();

        let buf = value
            .frame
            .as_flat()
            .as_slice()
            .to_owned()
            .into_boxed_slice();

        Frame {
            width: width as usize,
            height: height as usize,
            time: value.time,
            buf,
        }
    }
}

impl Data for Frame {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

pub struct VideoWidget {
    sign_info: Arc<(AtomicBool, AtomicU32)>,
}

impl VideoWidget {
    pub fn new() -> Self {
        let sign_last_chunk = Arc::new((AtomicBool::new(false), AtomicU32::new(0)));

        VideoWidget {
            sign_info: sign_last_chunk,
        }
    }

    async fn watch_video(
        event_sink: druid::ExtEventSink,
        sign_last_chunk: Arc<(AtomicBool, AtomicU32)>,
        options: VideoOptions,
    ) {
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

                let frame: Frame = info.into();

                event_sink.add_idle_callback(move |data: &mut AppData| {
                    data.video.curr_frame = Some(frame);
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
            .await
            .expect("Failed to sign pipeline")
            .collect::<SignFile>();

        // TODO: Have a button to return to the main menu
        event_sink.add_idle_callback(move |data: &mut AppData| {
            signfile
                .write(&data.video.options.output)
                .expect("Failed to write signfile");
            data.view = View::MainMenu;
        });
    }
}

impl Widget<VideoData> for VideoWidget {
    fn event(
        &mut self,
        ctx: &mut druid::EventCtx,
        event: &Event,
        _data: &mut VideoData,
        _env: &druid::Env,
    ) {
        match event {
            Event::KeyDown(id) => {
                if id.code == Code::Space {
                    self.sign_info.0.store(true, Ordering::Relaxed);
                    ctx.set_handled();
                }
            }
            // We use animation frame to get focus on start
            Event::AnimFrame(_) => ctx.request_focus(),
            _ => {}
        }
    }

    fn update(
        &mut self,
        ctx: &mut druid::UpdateCtx,
        old_data: &VideoData,
        data: &VideoData,
        _env: &druid::Env,
    ) {
        if !old_data.same(data) {
            ctx.request_paint();
        }
    }

    fn layout(
        &mut self,
        _ctx: &mut druid::LayoutCtx,
        bc: &druid::BoxConstraints,
        _data: &VideoData,
        _env: &druid::Env,
    ) -> druid::Size {
        bc.max()
    }

    fn lifecycle(
        &mut self,
        ctx: &mut LifeCycleCtx,
        event: &LifeCycle,
        data: &VideoData,
        _env: &druid::Env,
    ) {
        match event {
            LifeCycle::WidgetAdded => {
                // Sneakily use this to gain focus
                ctx.request_anim_frame();

                let event_sink = ctx.get_external_handle();
                let sign_info = self.sign_info.clone();
                let options = data.options.clone();
                thread::spawn(move || block_on(Self::watch_video(event_sink, sign_info, options)));
            }
            _ => {}
        }
    }

    fn paint(&mut self, ctx: &mut druid::PaintCtx, data: &VideoData, _env: &druid::Env) {
        if let Some(image) = data.get_curr_image(ctx) {
            let frame = data.curr_frame.as_ref().unwrap();
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

            let sign_border_length = 500;
            let time_since_last_sign =
                frame.time.start() - self.sign_info.1.load(Ordering::Relaxed);
            if time_since_last_sign < sign_border_length.into() {
                let sbl = sign_border_length as f64;
                ctx.stroke(
                    rect,
                    &Color::rgba(1., 0., 0., (sbl - *time_since_last_sign as f64) / sbl),
                    50.,
                );
            }
        }
    }
}
