mod app;
mod menu;
mod state;
mod video;

use druid::{AppLauncher, WindowDesc};
use state::{AppData, Delegate};

pub fn main() {
    let window = WindowDesc::new(app::make_ui()).title("External Event Demo");

    let launcher = AppLauncher::with_window(window);

    let event_sink = launcher.get_external_handle();

    launcher
        .log_to_console()
        .delegate(Delegate)
        .launch(AppData::new(event_sink))
        .expect("launch failed");
}
