/// Conversion functions to types provided by the popular[`image`] crate.
pub trait ImageFns {
    type IB;

    /// Get a [`image::FlatSamples`] for this frame with a borrowed reference to the underlying frame data.
    fn as_flat(&self) -> image::FlatSamples<&[u8]>;
}
