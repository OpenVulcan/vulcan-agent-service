#!/usr/bin/env bash
# install_host_deps.sh — Install host-level native runtime dependencies into third_party/
# Developer/build use only. End users do not need to invoke this manually.
# This script currently provisions the vldb-lancedb dynamic library package.
# Usage: bash scripts/install_host_deps.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

THIRD_PARTY="$PROJECT_DIR/third_party"
DEPS_DIR="$THIRD_PARTY/deps"
VLDB_LANCEDB_DIR="$THIRD_PARTY/vldb_lancedb"
VLDB_LANCEDB_INCLUDE_DIR="$VLDB_LANCEDB_DIR/include"
VLDB_LANCEDB_DOCS_DIR="$VLDB_LANCEDB_DIR/docs"
VLDB_LANCEDB_REPO="OpenVulcan/vldb-lancedb"

ensure_dir() { mkdir -p "$1"; }

get_current_architecture() {
    # 将当前 CPU 架构规范化为发布资产命名所需的键。
    # Normalize the current CPU architecture to the key used by release asset names.
    case "$(uname -m)" in
        x86_64|amd64) echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
        *) echo "unsupported" ;;
    esac
}

get_vldb_lancedb_asset_info() {
    # 根据当前平台推导 vldb-lancedb 库模式包信息。
    # Derive the vldb-lancedb library package information for the current platform.
    local arch
    arch="$(get_current_architecture)"
    [ "$arch" = "unsupported" ] && {
        echo "ERROR: unsupported architecture for vldb-lancedb bootstrap: $(uname -m)" >&2
        return 1
    }

    case "$(uname -s)" in
        Linux)
            if [ "$arch" = "aarch64" ]; then
                echo "aarch64-unknown-linux-gnu|.tar.gz|libvldb_lancedb.so"
            else
                echo "x86_64-unknown-linux-gnu|.tar.gz|libvldb_lancedb.so"
            fi
            ;;
        Darwin)
            if [ "$arch" = "aarch64" ]; then
                echo "aarch64-apple-darwin|.tar.gz|libvldb_lancedb.dylib"
            else
                echo "x86_64-apple-darwin|.tar.gz|libvldb_lancedb.dylib"
            fi
            ;;
        *)
            echo "ERROR: unsupported platform for vldb-lancedb bootstrap: $(uname -s)" >&2
            return 1
            ;;
    esac
}

install_vldb_lancedb_library() {
    # 安装宿主级 vldb-lancedb 动态库。
    # Install the host-level vldb-lancedb dynamic library.
    ensure_dir "$DEPS_DIR"
    ensure_dir "$VLDB_LANCEDB_DIR"
    ensure_dir "$VLDB_LANCEDB_INCLUDE_DIR"
    ensure_dir "$VLDB_LANCEDB_DOCS_DIR"

    local asset_info
    asset_info="$(get_vldb_lancedb_asset_info)" || return 1
    local target archive_ext library_name
    IFS='|' read -r target archive_ext library_name <<< "$asset_info"

    local api_url="https://api.github.com/repos/${VLDB_LANCEDB_REPO}/releases/latest"
    echo "==> Querying latest vldb-lancedb release..."

    local release_data
    release_data="$(curl -fSL -s "$api_url")"

    local tag_name
    tag_name="$(printf '%s' "$release_data" | python3 -c 'import json,sys; data=json.load(sys.stdin); print(data.get("tag_name",""))')"
    if [ -z "$tag_name" ]; then
        echo "ERROR: latest vldb-lancedb release is missing tag_name" >&2
        return 1
    fi

    local asset_name="vldb-lancedb-lib-${tag_name}-${target}${archive_ext}"
    local marker="$VLDB_LANCEDB_DIR/.installed-${tag_name}-${target}"
    local library_dest="$DEPS_DIR/$library_name"

    if [ -f "$marker" ] && [ -f "$library_dest" ]; then
        echo "==> vldb-lancedb library already installed ($asset_name)."
        return 0
    fi

    local download_url
    download_url="$(printf '%s' "$release_data" | python3 -c "
import json, sys
data = json.load(sys.stdin)
for asset in data.get('assets', []):
    if asset.get('name') == '$asset_name':
        print(asset.get('browser_download_url', ''))
        break
")"
    if [ -z "$download_url" ]; then
        echo "ERROR: vldb-lancedb asset '$asset_name' not found in latest release." >&2
        return 1
    fi

    local temp_dir
    temp_dir="$(mktemp -d)"
    trap 'rm -rf "$temp_dir"' RETURN

    local archive_path="$temp_dir/$asset_name"
    echo "==> Downloading vldb-lancedb library package: $asset_name"
    curl -fSL "$download_url" -o "$archive_path"

    if [ "$archive_ext" = ".zip" ]; then
        python3 - "$archive_path" "$temp_dir" <<'PY'
import sys, zipfile
archive, dest = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(archive) as zf:
    zf.extractall(dest)
PY
    else
        tar -xzf "$archive_path" -C "$temp_dir"
    fi

    local library_source
    library_source="$(find "$temp_dir" -type f -name "$library_name" | head -1)"
    if [ -z "$library_source" ]; then
        echo "ERROR: dynamic library '$library_name' not found after extracting $asset_name" >&2
        return 1
    fi

    cp "$library_source" "$library_dest"

    local header_source
    header_source="$(find "$temp_dir" -type f -name 'vldb_lancedb.h' | head -1)"
    if [ -n "$header_source" ]; then
        cp "$header_source" "$VLDB_LANCEDB_INCLUDE_DIR/vldb_lancedb.h"
    fi

    local doc_source
    doc_source="$(find "$temp_dir" -type f -name 'LIBRARY_USAGE.zh-CN.md' | head -1)"
    if [ -n "$doc_source" ]; then
        cp "$doc_source" "$VLDB_LANCEDB_DOCS_DIR/LIBRARY_USAGE.zh-CN.md"
    fi

    find "$VLDB_LANCEDB_DIR" -maxdepth 1 -type f -name '.installed-*' -delete
    : > "$marker"
    echo "==> vldb-lancedb library installed successfully."
}

install_vldb_lancedb_library
