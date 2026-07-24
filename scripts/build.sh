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

LUA_RUNTIME_OUT="output/lua_runtime"
SOURCE_LUA_RUNTIME_ROOT="runtime/lua_runtime"
MANAGED_RUNTIME_DISTRIBUTION_ROOT="third_party/luaskills_managed_runtimes"
MANAGED_RUNTIME_LAYOUT_CHECK_SCRIPT="scripts/debug-tools/managed_runtime_layout_check.py"

# remove_legacy_output_layout deletes only obsolete Lua-owned children below the verified output root.
# remove_legacy_output_layout 仅删除已校验 output 根下废弃的 Lua 所有目录项。
remove_legacy_output_layout() {
    # output_root is canonical because the repository working directory already exists.
    # output_root 是规范路径，因为仓库工作目录已经存在。
    local output_root="$(pwd -P)/output"
    # legacy_names enumerates runtime directories that moved beneath output/lua_runtime.
    # legacy_names 枚举已经迁移到 output/lua_runtime 下的运行时目录。
    local legacy_names=(skills state dependencies databases temp libs lua_packages resources licenses system_lua_lib)
    local legacy_name=""
    local legacy_path=""
    mkdir -p "${output_root}"
    for legacy_name in "${legacy_names[@]}"; do
        legacy_path="${output_root}/${legacy_name}"
        case "${legacy_path}" in
            "${output_root}"/*) rm -rf -- "${legacy_path}" ;;
            *) echo "Legacy cleanup target escaped output: ${legacy_path}" >&2; exit 1 ;;
        esac
    done
    # legacy_bin_entries excludes the host executable and removes only old Lua-owned bin payloads.
    # legacy_bin_entries 排除宿主可执行文件，仅删除旧 Lua 所有的 bin 载荷。
    local legacy_bin_entries=(tools vldb-controller vldb-controller.exe)
    local legacy_bin_entry=""
    for legacy_bin_entry in "${legacy_bin_entries[@]}"; do
        legacy_path="${output_root}/bin/${legacy_bin_entry}"
        case "${legacy_path}" in
            "${output_root}"/*) rm -rf -- "${legacy_path}" ;;
            *) echo "Legacy bin cleanup target escaped output: ${legacy_path}" >&2; exit 1 ;;
        esac
    done
}

remove_legacy_output_layout

mkdir -p "$LUA_RUNTIME_OUT/bin"
mkdir -p "$LUA_RUNTIME_OUT/dependencies/runtimes"
mkdir -p "$LUA_RUNTIME_OUT/dependencies/envs"
mkdir -p "$LUA_RUNTIME_OUT/lua_packages"
mkdir -p "$LUA_RUNTIME_OUT/libs"
mkdir -p "$LUA_RUNTIME_OUT/resources"
mkdir -p "$LUA_RUNTIME_OUT/licenses"
mkdir -p "$LUA_RUNTIME_OUT/config"
mkdir -p "$LUA_RUNTIME_OUT/databases/sqlite"
mkdir -p "$LUA_RUNTIME_OUT/databases/lancedb"
mkdir -p "$LUA_RUNTIME_OUT/state/skills"
mkdir -p "$LUA_RUNTIME_OUT/temp"
mkdir -p "$LUA_RUNTIME_OUT/system_lua_lib"
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

# Sync official LuaSkills runtime package exports to output/lua_runtime/.
# 同步官方 LuaSkills 运行时包导出内容到 output/lua_runtime/。
# Build packaging treats third_party/luaskills_runtime as a caller-managed asset root and copies it as-is.
# 构建打包会把 third_party/luaskills_runtime 视为调用方自管的资产根目录，并按现状直接同步。
# Cross-platform validation is intentionally omitted because forks may replace lua_packages/runtime payloads with custom layouts.
# 这里有意不做跨平台校验，因为 fork 方可能会用自定义布局替换 lua_packages/runtime 载荷。
LUASKILLS_RUNTIME_ROOT="third_party/luaskills_runtime"
if [ -d "$LUASKILLS_RUNTIME_ROOT" ]; then
    if copy_directory_contents "$LUASKILLS_RUNTIME_ROOT/lua_packages" "$LUA_RUNTIME_OUT/lua_packages"; then
        echo "==> LuaSkills runtime lua_packages synced to $LUA_RUNTIME_OUT/lua_packages/"
    else
        echo "==> LuaSkills runtime package has no lua_packages directory"
    fi
    if copy_directory_contents "$LUASKILLS_RUNTIME_ROOT/libs" "$LUA_RUNTIME_OUT/libs"; then
        echo "==> LuaSkills runtime libs synced to $LUA_RUNTIME_OUT/libs/"
    else
        echo "==> LuaSkills runtime package has no libs directory"
    fi
    if copy_directory_contents "$LUASKILLS_RUNTIME_ROOT/resources" "$LUA_RUNTIME_OUT/resources"; then
        echo "==> LuaSkills runtime resources synced to $LUA_RUNTIME_OUT/resources/"
    else
        echo "==> LuaSkills runtime package has no resources directory"
    fi
    if copy_directory_contents "$LUASKILLS_RUNTIME_ROOT/licenses" "$LUA_RUNTIME_OUT/licenses"; then
        echo "==> LuaSkills runtime licenses synced to $LUA_RUNTIME_OUT/licenses/"
    else
        echo "==> LuaSkills runtime package has no licenses directory"
    fi
else
    echo "==> No third_party/luaskills_runtime found (run make deps first)"
fi

# Sync fetched managed distributions into the fixed read-only runtime root.
# 把已拉取的受管发行包同步到固定只读运行时根。
if copy_directory_contents "$MANAGED_RUNTIME_DISTRIBUTION_ROOT" "$LUA_RUNTIME_OUT/dependencies/runtimes"; then
    echo "==> Managed Python/Node distributions synced to $LUA_RUNTIME_OUT/dependencies/runtimes/"
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

# Sync runtime config files to output/configs/
mkdir -p output/configs
if [ -d "runtime/configs" ] && [ "$(ls -A runtime/configs/ 2>/dev/null)" ]; then
    cp -rf runtime/configs/* output/configs/
    enable_output_model_config_for_local_testing "output/configs"
    echo "==> Runtime configs synced to output/configs/"
else
    echo "==> No runtime/configs directory found"
fi

# Sync the source runtime skill config to the isolated package.
# 把源码运行时 Skill 配置同步到隔离包。
if [ -d "$SOURCE_LUA_RUNTIME_ROOT/config" ] && [ "$(ls -A "$SOURCE_LUA_RUNTIME_ROOT/config/" 2>/dev/null)" ]; then
    reset_directory_contents "$LUA_RUNTIME_OUT/config"
    cp -a "$SOURCE_LUA_RUNTIME_ROOT/config/." "$LUA_RUNTIME_OUT/config/"
    echo "==> LuaSkills config synced to $LUA_RUNTIME_OUT/config/"
fi

# Sync runtime shared resources to output/lua_runtime/resources/.
if [ -d "$SOURCE_LUA_RUNTIME_ROOT/resources" ] && [ "$(ls -A "$SOURCE_LUA_RUNTIME_ROOT/resources/" 2>/dev/null)" ]; then
    cp -a "$SOURCE_LUA_RUNTIME_ROOT/resources/." "$LUA_RUNTIME_OUT/resources/"
    echo "==> Runtime shared resources synced to $LUA_RUNTIME_OUT/resources/"
else
    echo "==> No runtime/lua_runtime/resources directory found"
fi

# Sync runtime state records to output/lua_runtime/state/.
# 同步运行时状态记录到 output/lua_runtime/state/。
if [ -d "$SOURCE_LUA_RUNTIME_ROOT/state" ] && [ "$(ls -A "$SOURCE_LUA_RUNTIME_ROOT/state/" 2>/dev/null)" ]; then
    cp -a "$SOURCE_LUA_RUNTIME_ROOT/state/." "$LUA_RUNTIME_OUT/state/"
    echo "==> Runtime state synced to $LUA_RUNTIME_OUT/state/"
else
    echo "==> No runtime/lua_runtime/state directory found"
fi

# Sync runtime Lua skills to output/lua_runtime/skills/.
SKILLS_OUT="$LUA_RUNTIME_OUT/skills"
mkdir -p "$SKILLS_OUT"
if [ -d "$SOURCE_LUA_RUNTIME_ROOT/skills" ] && [ "$(ls -A "$SOURCE_LUA_RUNTIME_ROOT/skills/" 2>/dev/null)" ]; then
    reset_directory_contents "$SKILLS_OUT"
    cp -a "$SOURCE_LUA_RUNTIME_ROOT/skills/." "$SKILLS_OUT/"
    echo "==> Runtime Lua skills synced to $SKILLS_OUT/"
else
    echo "==> No runtime/lua_runtime/skills directory found"
fi

# Synchronize source-controlled dependency families without touching writable envs or fetched runtimes.
# 同步源码控制的依赖包族，同时不触碰可写 envs 或已拉取的 runtimes。
for dependency_kind in tools lua ffi; do
    dependency_source="$SOURCE_LUA_RUNTIME_ROOT/dependencies/$dependency_kind"
    dependency_destination="$LUA_RUNTIME_OUT/dependencies/$dependency_kind"
    if [ -d "$dependency_source" ]; then
        reset_directory_contents "$dependency_destination"
        cp -a "$dependency_source/." "$dependency_destination/"
    fi
done

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
