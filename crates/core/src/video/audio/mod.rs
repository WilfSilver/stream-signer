//! This contains wrappers around [gst_audio] to help attach sections of audio
//! to video frames so that they can be signed correctly

mod buffer;
mod frame;

pub use buffer::AudioBuffer;
pub use frame::AudioSlice;
