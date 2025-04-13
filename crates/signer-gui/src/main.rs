mod app;
mod controller;
mod debug;
mod menu;
mod state;
mod video;

use druid::{AppLauncher, WindowDesc};
use state::{AppData, Delegate};

#[tokio::main]
pub async fn main() {
    let window = WindowDesc::new(app::make_ui()).title("Video Signer");

    let launcher = AppLauncher::with_window(window);

    let event_sink = launcher.get_external_handle();

    launcher
        .log_to_console()
        .delegate(Delegate)
        .launch(AppData::new(event_sink))
        .expect("launch failed");
}
