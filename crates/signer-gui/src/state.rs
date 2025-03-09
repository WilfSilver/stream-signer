use std::sync::Arc;

use druid::{
    commands, AppDelegate, Command, Data, DelegateCtx, Env, ExtEventSink, Handled, Lens, Target,
};

use crate::video::{Frame, StateWithFrame};

#[derive(Clone, Data)]
pub struct VideoOptions {
    pub url: String,
}

impl Default for VideoOptions {
    fn default() -> Self {
        Self {
            url: "https://test-videos.co.uk/vids/bigbuckbunny/mp4/av1/360/Big_Buck_Bunny_360_10s_1MB.mp4".to_string(),
        }
    }
}

#[derive(Clone, Data, Default)]
pub struct VideoData {
    pub curr_frame: Option<Frame>,
}

#[derive(Clone, Copy, Default, Data, PartialEq)]
pub enum View {
    #[default]
    MainMenu,
    Video,
}

#[derive(Clone, Data, Lens)]
pub struct AppData {
    pub view: View,
    pub video: VideoData,
    pub event_sink: Arc<ExtEventSink>,
    pub video_src: String,
    pub signfile: String,
}

impl AppData {
    pub fn new(event_sink: ExtEventSink) -> Self {
        Self {
            view: View::default(),
            video: VideoData::default(),
            event_sink: Arc::new(event_sink),
            video_src: String::new(),
            signfile: String::new(),
        }
    }

    pub fn gen_video_options(&self) -> VideoOptions {
        VideoOptions {
            url: self.video_src.clone(),
        }
    }
}

impl StateWithFrame for AppData {
    fn get_curr_frame(&self) -> &Option<Frame> {
        &self.video.curr_frame
    }
}

pub struct Delegate;

impl AppDelegate<AppData> for Delegate {
    fn command(
        &mut self,
        _ctx: &mut DelegateCtx,
        _target: Target,
        cmd: &Command,
        data: &mut AppData,
        _env: &Env,
    ) -> Handled {
        if let Some(file_info) = cmd.get(commands::SAVE_FILE_AS) {
            if let Some(path) = file_info.path().to_str() {
                data.signfile = path.into();
            } else {
                println!("Failed to open save location");
            }
            return Handled::Yes;
        }
        if let Some(file_info) = cmd.get(commands::OPEN_FILE) {
            if let Some(path) = file_info.path().to_str() {
                data.video_src = format!("file://{}", path);
            } else {
                println!("Failed to open file");
            }
            return Handled::Yes;
        }
        Handled::No
    }
}
