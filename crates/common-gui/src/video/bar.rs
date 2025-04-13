use druid::{Widget, WidgetExt, widget::ProgressBar};

pub fn make_bar() -> impl Widget<f64> {
    ProgressBar.fix_height(25.).expand_width()
}
