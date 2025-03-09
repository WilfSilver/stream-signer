pub mod client;
pub mod did;
pub mod identity;
pub mod issuer;

use std::env;

pub use iota_sdk;

pub mod videos {
    pub const BIG_BUNNY: &str = "Big_Buck_Bunny_360_10s_1MB.mp4";
}

/// Returns the full URL to the given video in the `tests/videos` directory
pub fn test_video<S: AsRef<str>>(name: S) -> String {
    format!(
        "file://{}/tests/videos/{}",
        env::current_dir().unwrap().to_str().unwrap(),
        name.as_ref()
    )
}
