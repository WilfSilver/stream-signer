use common_gui::{
    state::VideoState,
    video::{VideoFeedExt, VideoWidget},
    ShowIfExt,
};
use druid::{widget::ViewSwitcher, Widget, WidgetExt};

use crate::{
    menu,
    state::{AppData, View},
    video::{make_sig_list_overlay, VerifyFeed},
};

pub fn make_ui() -> impl Widget<AppData> {
    ViewSwitcher::new(
        |data: &AppData, _env| data.view,
        |selector, _data, _env| match selector {
            View::MainMenu => Box::new(menu::make_menu_ui()),
            View::Video => Box::new(
                VideoWidget::new(
                    VerifyFeed.lens(VideoState::options),
                    Some(
                        make_sig_list_overlay()
                            .lens(VideoState::options)
                            .show_if(VideoState::show_overlay),
                    ),
                )
                .center()
                .lens(AppData::video),
            ),
        },
    )
}
