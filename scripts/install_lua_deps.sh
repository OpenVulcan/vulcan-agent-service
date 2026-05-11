#!/usr/bin/env bash
# install_lua_deps.sh downloads the official LuaSkills runtime package payload into third_party.
# install_lua_deps.sh 用于下载官方 LuaSkills runtime package 载荷到 third_party。
# Developer/build use only. This script only syncs luaskills-packages runtime assets and metadata.
# 仅供开发与构建使用；该脚本只同步 luaskills-packages 的 runtime 资产与元数据。
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

# RUNTIME_INSTALL_ROOT stores the extracted official LuaSkills runtime payloads.
# RUNTIME_INSTALL_ROOT 保存解压后的官方 LuaSkills 运行时载荷。
RUNTIME_INSTALL_ROOT="$THIRD_PARTY/luaskills_runtime"

# DOWNLOAD_CACHE stores verified archives and sidecar checksums.
# DOWNLOAD_CACHE 保存已校验的压缩包与旁路校验文件。
DOWNLOAD_CACHE="$THIRD_PARTY/downloads"

# LUA_RUNTIME_REPO stores the GitHub repository for runtime package assets.
# LUA_RUNTIME_REPO 保存 runtime package 资产所在的 GitHub 仓库。
LUA_RUNTIME_REPO="${LUA_RUNTIME_REPO:-LuaSkills/luaskills-packages}"

# LUA_RUNTIME_SERIES stores the compatible major.minor series for runtime package assets.
# LUA_RUNTIME_SERIES 保存 runtime package 资产的兼容 major.minor 协议线。
LUA_RUNTIME_SERIES="${LUA_RUNTIME_SERIES:-0.1}"

# LUA_RUNTIME_PACKAGES_VERSION stores one optional exact luaskills-packages GitHub Release tag override.
# LUA_RUNTIME_PACKAGES_VERSION 保存 luaskills-packages GitHub Release 标签的可选精确覆盖值。
LUA_RUNTIME_PACKAGES_VERSION="${LUA_RUNTIME_PACKAGES_VERSION:-}"

# LUA_RUNTIME_VERSION preserves the legacy environment contract where callers often pass the luaskills crate version.
# LUA_RUNTIME_VERSION 保留旧环境变量契约；历史调用方通常会在这里传入 luaskills crate 版本号。
LUA_RUNTIME_VERSION="${LUA_RUNTIME_VERSION:-}"

ensure_dir() {
    # Create one directory when it does not already exist.
    # 当目录不存在时创建目录。
    mkdir -p "$1"
}

normalize_release_tag() {
    # Normalize one version token into a Git-style release tag.
    # 将一个版本标记规范化为 Git 风格的 release 标签。
    local value="${1:-}"
    [ -n "$value" ] || {
        echo "Release tag value cannot be empty." >&2
        return 1
    }
    case "$value" in
        v*) printf '%s\n' "$value" ;;
        *) printf 'v%s\n' "$value" ;;
    esac
}

platform_key() {
    # Resolve the current LuaSkills runtime asset platform key.
    # 解析当前 LuaSkills 运行时资产平台标识。
    local os_key arch_key
    case "$(uname -s)" in
        Linux) os_key="linux" ;;
        Darwin) os_key="macos" ;;
        *) echo "Unsupported operating system for LuaSkills runtime assets: $(uname -s)" >&2; return 1 ;;
    esac

    case "$(uname -m)" in
        x86_64|amd64) arch_key="x64" ;;
        aarch64|arm64) arch_key="arm64" ;;
        *) echo "Unsupported architecture for LuaSkills runtime assets: $(uname -m)" >&2; return 1 ;;
    esac

    printf '%s-%s\n' "$os_key" "$arch_key"
}

convert_tag_to_semver() {
    # Convert one Git tag such as v0.1.6 into a semantic-version tuple.
    # 将形如 v0.1.6 的 Git 标签转换为语义化版本元组。
    python3 - "$1" <<'PY'
import re
import sys

tag = sys.argv[1]
normalized = tag[1:] if tag.startswith("v") else tag
if not re.fullmatch(r"\d+\.\d+\.\d+", normalized):
    raise SystemExit(f"Unsupported semantic version tag: {tag}")
print(normalized)
PY
}

resolve_release_tag_for_series() {
    # Resolve the newest published GitHub release tag inside one major.minor series.
    # 解析一个 major.minor 协议线内最新的已发布 GitHub release 标签。
    local repo="$1"
    local series="$2"
    curl -fsSL "https://api.github.com/repos/${repo}/releases?per_page=100" | python3 -c '
import json
import re
import sys

series = sys.argv[1]
repo = sys.argv[2]
if not re.fullmatch(r"\d+\.\d+", series):
    raise SystemExit(f"unsupported packages series: {series}")

matches = []
for release in json.load(sys.stdin):
    if release.get("draft") or release.get("prerelease"):
        continue
    tag = str(release.get("tag_name", ""))
    normalized = tag[1:] if tag.startswith("v") else tag
    if not re.fullmatch(r"\d+\.\d+\.\d+", normalized):
        continue
    major, minor, patch = (int(part) for part in normalized.split("."))
    if f"{major}.{minor}" != series:
        continue
    matches.append(((major, minor, patch), tag))

if not matches:
    raise SystemExit(f"no published release found for {repo} series {series}")

matches.sort(key=lambda item: item[0], reverse=True)
print(matches[0][1])
' "$series" "$repo"
}

resolve_lua_runtime_packages_tag() {
    # Resolve the effective luaskills-packages release tag from exact overrides, legacy inputs, and the compatible series.
    # 基于精确覆盖、旧输入语义与兼容协议线解析最终 luaskills-packages 发布标签。
    local repo="$1"
    local series="$2"
    local packages_version="${3:-}"
    local legacy_runtime_version="${4:-}"

    if [ -n "$packages_version" ]; then
        normalize_release_tag "$packages_version"
        return 0
    fi

    if [ -z "$legacy_runtime_version" ]; then
        resolve_release_tag_for_series "$repo" "$series"
        return 0
    fi

    local legacy_tag legacy_series
    legacy_tag="$(normalize_release_tag "$legacy_runtime_version")"
    # Fail fast on malformed legacy version input so Bash matches PowerShell semantics.
    # 对格式错误的旧版本输入立即失败，确保 Bash 与 PowerShell 语义一致。
    if ! legacy_series="$(python3 - "$legacy_tag" <<'PY'
import re
import sys

tag = sys.argv[1]
normalized = tag[1:] if tag.startswith("v") else tag
if not re.fullmatch(r"\d+\.\d+\.\d+", normalized):
    raise SystemExit(1)
major, minor, _patch = normalized.split(".")
print(f"{major}.{minor}")
PY
)"; then
        echo "Unsupported LUA_RUNTIME_VERSION value '${legacy_runtime_version}'. Use a semantic version such as 0.4.2, or set LUA_RUNTIME_PACKAGES_VERSION for an exact luaskills-packages tag." >&2
        return 1
    fi

    if [ "$legacy_series" = "$series" ]; then
        printf '%s\n' "$legacy_tag"
        return 0
    fi

    echo "==> LUA_RUNTIME_VERSION=${legacy_runtime_version} detected as legacy luaskills crate version; resolving compatible luaskills-packages tag from series ${series}." >&2
    resolve_release_tag_for_series "$repo" "$series"
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

save_release_asset_with_sha256() {
    # Download one GitHub Release asset and verify its .sha256 sidecar.
    # 下载单个 GitHub Release 资产并校验其 .sha256 旁路文件。
    local repo="$1"
    local tag="$2"
    local asset_name="$3"
    local sha_asset_name="${4:-}"
    ensure_dir "$DOWNLOAD_CACHE"

    if [ -z "$sha_asset_name" ]; then
        sha_asset_name="${asset_name}.sha256"
    fi

    local archive_path="$DOWNLOAD_CACHE/$asset_name"
    local sha_path="$archive_path.sha256"
    local archive_url sha_url expected_sha256 actual_sha256
    archive_url="$(release_asset_url "$repo" "$tag" "$asset_name")"
    sha_url="$(release_asset_url "$repo" "$tag" "$sha_asset_name")"

    echo "==> Downloading checksum: $sha_url" >&2
    curl -fSL "$sha_url" -o "$sha_path"
    expected_sha256="$(awk '{print tolower($1)}' "$sha_path")"

    if archive_matches_sha256 "$archive_path" "$expected_sha256"; then
        echo "==> Reusing verified archive: $archive_path" >&2
        printf '%s\n' "$archive_path"
        return 0
    fi

    echo "==> Downloading asset: $archive_url" >&2
    curl -fSL "$archive_url" -o "$archive_path"
    actual_sha256="$(sha256_file "$archive_path")"
    if [ "$actual_sha256" != "$expected_sha256" ]; then
        echo "SHA-256 mismatch for $asset_name. Expected $expected_sha256, got $actual_sha256" >&2
        return 1
    fi

    printf '%s\n' "$archive_path"
}

clear_runtime_install_root() {
    # Clear the extracted runtime install directory inside third_party.
    # 清理 third_party 内的运行时安装目录。
    ensure_dir "$THIRD_PARTY"
    case "$RUNTIME_INSTALL_ROOT" in
        "$THIRD_PARTY"/*) rm -rf "$RUNTIME_INSTALL_ROOT" ;;
        *) echo "Refusing to clear a runtime directory outside third_party: $RUNTIME_INSTALL_ROOT" >&2; return 1 ;;
    esac
    ensure_dir "$RUNTIME_INSTALL_ROOT"
}

copy_directory_contents() {
    # Copy direct directory contents while preserving package layout.
    # 复制目录直属内容并保持包布局。
    local source_dir="$1"
    local destination_dir="$2"
    if [ ! -d "$source_dir" ]; then
        echo "Required runtime directory is missing from package: $source_dir" >&2
        return 1
    fi
    ensure_dir "$destination_dir"
    cp -a "$source_dir"/. "$destination_dir"/
}

resolve_bundle_extract_root() {
    # Resolve the extracted bundle directory that contains lua_packages metadata files.
    # 解析包含 lua_packages 元数据文件的 bundle 解压目录。
    local extract_root="$1"
    local compat_file=""
    compat_file="$(find "$extract_root" -type f -name 'lua_packages.txt' | head -1)"
    [ -n "$compat_file" ] || {
        echo "LuaSkills packages bundle does not contain lua_packages.txt." >&2
        return 1
    }
    dirname "$compat_file"
}

normalize_bundle_license_index_paths() {
    # Rewrite bundle license-index paths so they match the runtime layout.
    # 重写 bundle 授权索引路径，使其匹配运行时布局。
    local index_path="$1"
    [ -f "$index_path" ] || {
        echo "LuaSkills packages license index is missing: $index_path" >&2
        return 1
    }
    python3 - "$index_path" <<'PY'
from pathlib import Path
import sys

index_path = Path(sys.argv[1])
content = index_path.read_text(encoding="utf-8")
content = content.replace('"dist/licenses/', '"licenses/luaskills-packages/')
index_path.write_text(content, encoding="utf-8")
PY
}

runtime_install_ready() {
    # Check whether the extracted runtime root already satisfies the packaged runtime layout.
    # 检查已解压的运行根目录是否已经满足 packaged runtime 布局要求。
    local marker_file="$1"
    [ -f "$marker_file" ] &&
        [ -f "$RUNTIME_INSTALL_ROOT/resources/lua-runtime-manifest.json" ] &&
        [ -f "$RUNTIME_INSTALL_ROOT/resources/luaskills-packages-manifest.json" ] &&
        [ -f "$RUNTIME_INSTALL_ROOT/resources/luaskills-packages/THIRD_PARTY_LICENSES.json" ] &&
        [ -f "$RUNTIME_INSTALL_ROOT/resources/luaskills-packages/THIRD_PARTY_NOTICES.md" ] &&
        [ -f "$RUNTIME_INSTALL_ROOT/licenses/luaskills-packages/index.json" ]
}

install_runtime_payloads() {
    # Extract and install the runtime package plus the luaskills-packages bundle metadata into third_party.
    # 解压并安装 runtime package 与 luaskills-packages bundle 元数据到 third_party。
    local runtime_archive_path="$1"
    local bundle_archive_path="$2"
    local runtime_tag="$3"
    local platform="$4"
    local marker="$RUNTIME_INSTALL_ROOT/.installed-${runtime_tag}-${platform}"

    if runtime_install_ready "$marker"; then
        echo "==> LuaSkills runtime payloads already installed ($runtime_tag, $platform)."
        return 0
    fi

    local runtime_temp_dir bundle_temp_dir bundle_root
    runtime_temp_dir="$(mktemp -d)"
    bundle_temp_dir="$(mktemp -d)"
    trap 'rm -rf "$runtime_temp_dir" "$bundle_temp_dir"' RETURN

    tar -xzf "$runtime_archive_path" -C "$runtime_temp_dir"
    python3 - "$bundle_archive_path" "$bundle_temp_dir" <<'PY'
import sys
import zipfile

archive_path, destination = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(archive_path) as archive:
    archive.extractall(destination)
PY

    bundle_root="$(resolve_bundle_extract_root "$bundle_temp_dir")"

    clear_runtime_install_root

    local dir_name
    for dir_name in lua_packages libs resources licenses; do
        copy_directory_contents "$runtime_temp_dir/$dir_name" "$RUNTIME_INSTALL_ROOT/$dir_name"
    done

    local packages_resources_root="$RUNTIME_INSTALL_ROOT/resources/luaskills-packages"
    local packages_licenses_root="$RUNTIME_INSTALL_ROOT/licenses/luaskills-packages"
    ensure_dir "$packages_resources_root"
    ensure_dir "$packages_licenses_root"

    local file_name
    for file_name in \
        THIRD_PARTY_LICENSES.json \
        THIRD_PARTY_NOTICES.md \
        install-manifest.json \
        lua_packages.txt \
        platform-support.json \
        platform-support.md; do
        if [ -f "$bundle_root/$file_name" ]; then
            cp -f "$bundle_root/$file_name" "$packages_resources_root/$file_name"
        fi
    done

    if [ -d "$bundle_root/help" ]; then
        copy_directory_contents "$bundle_root/help" "$packages_resources_root/help"
    fi
    if [ -d "$bundle_root/licenses" ]; then
        copy_directory_contents "$bundle_root/licenses" "$packages_licenses_root"
    fi

    normalize_bundle_license_index_paths "$packages_licenses_root/index.json"

    [ -f "$RUNTIME_INSTALL_ROOT/resources/lua-runtime-manifest.json" ] || {
        echo "Lua runtime manifest was not found after installing runtime packages." >&2
        return 1
    }
    [ -f "$RUNTIME_INSTALL_ROOT/resources/luaskills-packages-manifest.json" ] || {
        echo "LuaSkills packages manifest was not found after installing runtime packages." >&2
        return 1
    }
    [ -f "$packages_resources_root/THIRD_PARTY_LICENSES.json" ] || {
        echo "LuaSkills packages third-party licenses file was not found after installing runtime packages." >&2
        return 1
    }
    [ -f "$packages_resources_root/THIRD_PARTY_NOTICES.md" ] || {
        echo "LuaSkills packages third-party notices file was not found after installing runtime packages." >&2
        return 1
    }
    [ -f "$packages_licenses_root/index.json" ] || {
        echo "LuaSkills packages license index was not found after installing runtime packages." >&2
        return 1
    }

    find "$RUNTIME_INSTALL_ROOT" -maxdepth 1 -type f -name '.installed-*' -delete
    : > "$marker"
    echo "==> LuaSkills runtime payloads installed to $RUNTIME_INSTALL_ROOT"
}

RESOLVED_LUA_RUNTIME_TAG="$(resolve_lua_runtime_packages_tag "$LUA_RUNTIME_REPO" "$LUA_RUNTIME_SERIES" "$LUA_RUNTIME_PACKAGES_VERSION" "$LUA_RUNTIME_VERSION")"
PLATFORM="$(platform_key)"
RUNTIME_ASSET_NAME="lua-runtime-packages-${PLATFORM}.tar.gz"
BUNDLE_ASSET_NAME="luaskills-packages-bundle-${RESOLVED_LUA_RUNTIME_TAG}.zip"

echo ""
echo "=== LuaSkills Runtime Packages ==="
echo "==> Runtime repo:    $LUA_RUNTIME_REPO"
echo "==> Runtime version: $RESOLVED_LUA_RUNTIME_TAG"
echo "==> Platform:        $PLATFORM"
echo "==> This flow downloads only luaskills-packages runtime assets and bundle metadata."

echo ""
echo "=== Step 1: Runtime Package ==="
RUNTIME_ARCHIVE_PATH="$(save_release_asset_with_sha256 "$LUA_RUNTIME_REPO" "$RESOLVED_LUA_RUNTIME_TAG" "$RUNTIME_ASSET_NAME")"

echo ""
echo "=== Step 2: Runtime Metadata Bundle ==="
BUNDLE_ARCHIVE_PATH="$(save_release_asset_with_sha256 "$LUA_RUNTIME_REPO" "$RESOLVED_LUA_RUNTIME_TAG" "$BUNDLE_ASSET_NAME" "luaskills-packages-bundle-${RESOLVED_LUA_RUNTIME_TAG}.sha256")"

echo ""
echo "=== Step 3: Install Runtime Payloads ==="
install_runtime_payloads "$RUNTIME_ARCHIVE_PATH" "$BUNDLE_ARCHIVE_PATH" "$RESOLVED_LUA_RUNTIME_TAG" "$PLATFORM"

echo ""
echo "==> Lua runtime dependencies ready."
