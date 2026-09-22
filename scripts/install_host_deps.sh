#!/usr/bin/env bash
# install_host_deps.sh - Install the host-side vldb-controller binary into third_party/
# Developer/build use only. End users do not need to invoke this manually.
# Usage: bash scripts/install_host_deps.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

THIRD_PARTY="$PROJECT_DIR/third_party"
VLDB_CONTROLLER_DIR="$THIRD_PARTY/vldb_controller"
VLDB_CONTROLLER_BIN_DIR="$VLDB_CONTROLLER_DIR/bin"
VLDB_CONTROLLER_REPO="OpenVulcan/vldb-controller"
VLDB_CONTROLLER_TAG="v0.2.3"

ensure_dir() { mkdir -p "$1"; }

get_release_by_tag_or_null() {
    local repo="$1"
    local tag_name="$2"
    local api_url="https://api.github.com/repos/${repo}/releases/tags/${tag_name}"
    local response_file
    response_file="$(mktemp)"
    # Authenticate CI release lookups so parallel platform jobs do not exhaust anonymous API limits.
    # 鉴权 CI 发行版查询，避免并行平台任务耗尽匿名 API 配额。
    local curl_auth_args=()
    if [ -n "${GITHUB_TOKEN:-}" ]; then
        curl_auth_args=(-H "Authorization: Bearer ${GITHUB_TOKEN}")
    fi
    local http_code
    http_code="$(curl -sSL "${curl_auth_args[@]}" -o "$response_file" -w '%{http_code}' "$api_url")"
    if [ "$http_code" = "200" ]; then
        cat "$response_file"
        rm -f "$response_file"
        return 0
    fi
    rm -f "$response_file"
    if [ "$http_code" = "404" ]; then
        return 0
    fi
    echo "ERROR: release lookup for tag '$tag_name' failed with HTTP $http_code" >&2
    return 1
}

get_current_architecture() {
    case "$(uname -m)" in
        x86_64|amd64) echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
        *) echo "unsupported" ;;
    esac
}

get_vldb_controller_asset_info() {
    local arch
    arch="$(get_current_architecture)"
    [ "$arch" = "unsupported" ] && {
        echo "ERROR: unsupported architecture for vldb-controller bootstrap: $(uname -m)" >&2
        return 1
    }

    case "$(uname -s)" in
        Linux)
            if [ "$arch" = "aarch64" ]; then
                echo "aarch64-unknown-linux-gnu|.tar.gz|vldb-controller"
            else
                echo "x86_64-unknown-linux-gnu|.tar.gz|vldb-controller"
            fi
            ;;
        Darwin)
            if [ "$arch" = "aarch64" ]; then
                echo "aarch64-apple-darwin|.tar.gz|vldb-controller"
            else
                echo "x86_64-apple-darwin|.tar.gz|vldb-controller"
            fi
            ;;
        *)
            echo "ERROR: unsupported platform for vldb-controller bootstrap: $(uname -s)" >&2
            return 1
            ;;
    esac
}

install_vldb_controller_binary() {
    ensure_dir "$VLDB_CONTROLLER_DIR"
    ensure_dir "$VLDB_CONTROLLER_BIN_DIR"

    local asset_info
    asset_info="$(get_vldb_controller_asset_info)" || return 1
    local target archive_ext binary_name
    IFS='|' read -r target archive_ext binary_name <<< "$asset_info"

    local release_data=""
    local tag_name=""
    local asset_name=""
    local marker=""
    local local_archive=""
    local binary_dest="$VLDB_CONTROLLER_BIN_DIR/$binary_name"

    local local_pattern="vldb-controller-v*-${target}${archive_ext}"
    local_archive="$(find "$THIRD_PARTY" -maxdepth 2 -type f -name "$local_pattern" | sort -r | head -1)"
    if [ -n "$local_archive" ]; then
        asset_name="$(basename "$local_archive")"
        tag_name="$(printf '%s' "$asset_name" | python3 -c "
import re, sys
name = sys.stdin.read().strip()
match = re.match(r'^vldb-controller-(v.+)-[^-]+(?:-[^-]+){2,3}(?:\\.zip|\\.tar\\.gz)$', name)
print(match.group(1) if match else '')
")"
        if [ -z "$tag_name" ]; then
            echo "ERROR: unable to parse vldb-controller tag from local archive name: $asset_name" >&2
            return 1
        fi
        marker="$VLDB_CONTROLLER_DIR/.installed-${tag_name}-${target}"
    else
        tag_name="$VLDB_CONTROLLER_TAG"
        asset_name="vldb-controller-${tag_name}-${target}${archive_ext}"
        marker="$VLDB_CONTROLLER_DIR/.installed-${tag_name}-${target}"
        local_archive="$(find "$THIRD_PARTY" -maxdepth 2 -type f -name "$asset_name" | head -1)"
    fi

    if [ -f "$marker" ] && [ -f "$binary_dest" ]; then
        echo "==> vldb-controller host binary already installed ($asset_name)."
        return 0
    fi

    local download_url=""
    if [ -z "$local_archive" ]; then
        release_data="$(get_release_by_tag_or_null "$VLDB_CONTROLLER_REPO" "$tag_name")" || return 1
        if [ -z "$release_data" ]; then
            if [ -f "$binary_dest" ]; then
                echo "WARNING: vldb-controller release assets are not published for tag $tag_name. Reusing the existing local binary at $binary_dest and refreshing the install marker." >&2
                find "$VLDB_CONTROLLER_DIR" -maxdepth 1 -type f -name '.installed-*' -delete
                : > "$marker"
                return 0
            fi
            echo "ERROR: vldb-controller tag '$tag_name' currently has no GitHub Release host binary asset. Please download the artifact '$asset_name' manually and place it under third_party (or one direct child directory) before rerunning." >&2
            return 1
        fi

        download_url="$(printf '%s' "$release_data" | python3 -c "
import json, sys
data = json.load(sys.stdin)
for asset in data.get('assets', []):
    if asset.get('name') == '$asset_name':
        print(asset.get('browser_download_url', ''))
        break
")"
        if [ -z "$download_url" ]; then
            echo "ERROR: vldb-controller asset '$asset_name' not found in release '$tag_name'." >&2
            return 1
        fi
    fi

    local temp_dir
    temp_dir="$(mktemp -d)"
    trap 'rm -rf "$temp_dir"' RETURN

    local archive_path="$temp_dir/$asset_name"
    if [ -n "$local_archive" ]; then
        echo "==> Using local vldb-controller package: $local_archive"
        cp "$local_archive" "$archive_path"
    else
        echo "==> Downloading vldb-controller package: $asset_name"
        curl -fSL "$download_url" -o "$archive_path"
    fi

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

    local binary_source
    binary_source="$(find "$temp_dir" -type f -name "$binary_name" | head -1)"
    if [ -z "$binary_source" ]; then
        echo "ERROR: executable '$binary_name' not found after extracting $asset_name" >&2
        return 1
    fi

    cp "$binary_source" "$binary_dest"

    find "$VLDB_CONTROLLER_DIR" -maxdepth 1 -type f -name '.installed-*' -delete
    : > "$marker"
    echo "==> vldb-controller host binary installed successfully."
}

install_vldb_controller_binary
