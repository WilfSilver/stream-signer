use std::sync::Arc;

use druid::{
    commands, AppDelegate, Command, Data, DelegateCtx, Env, ExtEventSink, Handled, Lens, Target,
};

use crate::video::VideoData;

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
}

impl AppData {
    pub fn new(event_sink: ExtEventSink) -> Self {
        Self {
            view: View::default(),
            video: VideoData::default(),
            event_sink: Arc::new(event_sink),
        }
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
                data.video.options.output = path.into();
            } else {
                println!("Failed to open save location");
            }
            return Handled::Yes;
        }
        if let Some(file_info) = cmd.get(commands::OPEN_FILE) {
            if let Some(path) = file_info.path().to_str() {
                data.video.options.src = path.to_string(); //format!("file://{}", path);
            } else {
                println!("Failed to open file");
            }
            return Handled::Yes;
        }
        Handled::No
    }
}
