use druid::{
    Widget, WidgetExt,
    widget::{Svg, SvgData, ViewSwitcher},
};

pub fn make_play_btn() -> impl Widget<bool> {
    ViewSwitcher::new(
        |data: &bool, _env| *data,
        |selector, _data, _env| {
            if *selector {
                println!("Playing...");
                Box::new(read_svg(include_str!("./assets/pause.svg")))
            } else {
                println!("Pausing...");
                Box::new(read_svg(include_str!("./assets/play.svg")))
            }
        },
    )
}

fn read_svg(svg: &'static str) -> impl Widget<bool> {
    Svg::new(match svg.parse::<SvgData>() {
        Ok(svg) => svg,

        Err(err) => {
            println!("{}", err);

            println!("Using an empty SVG instead.");

            SvgData::default()
        }
    })
    .fix_size(150., 150.)
    .center()
}
