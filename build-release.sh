#!/bin/bash

# Local build script for testing releases
# This script builds the plugin for Linux and optionally Windows

set -e

echo "Building DeepFilter VST Plugin..."

# Clean previous builds
echo "Cleaning previous builds..."
rm -rf target/bundled
rm -rf release-test/

# Create release test directory
mkdir -p release-test

# Build for Linux
echo "Building for Linux..."
cargo xtask bundle deepfilter-vst --release

# Copy Linux artifacts
echo "Copying Linux artifacts..."
mkdir -p release-test/linux
cp target/bundled/deepfilter-vst.clap release-test/linux/
cp -r target/bundled/deepfilter-vst.vst3 release-test/linux/

# Create Linux archive
cd release-test
tar -czf deepfilter-vst-linux-x64.tar.gz -C linux .
cd ..

echo "Linux build complete: release-test/deepfilter-vst-linux-x64.tar.gz"

# Build for Windows if requested
if [ "$1" = "--windows" ] || [ "$1" = "-w" ]; then
    echo "Building for Windows..."
    
    # Check if Windows target is installed
    if ! rustup target list --installed | grep -q x86_64-pc-windows-gnu; then
        echo "Installing Windows target..."
        rustup target add x86_64-pc-windows-gnu
    fi
    
    # Check if MinGW is available
    if ! command -v x86_64-w64-mingw32-gcc &> /dev/null; then
        echo "Error: MinGW not found. Install with: sudo apt install mingw-w64"
        exit 1
    fi
    
    cargo xtask bundle deepfilter-vst --release --target x86_64-pc-windows-gnu
    
    # Copy Windows artifacts
    echo "Copying Windows artifacts..."
    mkdir -p release-test/windows
    cp target/x86_64-pc-windows-gnu/bundled/deepfilter-vst.clap release-test/windows/
    cp -r target/x86_64-pc-windows-gnu/bundled/deepfilter-vst.vst3 release-test/windows/
    
    # Create Windows archive
    cd release-test
    tar -czf deepfilter-vst-windows-x64.tar.gz -C windows .
    cd ..
    
    echo "Windows build complete: release-test/deepfilter-vst-windows-x64.tar.gz"
fi

echo ""
echo "Build Summary:"
echo "=============="
ls -lh release-test/*.tar.gz

echo ""
echo "Archive Contents:"
echo "================"
for archive in release-test/*.tar.gz; do
    echo "--- $(basename "$archive") ---"
    tar -tzf "$archive"
    echo ""
done

echo "Build complete! Archives are in the release-test/ directory."
echo ""
echo "To test installation:"
echo "  Linux VST3: tar -xzf release-test/deepfilter-vst-linux-x64.tar.gz && mv deepfilter-vst.vst3 ~/.vst3/"
echo "  Linux CLAP: tar -xzf release-test/deepfilter-vst-linux-x64.tar.gz && mv deepfilter-vst.clap ~/.clap/"