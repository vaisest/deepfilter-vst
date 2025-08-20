# Release Guide

This document explains how to create releases for the DeepFilter VST plugin.

## Release Workflows

The repository includes two GitHub Actions workflows for creating releases:

### 1. Automatic Release (`release.yml`)

**Trigger**: Push a version tag to the repository

**Usage**:
```bash
# Create and push a new version tag
git tag v0.1.0
git push origin v0.1.0
```

**What it does**:
- Builds the plugin for Linux (x64) and Windows (x64)
- Creates compressed archives with VST3 and CLAP files
- Creates a draft release with installation instructions
- Uploads build artifacts to the release

### 2. Manual Release (`manual-release.yml`)

**Trigger**: Manual execution from GitHub Actions web interface

**Usage**:
1. Go to `Actions` tab in GitHub repository
2. Select "Manual Release" workflow
3. Click "Run workflow"
4. Enter version (e.g., `v0.1.0`) and select platforms
5. Click "Run workflow" button

**Options**:
- `version`: Version tag for the release (required)
- `include_windows`: Whether to build Windows version (default: true)

## Local Testing

Before creating releases, test the build process locally:

```bash
# Test Linux build only
./build-release.sh

# Test both Linux and Windows builds
./build-release.sh --windows
```

This creates test archives in the `release-test/` directory.

## Release Process

### For Maintainers

1. **Prepare the release**:
   - Update version in `Cargo.toml` if needed
   - Update `CHANGELOG.md` (if it exists)
   - Test builds locally with `./build-release.sh --windows`
   - Commit any changes

2. **Create the release**:

   **Option A: Automatic (recommended)**
   ```bash
   # Create and push version tag
   git tag v0.1.0
   git push origin v0.1.0
   ```

   **Option B: Manual**
   - Go to Actions → Manual Release
   - Enter version and run workflow

3. **Publish the release**:
   - Go to GitHub Releases page
   - Find the draft release created by the workflow
   - Review the release notes and artifacts
   - Edit if needed, then publish

### Release Artifacts

Each release includes:

- `deepfilter-vst-linux-x64.tar.gz`: Linux version with VST3 and CLAP
- `deepfilter-vst-windows-x64.tar.gz`: Windows version with VST3 and CLAP

### Archive Contents

Each archive contains:
- `deepfilter-vst.clap`: CLAP plugin file
- `deepfilter-vst.vst3/`: VST3 plugin bundle directory

## Installation Instructions (for Users)

### Linux
```bash
# Extract archive
tar -xzf deepfilter-vst-linux-x64.tar.gz

# Install VST3 (create directory if it doesn't exist)
mkdir -p ~/.vst3
mv deepfilter-vst.vst3 ~/.vst3/

# Install CLAP (create directory if it doesn't exist)  
mkdir -p ~/.clap
mv deepfilter-vst.clap ~/.clap/
```

### Windows
1. Extract the archive
2. Copy `deepfilter-vst.vst3` folder to `C:\Program Files\Common Files\VST3\`
3. Copy `deepfilter-vst.clap` file to your DAW's CLAP directory

## Troubleshooting

### Build Issues

**Linux dependencies missing**:
```bash
sudo apt install build-essential pkg-config libasound2-dev libjack-jackd2-dev \
  libx11-dev libgl1-mesa-dev libxrandr-dev libxcursor-dev libxinerama-dev \
  libxi-dev libglu1-mesa-dev libx11-xcb-dev
```

**Windows cross-compilation setup**:
```bash
sudo apt install mingw-w64
rustup target add x86_64-pc-windows-gnu
```

### Workflow Issues

- **Workflow not visible**: Make sure workflows are on the main/master branch
- **Build failure**: Check the Actions logs for specific error messages
- **Missing artifacts**: Ensure all build jobs completed successfully

## Version Management

- Use semantic versioning: `vMAJOR.MINOR.PATCH`
- Examples: `v0.1.0`, `v0.2.1`, `v1.0.0`
- Pre-releases: `v0.1.0-beta.1`, `v0.1.0-rc.1`