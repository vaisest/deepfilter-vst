# Deepfilter-VST

This repository contains a VST plug-in for the [DeepFilterNet](https://github.com/Rikorose/DeepFilterNet) machine learning model as the original model was only implemented as a LADSPA plugin. In some of my testing, this plugin had much improved performance compared to the LADSPA plugin, although I don't see a reason for this as the same model is used for both.

## Building

After installing [Rust](https://rustup.rs/), you can compile deepfilter-vst as follows:

```shell
cargo xtask bundle deepfilter-vst --release
```

This project is not very widely tested, but it should work. It does not yet have advanced configuration available. The only controls available are the attenuation limit, which determines how much the model is allowed to attenuate the incoming signal. This limit should not be set too low or the plugin will not do anything.

This plugin should be a decent alternative to RNNoise, which doesn't exactly have the best results in my own experience. The model used however is much more demanding.  
In my personal testing a properly compiled RNNoise plugin was about 3 times as performant as this plugin. I would recommend testing both options. If you notice that this plugin has better results, it should still be real time capable on modern CPUs.

NIH-Plug also supports CLAP plugins, but I have zero experience using CLAP, and it is mostly untested. Regardless, it should be automatically built alongside the VST plugin.
