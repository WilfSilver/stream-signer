use std::sync::Arc;

use druid::{Data, ExtEventSink};

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
    pub options: VideoOptions,
}

#[derive(Clone, Copy, Default, Data, PartialEq)]
pub enum View {
    #[default]
    MainMenu,
    Video,
}

#[derive(Clone, Data)]
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

impl StateWithFrame for AppData {
    fn get_curr_frame(&self) -> &Option<Frame> {
        &self.video.curr_frame
    }
}
