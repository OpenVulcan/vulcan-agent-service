#!/usr/bin/env bash
# Build script for vulcan-agent-service
# Usage:
#   ./build.sh          # debug build
#   ./build.sh release  # release build

set -e

PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_DIR"

BIN_NAME="vulcan-agent-service"
BUILD_MODE="${1:-debug}"

if [ "$BUILD_MODE" = "release" ]; then
    OUT_DIR="output/bin"
    # CARGO_FLAGS selects the Cargo release profile for the requested build mode.
    # CARGO_FLAGS 用于根据构建模式选择 Cargo release 配置。
    CARGO_FLAGS=(--release)
    echo "==> Building release..."
else
    OUT_DIR="output/debug"
    # CARGO_FLAGS stays empty so Cargo uses its debug profile.
    # CARGO_FLAGS 保持为空，使 Cargo 使用 debug 配置。
    CARGO_FLAGS=()
    echo "==> Building debug..."
fi

# Build
if ! cargo build "${CARGO_FLAGS[@]}" 2>&1; then
    echo "Cargo build failed; existing binaries were not copied." >&2
    exit 1
fi

# Copy binary
# CARGO_PROFILE names the Cargo output profile selected above.
# CARGO_PROFILE 表示上方选择的 Cargo 输出配置。
CARGO_PROFILE="debug"
if [ "$BUILD_MODE" = "release" ]; then
    CARGO_PROFILE="release"
fi
# CARGO_TARGET identifies the executable produced by the successful Cargo build.
# CARGO_TARGET 表示成功 Cargo 构建生成的可执行文件。
CARGO_TARGET="target/${CARGO_PROFILE}/${BIN_NAME}"
if [ -f "$CARGO_TARGET.exe" ]; then
    CARGO_TARGET="${CARGO_TARGET}.exe"
fi
if [ ! -f "$CARGO_TARGET" ]; then
    echo "Build succeeded but binary was not found at ${CARGO_TARGET}" >&2
    exit 1
fi

mkdir -p "$OUT_DIR"
cp -f "$CARGO_TARGET" "$OUT_DIR/"
echo "==> Binary copied to ${OUT_DIR}/"

LUA_RUNTIME_OUT="output/lua_runtime"
MANAGED_RUNTIME_DISTRIBUTION_ROOT="third_party/luaskills_managed_runtimes"
MANAGED_RUNTIME_LAYOUT_CHECK_SCRIPT="scripts/debug-tools/managed_runtime_layout_check.py"

mkdir -p "$LUA_RUNTIME_OUT/resources" "$LUA_RUNTIME_OUT/bin" output/configs output/logs

copy_directory_contents() {
    # Copy direct directory contents without deleting existing destination data.
    # 复制目录直属内容，并且不删除目标目录中已有的数据。
    local source_dir="$1"
    local destination_dir="$2"
    [ -d "$source_dir" ] || return 1
    mkdir -p "$destination_dir"
    cp -a "$source_dir"/. "$destination_dir"/
}

# Copy official LuaSkills runtime package exports into output/lua_runtime/.
# 将官方 LuaSkills 运行时包导出内容复制到 output/lua_runtime/。
# Build packaging treats third_party/luaskills_runtime as a caller-managed asset root and copies it as-is.
# 构建打包会把 third_party/luaskills_runtime 视为调用方自管的资产根目录，并按现状直接同步。
# Existing output files are preserved so a build cannot erase user-managed runtime data.
# 保留已有 output 文件，确保构建不会擦除用户管理的运行时数据。
LUASKILLS_RUNTIME_ROOT="third_party/luaskills_runtime"
if [ -d "$LUASKILLS_RUNTIME_ROOT" ]; then
    if copy_directory_contents "$LUASKILLS_RUNTIME_ROOT/lua_packages" "$LUA_RUNTIME_OUT/lua_packages"; then
        echo "==> LuaSkills runtime lua_packages copied to $LUA_RUNTIME_OUT/lua_packages/"
    else
        echo "==> LuaSkills runtime package has no lua_packages directory"
    fi
    if copy_directory_contents "$LUASKILLS_RUNTIME_ROOT/libs" "$LUA_RUNTIME_OUT/libs"; then
        echo "==> LuaSkills runtime libs copied to $LUA_RUNTIME_OUT/libs/"
    else
        echo "==> LuaSkills runtime package has no libs directory"
    fi
    if copy_directory_contents "$LUASKILLS_RUNTIME_ROOT/resources" "$LUA_RUNTIME_OUT/resources"; then
        echo "==> LuaSkills runtime resources copied to $LUA_RUNTIME_OUT/resources/"
    else
        echo "==> LuaSkills runtime package has no resources directory"
    fi
    if copy_directory_contents "$LUASKILLS_RUNTIME_ROOT/licenses" "$LUA_RUNTIME_OUT/licenses"; then
        echo "==> LuaSkills runtime licenses copied to $LUA_RUNTIME_OUT/licenses/"
    else
        echo "==> LuaSkills runtime package has no licenses directory"
    fi
else
    echo "==> No third_party/luaskills_runtime found (run make deps first)"
fi

# Copy fetched managed distributions into the fixed read-only runtime root.
# 将已拉取的受管发行包复制到固定只读运行时根。
if copy_directory_contents "$MANAGED_RUNTIME_DISTRIBUTION_ROOT" "$LUA_RUNTIME_OUT/dependencies/runtimes"; then
    echo "==> Managed Python/Node distributions copied to $LUA_RUNTIME_OUT/dependencies/runtimes/"
    if command -v python3 >/dev/null 2>&1; then
        python3 "$MANAGED_RUNTIME_LAYOUT_CHECK_SCRIPT" "$LUA_RUNTIME_OUT"
    elif [ "$BUILD_MODE" = "release" ]; then
        echo "python3 is required to validate managed runtimes during release packaging" >&2
        exit 1
    else
        echo "WARNING: python3 is unavailable; debug packaging skipped the managed runtime layout validator" >&2
    fi
else
    if [ "$BUILD_MODE" = "release" ]; then
        echo "Managed Python/Node distributions are required for release packaging; run make deps managed first" >&2
        exit 1
    fi
    echo "==> No managed Python/Node distributions found (run make deps managed first)"
fi

# Copy repository config templates to output/configs without overwriting installed configuration.
# 将仓库配置模板复制到 output/configs，且不覆盖已安装配置。
if [ -d "configs" ]; then
    # config_source iterates repository templates without deleting output files.
    # config_source 遍历仓库模板，且不删除 output 文件。
    for config_source in configs/*; do
        [ -f "$config_source" ] || continue
        # config_name and config_destination identify the protected output target.
        # config_name 与 config_destination 用于定位受保护的输出目标。
        config_name="$(basename "$config_source")"
        config_destination="output/configs/$config_name"
        if [ ! -e "$config_destination" ]; then
            cp -f "$config_source" "$config_destination"
        fi
    done
    echo "==> Config templates copied to output/configs/ (existing files preserved)"
else
    echo "==> No configs directory found"
fi

# Copy repository shared resources to output/lua_runtime/resources/.
# 将仓库共享资源复制到 output/lua_runtime/resources/。
if [ -d "resources" ]; then
    cp -a resources/. "$LUA_RUNTIME_OUT/resources/"
    echo "==> Shared resources copied to $LUA_RUNTIME_OUT/resources/"
else
    echo "==> No resources directory found"
fi

# Prepare output/lua_runtime/bin as the fixed host-provided tool directory.
HOST_TOOLS_OUT="$LUA_RUNTIME_OUT/bin"
mkdir -p "$HOST_TOOLS_OUT"
echo "==> Host tool output directory prepared at $HOST_TOOLS_OUT/"

# Copy the host-installed vldb-controller executable to output/lua_runtime/bin/ when prepared.
CONTROLLER_OUT="$LUA_RUNTIME_OUT/bin"
mkdir -p "$CONTROLLER_OUT"
CONTROLLER_BINARY_SOURCE="third_party/vldb_controller/bin/vldb-controller"
if [ -e "$CONTROLLER_BINARY_SOURCE" ]; then
    if [ -f "$CONTROLLER_BINARY_SOURCE" ]; then
        if cp -f "$CONTROLLER_BINARY_SOURCE" "$CONTROLLER_OUT/" 2>/dev/null; then
            echo "==> vldb-controller synced to $CONTROLLER_OUT/"
        else
            echo "==> vldb-controller is currently running or locked; keeping the existing output binary and continuing"
        fi
    else
        echo "ERROR: vldb-controller source path is not a file: $CONTROLLER_BINARY_SOURCE" >&2
        exit 1
    fi
else
    echo "==> No third_party/vldb_controller/bin/vldb-controller found"
fi

echo "==> Done. Binary: ${OUT_DIR}/"
