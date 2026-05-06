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
mkdir -p output/lua_packages
mkdir -p output/resources
mkdir -p output/licenses
mkdir -p output/databases/sqlite
mkdir -p output/databases/lancedb
mkdir -p output/state/skills
mkdir -p output/temp
mkdir -p output/logs

reset_directory_contents() {
    # Ensure one directory exists and remove stale contents before a structured sync.
    # 确保目录存在，并在结构化同步前清理旧内容。
    local target_dir="$1"
    mkdir -p "$target_dir"
    find "$target_dir" -mindepth 1 -maxdepth 1 -exec rm -rf {} +
}

copy_directory_contents() {
    # Copy direct directory contents into a destination while preserving package layout.
    # 将目录直属内容复制到目标目录，并保持包布局。
    local source_dir="$1"
    local destination_dir="$2"
    [ -d "$source_dir" ] || return 1
    reset_directory_contents "$destination_dir"
    cp -a "$source_dir"/. "$destination_dir"/
}

enable_output_model_config_for_local_testing() {
    # Enable model capabilities only in the built output runtime config for local smoke testing.
    # 仅在构建后的输出运行配置中启用模型能力，方便本地冒烟测试。
    local config_dir="$1"
    local model_config_out="$config_dir/model_config.yaml"
    [ -f "$model_config_out" ] || return 0
    sed -i.bak -E \
        -e 's/^  enabled:[[:space:]]*false[[:space:]]*$/  enabled: true/' \
        -e 's/^    enabled:[[:space:]]*false[[:space:]]*$/    enabled: true/' \
        "$model_config_out"
    rm -f "$model_config_out.bak"
    echo "==> Output model_config.yaml enabled for local model smoke tests"
}

# Sync official LuaSkills runtime package exports to output/.
# Build packaging treats third_party/luaskills_runtime as a caller-managed asset root and copies it as-is.
# 构建打包会把 third_party/luaskills_runtime 视为调用方自管的资产根目录，并按现状直接同步。
# Cross-platform validation is intentionally omitted because forks may replace lua_packages/runtime payloads with custom layouts.
# 这里有意不做跨平台校验，因为 fork 方可能会用自定义布局替换 lua_packages/runtime 载荷。
mkdir -p output/libs
LUASKILLS_RUNTIME_ROOT="third_party/luaskills_runtime"
if [ -d "$LUASKILLS_RUNTIME_ROOT" ]; then
    if copy_directory_contents "$LUASKILLS_RUNTIME_ROOT/lua_packages" "output/lua_packages"; then
        echo "==> LuaSkills runtime lua_packages synced to output/lua_packages/"
    else
        echo "==> LuaSkills runtime package has no lua_packages directory"
    fi
    if copy_directory_contents "$LUASKILLS_RUNTIME_ROOT/libs" "output/libs"; then
        echo "==> LuaSkills runtime libs synced to output/libs/"
    else
        echo "==> LuaSkills runtime package has no libs directory"
    fi
    if copy_directory_contents "$LUASKILLS_RUNTIME_ROOT/resources" "output/resources"; then
        echo "==> LuaSkills runtime resources synced to output/resources/"
    else
        echo "==> LuaSkills runtime package has no resources directory"
    fi
    if copy_directory_contents "$LUASKILLS_RUNTIME_ROOT/licenses" "output/licenses"; then
        echo "==> LuaSkills runtime licenses synced to output/licenses/"
    else
        echo "==> LuaSkills runtime package has no licenses directory"
    fi
else
    echo "==> No third_party/luaskills_runtime found (run make deps first)"
fi

# Sync runtime config files to output/configs/
mkdir -p output/configs
if [ -d "runtime/configs" ] && [ "$(ls -A runtime/configs/ 2>/dev/null)" ]; then
    cp -rf runtime/configs/* output/configs/
    enable_output_model_config_for_local_testing "output/configs"
    echo "==> Runtime configs synced to output/configs/"
else
    echo "==> No runtime/configs directory found"
fi

# Sync runtime shared resources to output/resources/
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
    reset_directory_contents "$SKILLS_OUT"
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
