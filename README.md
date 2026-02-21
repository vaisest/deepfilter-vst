# Deepfilter-VST

## Building

After installing [Rust](https://rustup.rs/), you can compile deepfilter-vst as follows:

```shell
cargo xtask bundle deepfilter-vst --release
```

This project is not very widely tested, but it should work. It does not yet have advanced configuration available. The only controls available are the attenuation limit, which determines how much the model is allowed to attenuate the incoming signal. This limit should not be set too low or the plugin will not do anything.
