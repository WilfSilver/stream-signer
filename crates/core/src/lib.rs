//! This is a library for signing and verifying video streams.
//!
//! This was a part of my dissertation project, where I designed a
//! specification and wrote an example implementation with GUI's of how to sign
//! streams of videos to combat deepfakes. However it goes a step further to
//! allow "embeddings" within a video, meaning journalistic type videos can
//! verify their sources. As well as variable chunk length to allow the signer
//! to require context to be given when embedding it into another video.
//!
//! The [specification can be found on GitHub](https://github.com/WilfSilver/stream-signer/blob/main/spec.md)
//! with [the example GUI applications](https://github.com/WilfSilver/stream-signer/)
//!
//! This uses [Verifiable Data Model](https://www.w3.org/TR/vc-data-model-2.0/)
//! within the specification to determine the authenticity of a signature.
//!
//! To get started, please see [SignPipeline]
//!
//! For features, this defaults to have both `signing` and `verifying`, but
//! if an application only needs to sign, or only needs to verify a stream
//! you can exclude code required for the other by putting the following in
//! your `Cargo.toml`:
//!
//! ```toml
//! stream-signer = { version = "x.x.x", default-features = false, features = ["verifying"] }
//! ````
//!
//! Please note this is an experimental package to show that this concept could
//! work as a valid and more relaxed alternative to watermarking and concepts
//! like [JPEG Trust](https://jpeg.org/jpegtrust/index.html) or [C2PA](https://c2pa.org/)

pub mod audio;
pub mod file;
pub mod spec;
pub mod utils;
pub mod video;

pub use crate::file::{time, SignFile};
pub use video::SignPipeline;

pub use futures::TryStreamExt;

pub use gst;
