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

# Sync config files to output/configs
mkdir -p output/configs
if [ -d "configs" ] && [ "$(ls -A configs/ 2>/dev/null)" ]; then
    cp -rf configs/* output/configs/
    echo "==> Config files synced to output/configs/"
else
    echo "==> No config files to sync"
fi

# Sync Lua skills to output/lua_skills
SKILLS_OUT="output/lua_skills"
mkdir -p "$SKILLS_OUT"
if [ -d "lua_skills" ] && [ "$(ls -A lua_skills/ 2>/dev/null)" ]; then
    cp -rf lua_skills/* "$SKILLS_OUT/"
    echo "==> Lua skills synced to $SKILLS_OUT/"
else
    echo "==> No lua_skills directory found"
fi

# Sync third-party Lua packages to output/lua_packages
PKG_SRC="third_party/lua_packages"
PKG_OUT="output/lua_packages"
mkdir -p "$PKG_OUT"
if [ -d "$PKG_SRC" ] && [ "$(ls -A "$PKG_SRC/" 2>/dev/null)" ]; then
    cp -rf "$PKG_SRC"/* "$PKG_OUT/"
    echo "==> Third-party Lua packages synced to $PKG_OUT/"
else
    echo "==> No third_party/lua_packages found (run scripts/install_lua_deps.sh first)"
fi

echo "==> Done. Binary: ${OUT_DIR}/"
