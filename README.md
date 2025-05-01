# Streamed Video Signer

This is a dissertation project based on creating a system for signing and
verifying streamed video, which allows for embedded "quotes" to keep their
signature while cryptographically preventing abuse.

For documentation, please see [stream-signer.wilfsilver.co.uk](https://stream-signer.wilfsilver.co.uk).

## Specification

A rough and basic specification is provided in [./spec.md](./spec.md),
however this is only used as a basis to plan the implementation itself

## Dev

### Building

For the requirements, they are the same as [`gstreamer-rs`](https://gstreamer.freedesktop.org/documentation/rust/stable/latest/docs/gstreamer/index.html).

So for ubuntu you would need to run:

```sh
sudo apt install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav libgstrtspserver-1.0-dev libges-1.0-dev libglib2.0-dev libgtk-3-dev
```

Once these are installed you should be able to just do

```sh
cargo build --release
```

### Testing

The test videos are stored on LFS, meaning that you need to install and pull `git-lfs` e.g. for ubuntu:

```sh
sudo apt install git-lfs
git lfs pull
```

Then to test both the integraiton and doc tests simply run:

```sh
cargo test
```

For benchmarking, again the built in `bench` command can be used:

```sh
cargo bench
```

### Running GUIs

To run the GUIs the following commands can be used from the root:

```sh
cargo run --release --bin signer-gui # For the signing application
cargo run --release --bin verifier-gui # For the verification application
```

### Importing the core library

This package is not published and so would have to be cloned and imported.

It is recommended that you look at the [docs](https://stream-signer.wilfsilver.co.uk) for more information
