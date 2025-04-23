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
    pub const BIG_BUNNY_LONG: &str = "Big_Buck_Bunny_20s.mp4";
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
