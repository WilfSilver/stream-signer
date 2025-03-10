use druid::{widget::ViewSwitcher, Widget, WidgetExt};

use crate::{
    menu,
    state::{AppData, View},
    video::VideoWidget,
};

pub fn make_ui() -> impl Widget<AppData> {
    ViewSwitcher::new(
        |data: &AppData, _env| data.view,
        |selector, _data, _env| match selector {
            View::MainMenu => Box::new(menu::make_menu_ui()),
            View::Video => Box::new(VideoWidget::new().center().lens(AppData::video)),
        },
    )
}
