# DeepFilter VST Plugin

A VST3/CLAP audio plugin that uses DeepFilter neural networks for real-time noise reduction. This plugin processes audio in a separate worker thread to avoid blocking the audio processing thread.

## Downloads

### Pre-built Releases

Download the latest release from the [Releases](https://github.com/edsonsantoro/deepfilter-vst/releases) page.

Available formats:
- **Linux (x64)**: VST3 and CLAP formats for Linux systems
- **Windows (x64)**: VST3 and CLAP formats for Windows systems

### Installation

#### Linux
1. Extract the downloaded archive
2. For VST3: Copy `deepfilter-vst.vst3` folder to `~/.vst3/`
3. For CLAP: Copy `deepfilter-vst.clap` file to `~/.clap/`

#### Windows
1. Extract the downloaded archive  
2. For VST3: Copy `deepfilter-vst.vst3` folder to `C:\Program Files\Common Files\VST3\`
3. For CLAP: Copy `deepfilter-vst.clap` file to your DAW's CLAP plugin directory

## Dependencies

### Linux System Dependencies

Before building, install the required system dependencies:

#### Ubuntu/Debian:
```shell
sudo apt update
sudo apt install -y build-essential pkg-config libasound2-dev libjack-jackd2-dev \
    libx11-dev libgl1-mesa-dev libxrandr-dev libxcursor-dev libxinerama-dev \
    libxi-dev libglu1-mesa-dev libx11-xcb-dev
```

#### Fedora/CentOS/RHEL:
```shell
sudo dnf install -y gcc gcc-c++ pkgconfig alsa-lib-devel jack-audio-connection-kit-devel \
    libX11-devel mesa-libGL-devel libXrandr-devel libXcursor-devel libXinerama-devel \
    libXi-devel mesa-libGLU-devel libX11-devel
```

#### Arch Linux:
```shell
sudo pacman -S base-devel pkg-config alsa-lib jack2 libx11 mesa libxrandr \
    libxcursor libxinerama libxi glu
```

### Cross-compilation for Windows (on Linux)

To build Windows binaries from Linux, install the MinGW-w64 toolchain:

#### Ubuntu/Debian:
```shell
sudo apt install -y mingw-w64
```

Then add the Windows target to Rust:
```shell
rustup target add x86_64-pc-windows-gnu
```

## Building

### Prerequisites
1. Install [Rust](https://rustup.rs/)
2. Install system dependencies (see above)

### Building for Linux
```shell
cargo xtask bundle deepfilter-vst --release
```

### Cross-compiling for Windows (on Linux)
```shell
cargo xtask bundle deepfilter-vst --release --target x86_64-pc-windows-gnu
```

The built plugins will be available in `target/bundled/`:
- `deepfilter-vst.clap` - CLAP plugin format
- `deepfilter-vst.vst3` - VST3 plugin format

### Building Release Packages

For local testing of release packages, use the provided build script:

```shell
# Build Linux version only
./build-release.sh

# Build both Linux and Windows versions
./build-release.sh --windows
```

This will create release packages in the `release-test/` directory.

## Automated Releases

This repository includes GitHub Actions workflows for automated releases:

- **Automatic releases**: Triggered when a version tag is pushed (e.g., `git tag v0.1.0 && git push origin v0.1.0`)
- **Manual releases**: Use the "Manual Release" workflow in GitHub Actions to create releases on-demand

Both workflows build the plugin for Linux and Windows, create release archives, and publish a draft release with the compiled files ready for download.

## Plugin Parameters

The DeepFilter VST plugin provides 5 configurable parameters for fine-tuning noise reduction:

### 1. Attenuation Limit
- **Range:** 0.1 dB to 100.0 dB
- **Default:** 70.0 dB
- **Effect:** Controls the maximum attenuation applied to noisy frequency bins. Higher values allow more aggressive noise reduction but may affect speech quality. Use lower values for subtle noise reduction that preserves speech naturalness.

### 2. Min Threshold  
- **Range:** -30.0 dB to 0.0 dB
- **Default:** -15.0 dB
- **Effect:** Controls the minimum threshold for noise detection. Lower values make the filter more sensitive to noise but may affect quiet speech or introduce artifacts in silent passages.

### 3. Max ERB Threshold
- **Range:** 10.0 dB to 50.0 dB  
- **Default:** 35.0 dB
- **Effect:** Controls the maximum ERB (Equivalent Rectangular Bandwidth) threshold. This affects frequency-domain processing sensitivity and determines how the filter analyzes different frequency bands.

### 4. Max Threshold
- **Range:** 10.0 dB to 50.0 dB
- **Default:** 35.0 dB  
- **Effect:** Controls the maximum threshold for DeepFilter processing. Higher values allow more aggressive processing and stronger noise reduction.

### 5. Post Filter Beta
- **Range:** 0.0 to 2.0
- **Default:** 1.0
- **Effect:** Controls the post-filter beta coefficient. This affects the strength of post-processing filtering applied after the main DeepFilter neural network processing. Values above 1.0 increase post-filtering strength, while values below 1.0 reduce it.

## Usage Tips

- Start with default parameters and adjust incrementally
- For speech-heavy content, use lower Attenuation Limit values (30-50 dB)
- For music or complex audio, experiment with Min Threshold and ERB settings
- The Post Filter Beta can help clean up remaining artifacts after neural processing
- Monitor for over-processing artifacts when using aggressive settings

## Status

This project is a work in progress. The plugin is functional but may require further testing and optimization for production use.
