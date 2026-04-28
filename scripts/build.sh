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

mkdir -p "$OUT_DIR"
cp -f "$CARGO_TARGET" "$OUT_DIR/"
echo "==> Binary copied to ${OUT_DIR}/"

mkdir -p output/bin/tools
mkdir -p output/dependencies/shared/lua
mkdir -p output/dependencies/shared/ffi
mkdir -p output/dependencies/skill
mkdir -p output/databases/sqlite
mkdir -p output/databases/lancedb
mkdir -p output/state/skills
mkdir -p output/temp
mkdir -p output/logs

# Sync C dependency DLLs to output/libs/
mkdir -p output/libs
if [ -d "third_party/deps" ]; then
    find third_party/deps -type f \( -name "*.dll" -o -name "*.so" -o -name "*.dylib" \) -exec cp -f {} output/libs/ \;
    echo "==> C runtime libs synced to output/libs/"
else
    echo "==> No third_party/deps found"
fi

# Copy LuaJIT runtime DLL (lua51.dll) — required by luarocks-built C modules like lfs.dll
if [ -f "third_party/luajit/lua51.dll" ]; then
    cp -f third_party/luajit/lua51.dll output/libs/
    echo "==> LuaJIT lua51.dll synced to output/libs/"
fi

# Sync runtime config files to output/configs/
mkdir -p output/configs
if [ -d "runtime/configs" ] && [ "$(ls -A runtime/configs/ 2>/dev/null)" ]; then
    cp -rf runtime/configs/* output/configs/
    echo "==> Runtime configs synced to output/configs/"
else
    echo "==> No runtime/configs directory found"
fi

# Sync runtime shared resources to output/resources/
mkdir -p output/resources
if [ -d "runtime/resources" ] && [ "$(ls -A runtime/resources/ 2>/dev/null)" ]; then
    cp -rf runtime/resources/* output/resources/
    echo "==> Runtime shared resources synced to output/resources/"
else
    echo "==> No runtime/resources directory found"
fi

# Sync runtime state records to output/state/
# 同步运行时状态记录到 output/state/
mkdir -p output/state
if [ -d "runtime/state" ] && [ "$(ls -A runtime/state/ 2>/dev/null)" ]; then
    cp -rf runtime/state/* output/state/
    echo "==> Runtime state synced to output/state/"
else
    echo "==> No runtime/state directory found"
fi

# Sync runtime Lua skills to output/skills/
SKILLS_OUT="output/skills"
mkdir -p "$SKILLS_OUT"
if [ -d "runtime/skills" ] && [ "$(ls -A runtime/skills/ 2>/dev/null)" ]; then
    cp -rf runtime/skills/* "$SKILLS_OUT/"
    echo "==> Runtime Lua skills synced to $SKILLS_OUT/"
else
    echo "==> No runtime/skills directory found"
fi

# Prepare output/bin/tools/ as the host runtime tool directory.
HOST_TOOLS_OUT="output/bin/tools"
mkdir -p "$HOST_TOOLS_OUT"
echo "==> Host tool output directory prepared at $HOST_TOOLS_OUT/"

# Copy the host-installed vldb-controller executable to output/bin/ when the dependency bootstrap has prepared it.
CONTROLLER_OUT="output/bin"
mkdir -p "$CONTROLLER_OUT"
CONTROLLER_BINARY_SOURCE="third_party/vldb_controller/bin/vldb-controller"
if [ -e "$CONTROLLER_BINARY_SOURCE" ]; then
    if [ -f "$CONTROLLER_BINARY_SOURCE" ]; then
        cp -f "$CONTROLLER_BINARY_SOURCE" "$CONTROLLER_OUT/"
        echo "==> vldb-controller synced to $CONTROLLER_OUT/"
    else
        echo "ERROR: vldb-controller source path is not a file: $CONTROLLER_BINARY_SOURCE" >&2
        exit 1
    fi
else
    echo "==> No third_party/vldb_controller/bin/vldb-controller found"
fi

# Sync third-party Lua packages to output/lua_packages/
# Only copy runtime-relevant directories: lib/lua/, share/lua/
PKG_SRC="third_party/lua_packages"
PKG_OUT="output/lua_packages"
if [ -d "$PKG_SRC" ]; then
    for dir in lib/lua share/lua; do
        if [ -d "$PKG_SRC/$dir" ]; then
            mkdir -p "$PKG_OUT/$dir"
            copy_root="$PKG_SRC/$dir"
            if [ -d "$PKG_SRC/$dir/5.1" ]; then
                copy_root="$PKG_SRC/$dir/5.1"
            fi
            for entry in "$copy_root"/*; do
                [ -e "$entry" ] || continue
                if [ "$(basename "$entry")" = "5.1" ]; then
                    continue
                fi
                cp -rf "$entry" "$PKG_OUT/$dir/"
            done
        fi
    done
    echo "==> Third-party Lua packages synced to $PKG_OUT/ (flattening 5.1 package layout into lua/)"
else
    echo "==> No third_party/lua_packages found (run scripts/install_lua_deps.sh first)"
fi

echo "==> Done. Binary: ${OUT_DIR}/"
