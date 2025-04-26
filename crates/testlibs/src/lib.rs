pub mod client;
pub mod did;
pub mod identity;
pub mod issuer;

use std::env;

pub use anyhow;
pub use iota_sdk;
use rand::distr::SampleString;

pub mod videos {
    pub const BIG_BUNNY: &str = "Big_Buck_Bunny_360_10s_1MB.mp4";
    pub const BIG_BUNNY_LONG: &str = "Big_Buck_Bunny_20s_Silent.mp4";
    pub const BIG_BUNNY_1080: &str = "Big_Buck_Bunny_1080_10s_25fps.mp4";
    pub const BIG_BUNNY_AUDIO: &str = "Big_Buck_Bunny_10s_Audio.mp4";
    pub const BIG_BUNNY_AUDIO_6C: &str = "Big_Buck_Bunny_10s_Audio_6_channels.mp4";
    pub const BIG_BUNNY_AUDIO_LEFT: &str = "Big_Buck_Bunny_10s_Audio_Left.mp4";
    pub const BIG_BUNNY_AUDIO_RIGHT: &str = "Big_Buck_Bunny_10s_Audio_Right.mp4";
}

/// Returns the full URL to the given video in the `tests/videos` directory
pub fn test_video<S: AsRef<str>>(name: S) -> String {
    format!(
        "{}/tests/videos/{}",
        env::current_dir().unwrap().to_str().unwrap(),
        name.as_ref()
    )
}

pub fn random_temp_file() -> String {
    let dir = env::temp_dir();
    let file_name = rand::distr::Alphanumeric.sample_string(&mut rand::rng(), 32);
    dir.join(file_name).to_str().unwrap().to_owned()
}
