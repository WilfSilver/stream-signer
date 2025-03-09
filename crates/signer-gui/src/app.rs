use druid::{widget::ViewSwitcher, Widget, WidgetExt};

use crate::{
    menu,
    state::{AppData, View},
    video::VideoWidget,
};

pub fn make_ui() -> impl Widget<AppData> {
    ViewSwitcher::new(
        |data: &AppData, _env| data.view,
        |selector, data, _env| match selector {
            View::MainMenu => Box::new(menu::make_menu_ui()),
            View::Video => Box::new(
                VideoWidget::new(data.event_sink.clone(), data.gen_video_options()).center(),
            ),
        },
    )
}
