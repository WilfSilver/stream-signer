use druid::{
    widget::{Button, ViewSwitcher},
    Widget, WidgetExt,
};

use crate::{
    state::{AppData, View},
    video::VideoWidget,
};

pub fn make_ui() -> impl Widget<AppData> {
    ViewSwitcher::new(
        |data: &AppData, _env| data.view,
        |selector, data, _env| match selector {
            View::MainMenu => Box::new(
                Button::new("Go to video!")
                    .on_click(|_event, data: &mut AppData, _env| {
                        println!("Simple button clicked!");
                        data.view = View::Video;
                    })
                    .fix_size(150., 50.)
                    .padding(5.)
                    .center(),
            ),
            View::Video => Box::new(
                VideoWidget::new(data.event_sink.clone(), data.video.options.clone()).center(),
            ),
        },
    )
}
