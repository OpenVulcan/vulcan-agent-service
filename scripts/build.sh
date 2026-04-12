#!/usr/bin/env bash
# Build script for vulcan-mcp
# Usage:
#   ./build.sh          # debug build
#   ./build.sh release  # release build

set -e

PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_DIR"

BIN_NAME="vulcan-mcp"
BUILD_MODE="${1:-debug}"

if [ "$BUILD_MODE" = "release" ]; then
    OUT_DIR="output/bin"
    CARGO_FLAGS="--release"
    echo "==> Building release..."
else
    OUT_DIR="output/debug"
    CARGO_FLAGS=""
    echo "==> Building debug..."
fi

# Build
cargo build $CARGO_FLAGS 2>&1

# Copy binary
CARGO_TARGET="target/$([ "$BUILD_MODE" = "release" ] && echo "release" || echo "debug")/${BIN_NAME}"
if [ -f "$CARGO_TARGET.exe" ]; then
    CARGO_TARGET="${CARGO_TARGET}.exe"
fi

cp -f "$CARGO_TARGET" "$OUT_DIR/"
echo "==> Binary copied to ${OUT_DIR}/"

# Sync config files
mkdir -p output/configs
if [ -d "configs" ] && [ "$(ls -A configs/ 2>/dev/null)" ]; then
    cp -rf configs/* output/configs/
    echo "==> Config files synced to output/configs/"
else
    echo "==> No config files to sync"
fi

echo "==> Done. Binary: ${OUT_DIR}/"
