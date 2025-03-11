use common_gui::{state::VideoState, video::VideoWidget};
use druid::{widget::ViewSwitcher, Widget, WidgetExt};

use crate::{
    menu,
    state::{AppData, View},
    video::VerifyPlayer,
};

pub fn make_ui() -> impl Widget<AppData> {
    ViewSwitcher::new(
        |data: &AppData, _env| data.view,
        |selector, _data, _env| match selector {
            View::MainMenu => Box::new(menu::make_menu_ui()),
            View::Video => Box::new(
                VideoWidget::new(VerifyPlayer::new().lens(VideoState::options))
                    .center()
                    .lens(AppData::video),
            ),
        },
    )
}
