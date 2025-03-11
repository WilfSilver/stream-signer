use common_gui::video::VideoWidget;
use druid::{widget::ViewSwitcher, Widget, WidgetExt};

use crate::{
    menu,
    state::{AppData, View},
    video::SignPlayer,
};

pub fn make_ui() -> impl Widget<AppData> {
    ViewSwitcher::new(
        |data: &AppData, _env| data.view,
        |selector, _data, _env| match selector {
            View::MainMenu => Box::new(menu::make_menu_ui()),
            View::Video => Box::new(
                VideoWidget::new(SignPlayer::new())
                    .center()
                    .lens(AppData::video),
            ),
        },
    )
}
