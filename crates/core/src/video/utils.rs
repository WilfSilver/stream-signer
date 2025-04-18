use gst::MessageView;

/// Drain all messages from the bus, keeping track of eos and error.
/// (This prevents messages piling up and causing memory leaks)
pub fn get_bus_errors(bus: &gst::Bus) -> impl Iterator<Item = glib::Error> + '_ {
    let errs_warns = [gst::MessageType::Error, gst::MessageType::Warning];

    std::iter::from_fn(move || bus.pop_filtered(&errs_warns).map(into_glib_error))
}

pub(super) fn into_glib_error(msg: gst::Message) -> glib::Error {
    match msg.view() {
        MessageView::Error(e) => e.error(),
        MessageView::Warning(w) => w.error(),
        _ => {
            panic!("Only Warning and Error messages can be converted into GstreamerError")
        }
    }
}
