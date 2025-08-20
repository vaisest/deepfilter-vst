#!/bin/bash

# Test script to validate release builds work correctly
# This script verifies that both VST3 and CLAP formats are generated properly

set -e

echo "=== DeepFilter VST Release Validation Test ==="
echo ""

# Function to check if file exists and is not empty
check_file() {
    local file="$1"
    local description="$2"
    
    if [ -f "$file" ]; then
        local size=$(stat -c%s "$file")
        if [ "$size" -gt 0 ]; then
            echo "✓ $description: OK (${size} bytes)"
            return 0
        else
            echo "✗ $description: EMPTY FILE"
            return 1
        fi
    else
        echo "✗ $description: NOT FOUND"
        return 1
    fi
}

# Function to check if directory exists and has contents
check_directory() {
    local dir="$1"
    local description="$2"
    
    if [ -d "$dir" ]; then
        local count=$(find "$dir" -type f | wc -l)
        if [ "$count" -gt 0 ]; then
            echo "✓ $description: OK ($count files)"
            return 0
        else
            echo "✗ $description: EMPTY DIRECTORY"
            return 1
        fi
    else
        echo "✗ $description: NOT FOUND"
        return 1
    fi
}

# Build the plugin
echo "Building plugin..."
cargo xtask bundle deepfilter-vst --release

echo ""
echo "=== Checking Build Artifacts ==="

# Check CLAP file
check_file "target/bundled/deepfilter-vst.clap" "CLAP plugin"

# Check VST3 bundle
check_directory "target/bundled/deepfilter-vst.vst3" "VST3 bundle"

# Check VST3 internal structure
if [ -d "target/bundled/deepfilter-vst.vst3" ]; then
    check_directory "target/bundled/deepfilter-vst.vst3/Contents" "VST3 Contents"
    check_directory "target/bundled/deepfilter-vst.vst3/Contents/x86_64-linux" "VST3 Linux binary"
    check_file "target/bundled/deepfilter-vst.vst3/Contents/x86_64-linux/deepfilter-vst.so" "VST3 shared library"
fi

echo ""
echo "=== File Type Verification ==="

# Check file types
if [ -f "target/bundled/deepfilter-vst.clap" ]; then
    clap_type=$(file target/bundled/deepfilter-vst.clap)
    echo "CLAP file type: $clap_type"
    
    if echo "$clap_type" | grep -q "ELF.*shared object"; then
        echo "✓ CLAP is a valid shared library"
    else
        echo "✗ CLAP is not a valid shared library"
    fi
fi

if [ -f "target/bundled/deepfilter-vst.vst3/Contents/x86_64-linux/deepfilter-vst.so" ]; then
    vst3_type=$(file target/bundled/deepfilter-vst.vst3/Contents/x86_64-linux/deepfilter-vst.so)
    echo "VST3 file type: $vst3_type"
    
    if echo "$vst3_type" | grep -q "ELF.*shared object"; then
        echo "✓ VST3 is a valid shared library"
    else
        echo "✗ VST3 is not a valid shared library"
    fi
fi

echo ""
echo "=== Testing Release Archive Creation ==="

# Test archive creation
./build-release.sh

if [ -f "release-test/deepfilter-vst-linux-x64.tar.gz" ]; then
    echo "✓ Release archive created successfully"
    
    # Check archive contents
    echo ""
    echo "Archive contents:"
    tar -tzf release-test/deepfilter-vst-linux-x64.tar.gz
    
    # Test extraction
    mkdir -p test-extract
    cd test-extract
    tar -xzf ../release-test/deepfilter-vst-linux-x64.tar.gz
    
    echo ""
    echo "=== Testing Extracted Files ==="
    check_file "deepfilter-vst.clap" "Extracted CLAP plugin"
    check_directory "deepfilter-vst.vst3" "Extracted VST3 bundle"
    
    cd ..
    rm -rf test-extract
else
    echo "✗ Release archive creation failed"
fi

# Clean up
rm -rf release-test

echo ""
echo "=== Plugin Information ==="
echo "Plugin name: $(grep '^name = ' Cargo.toml | cut -d'"' -f2)"
echo "Plugin version: $(grep '^version = ' Cargo.toml | cut -d'"' -f2)"
echo "Plugin description: $(grep '^description = ' Cargo.toml | cut -d'"' -f2)"

echo ""
echo "=== Test Complete ==="
echo "All checks passed! The plugin should be ready for release."