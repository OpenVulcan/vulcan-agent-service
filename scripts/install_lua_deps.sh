#!/usr/bin/env bash
# install_lua_deps.sh downloads the official LuaSkills runtime dependency package.
# install_lua_deps.sh 用于下载 LuaSkills 官方运行期依赖包。
# Developer/build use only. The script does not compile Lua, LuaRocks, or C dependencies.
# 仅供开发与构建使用；该脚本不会编译 Lua、LuaRocks 或 C 依赖。
# Usage: bash scripts/install_lua_deps.sh

set -euo pipefail

# SCRIPT_DIR stores the script directory for stable sibling script resolution.
# SCRIPT_DIR 保存脚本目录，确保同级脚本解析稳定。
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# PROJECT_DIR stores the repository root.
# PROJECT_DIR 保存仓库根目录。
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# THIRD_PARTY stores downloaded dependency payloads outside tracked source.
# THIRD_PARTY 保存源码目录外的下载依赖载荷。
THIRD_PARTY="$PROJECT_DIR/third_party"

# RUNTIME_INSTALL_ROOT stores the extracted official LuaSkills runtime package.
# RUNTIME_INSTALL_ROOT 保存解压后的 LuaSkills 官方运行期包。
RUNTIME_INSTALL_ROOT="$THIRD_PARTY/luaskills_runtime"

# DOWNLOAD_CACHE stores verified archives and sidecar checksums.
# DOWNLOAD_CACHE 保存已校验的压缩包与旁路校验文件。
DOWNLOAD_CACHE="$THIRD_PARTY/downloads"

# RUNTIME_REPO stores the official LuaSkills repository that publishes runtime packages.
# RUNTIME_REPO 保存发布运行期包的 LuaSkills 官方仓库。
RUNTIME_REPO="${LUA_RUNTIME_REPO:-LuaSkills/luaskills}"

# HOST_DEPS_SCRIPT_PATH points at the host-native dependency downloader.
# HOST_DEPS_SCRIPT_PATH 指向宿主原生依赖下载脚本。
HOST_DEPS_SCRIPT_PATH="$SCRIPT_DIR/install_host_deps.sh"

ensure_dir() {
    # Create one directory when it does not already exist.
    # 当目录不存在时创建目录。
    mkdir -p "$1"
}

luaskills_version_tag() {
    # Resolve the LuaSkills release tag from the environment or Cargo dependency.
    # 从环境变量或 Cargo 依赖解析 LuaSkills 发布标签。
    if [ -n "${LUA_RUNTIME_VERSION:-}" ]; then
        case "$LUA_RUNTIME_VERSION" in
            v*) printf '%s\n' "$LUA_RUNTIME_VERSION" ;;
            *) printf 'v%s\n' "$LUA_RUNTIME_VERSION" ;;
        esac
        return 0
    fi

    if [ -f "$PROJECT_DIR/Cargo.toml" ]; then
        local version
        version="$(sed -nE 's/^[[:space:]]*luaskills[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' "$PROJECT_DIR/Cargo.toml" | head -1)"
        if [ -n "$version" ]; then
            printf 'v%s\n' "$version"
            return 0
        fi
    fi

    printf 'v0.2.2\n'
}

current_platform_key() {
    # Resolve the official LuaSkills runtime package platform key.
    # 解析 LuaSkills 官方运行期包的平台标识。
    local os_key arch_key
    case "$(uname -s)" in
        Linux) os_key="linux" ;;
        Darwin) os_key="macos" ;;
        *) echo "Unsupported operating system for LuaSkills runtime package: $(uname -s)" >&2; return 1 ;;
    esac

    case "$(uname -m)" in
        x86_64|amd64) arch_key="x64" ;;
        aarch64|arm64) arch_key="arm64" ;;
        *) echo "Unsupported architecture for LuaSkills runtime package: $(uname -m)" >&2; return 1 ;;
    esac

    printf '%s-%s\n' "$os_key" "$arch_key"
}

release_asset_url() {
    # Build the direct GitHub Release asset URL.
    # 构造 GitHub Release 资产的直接下载地址。
    local repo="$1"
    local tag="$2"
    local asset_name="$3"
    printf 'https://github.com/%s/releases/download/%s/%s\n' "$repo" "$tag" "$asset_name"
}

sha256_file() {
    # Calculate one file's SHA-256 digest with common platform tools.
    # 使用常见平台工具计算单个文件的 SHA-256 摘要。
    local file_path="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$file_path" | awk '{print tolower($1)}'
        return 0
    fi
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$file_path" | awk '{print tolower($1)}'
        return 0
    fi
    echo "No SHA-256 tool found. Install sha256sum or shasum." >&2
    return 1
}

archive_matches_sha256() {
    # Check whether one archive matches its expected SHA-256 digest.
    # 检查压缩包是否匹配期望的 SHA-256 摘要。
    local archive_path="$1"
    local expected_sha256="$2"
    [ -f "$archive_path" ] || return 1
    local actual_sha256
    actual_sha256="$(sha256_file "$archive_path")"
    [ "$actual_sha256" = "$expected_sha256" ]
}

save_official_lua_runtime_archive() {
    # Download and verify the official LuaSkills runtime package archive.
    # 下载并校验 LuaSkills 官方运行期包。
    local repo="$1"
    local tag="$2"
    local asset_name="$3"
    ensure_dir "$DOWNLOAD_CACHE"

    local archive_path="$DOWNLOAD_CACHE/$asset_name"
    local sha_path="$archive_path.sha256"
    local archive_url sha_url expected_sha256 actual_sha256
    archive_url="$(release_asset_url "$repo" "$tag" "$asset_name")"
    sha_url="$(release_asset_url "$repo" "$tag" "$asset_name.sha256")"

    echo "==> Downloading checksum: $sha_url" >&2
    curl -fSL "$sha_url" -o "$sha_path"
    expected_sha256="$(awk '{print tolower($1)}' "$sha_path")"

    if archive_matches_sha256 "$archive_path" "$expected_sha256"; then
        echo "==> Reusing verified LuaSkills runtime archive: $archive_path" >&2
        printf '%s\n' "$archive_path"
        return 0
    fi

    echo "==> Downloading LuaSkills runtime package: $archive_url" >&2
    curl -fSL "$archive_url" -o "$archive_path"
    actual_sha256="$(sha256_file "$archive_path")"
    if [ "$actual_sha256" != "$expected_sha256" ]; then
        echo "SHA-256 mismatch for $asset_name. Expected $expected_sha256, got $actual_sha256" >&2
        return 1
    fi

    printf '%s\n' "$archive_path"
}

clear_runtime_install_root() {
    # Clear the extracted official runtime package directory inside third_party.
    # 清理 third_party 内已解压的官方运行期包目录。
    ensure_dir "$THIRD_PARTY"
    case "$RUNTIME_INSTALL_ROOT" in
        "$THIRD_PARTY"/*) rm -rf "$RUNTIME_INSTALL_ROOT" ;;
        *) echo "Refusing to clear a runtime directory outside third_party: $RUNTIME_INSTALL_ROOT" >&2; return 1 ;;
    esac
    ensure_dir "$RUNTIME_INSTALL_ROOT"
}

copy_directory_contents() {
    # Copy direct directory contents while preserving the official package layout.
    # 复制目录直属内容并保持官方包布局。
    local source_dir="$1"
    local destination_dir="$2"
    if [ ! -d "$source_dir" ]; then
        echo "Required LuaSkills runtime directory is missing from package: $source_dir" >&2
        return 1
    fi
    ensure_dir "$destination_dir"
    cp -a "$source_dir"/. "$destination_dir"/
}

install_official_lua_runtime_package() {
    # Extract the official LuaSkills runtime archive into third_party/luaskills_runtime.
    # 将 LuaSkills 官方运行期压缩包解压到 third_party/luaskills_runtime。
    local archive_path="$1"
    local tag="$2"
    local platform="$3"
    local marker="$RUNTIME_INSTALL_ROOT/.installed-${tag}-${platform}"
    local manifest="$RUNTIME_INSTALL_ROOT/resources/lua-runtime-manifest.json"

    if [ -f "$marker" ] && [ -f "$manifest" ]; then
        echo "==> LuaSkills runtime package already installed ($tag, $platform)."
        return 0
    fi

    local temp_dir
    temp_dir="$(mktemp -d)"
    tar -xzf "$archive_path" -C "$temp_dir"
    clear_runtime_install_root

    local dir_name
    for dir_name in lua_packages libs resources licenses; do
        copy_directory_contents "$temp_dir/$dir_name" "$RUNTIME_INSTALL_ROOT/$dir_name"
    done

    : > "$marker"
    rm -rf "$temp_dir"
    echo "==> LuaSkills official runtime installed to $RUNTIME_INSTALL_ROOT"
}

invoke_host_dependency_install() {
    # Download host-native dependencies that are not part of the LuaSkills runtime package.
    # 下载不属于 LuaSkills runtime 包的宿主原生依赖。
    if [ ! -f "$HOST_DEPS_SCRIPT_PATH" ]; then
        echo "Missing host dependency script: $HOST_DEPS_SCRIPT_PATH" >&2
        return 1
    fi
    bash "$HOST_DEPS_SCRIPT_PATH"
}

RUNTIME_VERSION="$(luaskills_version_tag)"
PLATFORM="$(current_platform_key)"
ASSET_NAME="lua-runtime-${PLATFORM}.tar.gz"

echo ""
echo "=== LuaSkills Official Dependencies ==="
echo "==> Repository: $RUNTIME_REPO"
echo "==> Version:    $RUNTIME_VERSION"
echo "==> Platform:   $PLATFORM"
echo "==> This flow downloads official packages only; no LuaRocks, vcpkg, or source compilation is performed."

echo ""
echo "=== Step 1: Host Dependencies ==="
invoke_host_dependency_install

echo ""
echo "=== Step 2: LuaSkills Runtime Package ==="
ARCHIVE_PATH="$(save_official_lua_runtime_archive "$RUNTIME_REPO" "$RUNTIME_VERSION" "$ASSET_NAME")"
install_official_lua_runtime_package "$ARCHIVE_PATH" "$RUNTIME_VERSION" "$PLATFORM"

echo ""
echo "==> Dependencies ready."
