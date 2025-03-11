use common_gui::state::VideoState;
use druid::{
    widget::{Button, Flex, TextBox},
    FileDialogOptions, FileSpec, Widget, WidgetExt,
};

use crate::{
    state::{AppData, View},
    video::VideoOptions,
};

pub fn make_menu_ui() -> impl Widget<AppData> {
    Flex::column()
        .with_child(make_video_picker())
        .with_child(make_signfile_picker())
        .with_child(
            Button::new("Go to video!")
                .on_click(|_event, data: &mut AppData, _env| {
                    data.view = View::Video;
                })
                .fix_width(150.)
                .padding(5.),
        )
        .center()
}

fn make_signfile_picker() -> impl Widget<AppData> {
    let sign_file = FileSpec::new("Sign file", &["ssrt"]);
    let save_dialog_options = FileDialogOptions::new()
        .allowed_types(vec![sign_file])
        .default_type(sign_file)
        .default_name("my_signed_video.ssrt")
        .name_label("Target")
        .title("Choose the target for your sign file")
        .button_text("Save to");

    Flex::row()
        .with_flex_child(
            TextBox::new()
                .lens(VideoOptions::output)
                .lens(VideoState::options)
                .lens(AppData::video)
                .fix_width(500.),
            1.0,
        )
        .with_child(
            Button::new("Save sign to")
                .on_click(move |ctx, _data: &mut AppData, _env| {
                    ctx.submit_command(
                        druid::commands::SHOW_SAVE_PANEL.with(save_dialog_options.clone()),
                    );
                })
                .fix_width(150.)
                .padding(5.),
        )
        .fix_width(750.)
        .fix_height(50.)
}

fn make_video_picker() -> impl Widget<AppData> {
    let video_format = FileSpec::new("Video format", &["mp4", "webm"]);
    let open_dialog_options = FileDialogOptions::new()
        .allowed_types(vec![video_format])
        .default_type(video_format)
        .default_name("MySavedFile.txt")
        .name_label("Source")
        .title("Where did you put that file?")
        .button_text("Open");

    Flex::row()
        .with_flex_child(
            TextBox::new()
                .lens(VideoOptions::src)
                .lens(VideoState::options)
                .lens(AppData::video)
                .fix_width(500.),
            1.0,
        )
        .with_child(
            Button::new("Open file")
                .on_click(move |ctx, _data: &mut AppData, _env| {
                    ctx.submit_command(
                        druid::commands::SHOW_OPEN_PANEL.with(open_dialog_options.clone()),
                    );
                })
                .fix_width(150.)
                .padding(5.),
        )
        .fix_width(750.)
        .fix_height(50.)
}
