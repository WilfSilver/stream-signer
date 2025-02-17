use crate::file::Timestamp;

pub enum VideoError {
    Glib(glib::Error),
    OutOfRange((Timestamp, Timestamp)),
}

impl From<glib::Error> for VideoError {
    fn from(value: glib::Error) -> Self {
        VideoError::Glib(value)
    }
}
