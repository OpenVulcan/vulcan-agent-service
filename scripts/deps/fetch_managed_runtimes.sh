#!/usr/bin/env bash
set -euo pipefail

# ProjectRoot points at the repository root regardless of the caller location.
# ProjectRoot 指向仓库根目录，避免调用方当前位置影响路径解析。
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$PROJECT_ROOT"

# Target selects which managed runtime group to fetch.
# Target 选择需要拉取的受管运行时分组。
TARGET="${1:-all}"

# RuntimeRoot selects the LuaSkills/build root used for fixed defaults and staging.
# RuntimeRoot 选择用于固定默认布局与暂存的 LuaSkills/构建根。
RUNTIME_ROOT="${RUNTIME_ROOT:-third_party/managed_runtime_cache}"

# DistributionRoot optionally targets the directory that directly contains python and node.
# DistributionRoot 可选指定直接包含 python 与 node 的目录。
DISTRIBUTION_ROOT="${MANAGED_RUNTIME_DISTRIBUTION_ROOT:-third_party/luaskills_managed_runtimes}"

# PythonVersion selects the managed CPython version installed through uv.
# PythonVersion 选择通过 uv 安装的受管 CPython 版本。
PYTHON_VERSION="${PYTHON_VERSION:-3.14.6}"

# UvVersion selects the standalone uv binary version.
# UvVersion 选择独立 uv 二进制版本。
UV_VERSION="${UV_VERSION:-0.11.28}"

# NodeVersion selects the managed Node.js version.
# NodeVersion 选择受管 Node.js 版本。
NODE_VERSION="${NODE_VERSION:-24.18.0}"

# PnpmVersion selects the pnpm package-manager version.
# PnpmVersion 选择 pnpm 包管理器版本。
PNPM_VERSION="${PNPM_VERSION:-11.11.0}"

# Force removes existing targets before reinstalling them.
# Force 会在重新安装前删除已有目标目录。
FORCE="${FORCE:-0}"

ensure_dir() {
  # Create one directory when it does not exist.
  # 在目录不存在时创建该目录。
  mkdir -p "$1"
}

sha256_file() {
  # Compute one file SHA-256 digest as lowercase hex.
  # 将单个文件计算为小写十六进制 SHA-256 摘要。
  local path="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$path" | awk '{print tolower($1)}'
  else
    shasum -a 256 "$path" | awk '{print tolower($1)}'
  fi
}

sha512_base64_file() {
  # Compute one file SHA-512 digest as base64 for npm integrity checks.
  # 将单个文件计算为用于 npm integrity 校验的 SHA-512 Base64 摘要。
  local path="$1"
  python3 - "$path" <<'PY'
import base64
import hashlib
import sys

digest = hashlib.sha512()
with open(sys.argv[1], "rb") as handle:
    for chunk in iter(lambda: handle.read(1024 * 1024), b""):
        digest.update(chunk)
print(base64.b64encode(digest.digest()).decode("ascii"))
PY
}

managed_runtime_platform() {
  # Resolve platform metadata used by managed runtime binary assets.
  # 解析受管运行时二进制资产使用的平台元数据。
  local os_key arch_key rust_arch node_arch
  case "$(uname -s)" in
    Linux) os_key="linux" ;;
    Darwin) os_key="macos" ;;
    *) echo "Unsupported operating system: $(uname -s)" >&2; return 1 ;;
  esac
  case "$(uname -m)" in
    x86_64|amd64)
      arch_key="x64"
      rust_arch="x86_64"
      node_arch="x64"
      ;;
    aarch64|arm64)
      arch_key="arm64"
      rust_arch="aarch64"
      node_arch="arm64"
      ;;
    *) echo "Unsupported architecture: $(uname -m)" >&2; return 1 ;;
  esac

  if [ "$os_key" = "linux" ]; then
    printf '%s|uv-%s-unknown-linux-gnu.tar.gz|node-v%%s-linux-%s.tar.xz|node-v%%s-linux-%s\n' \
      "$os_key-$arch_key" "$rust_arch" "$node_arch" "$node_arch"
  else
    printf '%s|uv-%s-apple-darwin.tar.gz|node-v%%s-darwin-%s.tar.gz|node-v%%s-darwin-%s\n' \
      "$os_key-$arch_key" "$rust_arch" "$node_arch" "$node_arch"
  fi
}

download_url() {
  # Download one URL to a local path.
  # 将一个 URL 下载到本地路径。
  local url="$1"
  local destination="$2"
  ensure_dir "$(dirname "$destination")"
  curl -fL "$url" -o "$destination"
}

extract_archive() {
  # Extract one tar.gz or tar.xz archive into a destination directory.
  # 将 tar.gz 或 tar.xz 归档解压到目标目录。
  local archive="$1"
  local destination="$2"
  ensure_dir "$destination"
  tar -xf "$archive" -C "$destination"
}

write_runtime_manifest() {
  # Write one stable runtime-manifest.json file.
  # 写入一个稳定的 runtime-manifest.json 文件。
  local directory="$1"
  local runtime="$2"
  local version="$3"
  local platform="$4"
  local executable="$5"
  local source="$6"
  ensure_dir "$directory"
  python3 - "$directory/runtime-manifest.json" "$runtime" "$version" "$platform" "$executable" "$source" <<'PY'
import json
import sys

path, runtime, version, platform, executable, source = sys.argv[1:]
payload = {
    "schema_version": 1,
    "runtime": runtime,
    "version": version,
    "platform": platform,
    "executable": executable,
    "source": source,
}
with open(path, "w", encoding="utf-8") as handle:
    json.dump(payload, handle, indent=2)
    handle.write("\n")
PY
}

install_uv_runtime() {
  # Download and install one standalone uv binary into the runtime root.
  # 下载并安装一个独立 uv 二进制到运行时根目录。
  local platform_key="$1"
  local uv_asset="$2"
  local uv_target="$DISTRIBUTION_ROOT/python/uv-$UV_VERSION-$platform_key"
  local uv_exe="$uv_target/uv"
  if [ -x "$uv_exe" ] && [ "$FORCE" != "1" ]; then
    printf '%s\n' "$uv_exe"
    return
  fi
  if [ -d "$uv_target" ] && [ "$FORCE" = "1" ]; then
    rm -rf "$uv_target"
  fi

  local asset_url="https://github.com/astral-sh/uv/releases/download/$UV_VERSION/$uv_asset"
  local checksum_url="$asset_url.sha256"
  local temp_dir="$RUNTIME_ROOT/temp/managed-runtimes/uv-$UV_VERSION-$platform_key"
  rm -rf "$temp_dir"
  ensure_dir "$temp_dir"
  local archive="$temp_dir/$uv_asset"
  local checksum_path="$temp_dir/$uv_asset.sha256"
  download_url "$asset_url" "$archive"
  download_url "$checksum_url" "$checksum_path"

  local expected actual
  expected="$(awk '{print tolower($1); exit}' "$checksum_path")"
  actual="$(sha256_file "$archive")"
  if [ "$expected" != "$actual" ]; then
    echo "SHA-256 mismatch for $uv_asset. Expected $expected, got $actual" >&2
    return 1
  fi

  local extract_dir="$temp_dir/extract"
  extract_archive "$archive" "$extract_dir"
  ensure_dir "$uv_target"
  local extracted_uv
  extracted_uv="$(find "$extract_dir" -type f -name uv | head -n 1)"
  if [ -z "$extracted_uv" ]; then
    echo "uv executable not found in $uv_asset" >&2
    return 1
  fi
  cp "$extracted_uv" "$uv_exe"
  chmod +x "$uv_exe"
  write_runtime_manifest "$uv_target" "uv" "$UV_VERSION" "$platform_key" "uv" "$asset_url"
  "$uv_exe" --version >&2
  printf '%s\n' "$uv_exe"
}

install_python_runtime() {
  # Install one managed CPython runtime through the managed uv binary.
  # 通过受管 uv 二进制安装一个受管 CPython 运行时。
  local platform_key="$1"
  local uv_asset="$2"
  local uv_exe
  uv_exe="$(install_uv_runtime "$platform_key" "$uv_asset")"
  local python_root="$DISTRIBUTION_ROOT/python/cpython-$PYTHON_VERSION-$platform_key"
  if [ -f "$python_root/runtime-manifest.json" ] && [ "$FORCE" != "1" ]; then
    return
  fi
  if [ -d "$python_root" ] && [ "$FORCE" = "1" ]; then
    rm -rf "$python_root"
  fi
  ensure_dir "$python_root"

  if [ "$FORCE" = "1" ]; then
    UV_PYTHON_INSTALL_DIR="$python_root" "$uv_exe" python install "$PYTHON_VERSION" --reinstall
  else
    UV_PYTHON_INSTALL_DIR="$python_root" "$uv_exe" python install "$PYTHON_VERSION"
  fi

  local python_exe relative_exe
  python_exe="$(UV_PYTHON_INSTALL_DIR="$python_root" "$uv_exe" python find "$PYTHON_VERSION" | head -n 1)"
  if [ -z "$python_exe" ] || [ ! -x "$python_exe" ]; then
    echo "uv installed Python $PYTHON_VERSION but no interpreter path could be resolved" >&2
    return 1
  fi
  relative_exe="$(python3 - "$python_root" "$python_exe" <<'PY'
import os
import sys

print(os.path.relpath(os.path.realpath(sys.argv[2]), os.path.realpath(sys.argv[1])))
PY
)"
  write_runtime_manifest "$python_root" "python" "$PYTHON_VERSION" "$platform_key" "$relative_exe" "uv-managed-python"
  "$python_exe" --version >&2
}

install_node_runtime() {
  # Download and install one managed Node.js archive from nodejs.org.
  # 从 nodejs.org 下载并安装一个受管 Node.js 归档。
  local platform_key="$1"
  local node_asset_template="$2"
  local node_extract_template="$3"
  local node_target="$DISTRIBUTION_ROOT/node/node-$NODE_VERSION-$platform_key"
  local node_exe="$node_target/bin/node"
  if [ -x "$node_exe" ] && [ "$FORCE" != "1" ]; then
    printf '%s\n' "$node_exe"
    return
  fi
  if [ -d "$node_target" ] && [ "$FORCE" = "1" ]; then
    rm -rf "$node_target"
  fi

  local asset_name extract_name base_url asset_url shasums_url
  asset_name="$(printf "$node_asset_template" "$NODE_VERSION")"
  extract_name="$(printf "$node_extract_template" "$NODE_VERSION")"
  base_url="https://nodejs.org/dist/v$NODE_VERSION"
  asset_url="$base_url/$asset_name"
  shasums_url="$base_url/SHASUMS256.txt"

  local temp_dir="$RUNTIME_ROOT/temp/managed-runtimes/node-$NODE_VERSION-$platform_key"
  rm -rf "$temp_dir"
  ensure_dir "$temp_dir"
  local archive="$temp_dir/$asset_name"
  local shasums_path="$temp_dir/SHASUMS256.txt"
  download_url "$asset_url" "$archive"
  download_url "$shasums_url" "$shasums_path"

  local expected actual
  expected="$(awk -v name="$asset_name" '$2 == name {print tolower($1); exit}' "$shasums_path")"
  if [ -z "$expected" ]; then
    echo "Checksum entry for $asset_name not found in SHASUMS256.txt" >&2
    return 1
  fi
  actual="$(sha256_file "$archive")"
  if [ "$expected" != "$actual" ]; then
    echo "SHA-256 mismatch for $asset_name. Expected $expected, got $actual" >&2
    return 1
  fi

  local extract_dir="$temp_dir/extract"
  extract_archive "$archive" "$extract_dir"
  local extracted_root="$extract_dir/$extract_name"
  if [ ! -d "$extracted_root" ]; then
    echo "Node archive root '$extract_name' not found in $asset_name" >&2
    return 1
  fi
  ensure_dir "$(dirname "$node_target")"
  mv "$extracted_root" "$node_target"
  write_runtime_manifest "$node_target" "node" "$NODE_VERSION" "$platform_key" "bin/node" "$asset_url"
  "$node_exe" --version >&2
  printf '%s\n' "$node_exe"
}

install_pnpm_runtime() {
  # Download and install pnpm from npm registry without touching global npm state.
  # 从 npm registry 下载并安装 pnpm，且不触碰全局 npm 状态。
  local platform_key="$1"
  local node_asset_template="$2"
  local node_extract_template="$3"
  local node_exe
  node_exe="$(install_node_runtime "$platform_key" "$node_asset_template" "$node_extract_template")"
  local pnpm_target="$DISTRIBUTION_ROOT/node/pnpm-$PNPM_VERSION"
  local pnpm_entry="$pnpm_target/bin/pnpm.cjs"
  if [ -f "$pnpm_entry" ] && [ "$FORCE" != "1" ]; then
    return
  fi
  if [ -d "$pnpm_target" ] && [ "$FORCE" = "1" ]; then
    rm -rf "$pnpm_target"
  fi

  local temp_dir="$RUNTIME_ROOT/temp/managed-runtimes/pnpm-$PNPM_VERSION"
  rm -rf "$temp_dir"
  ensure_dir "$temp_dir"

  local metadata_path="$temp_dir/pnpm-metadata.json"
  download_url "https://registry.npmjs.org/pnpm/$PNPM_VERSION" "$metadata_path"
  local tarball_url integrity
  tarball_url="$(python3 - "$metadata_path" <<'PY'
import json
import sys

data = json.load(open(sys.argv[1], encoding="utf-8"))
print(data["dist"]["tarball"])
PY
)"
  integrity="$(python3 - "$metadata_path" <<'PY'
import json
import sys

data = json.load(open(sys.argv[1], encoding="utf-8"))
print(data["dist"]["integrity"])
PY
)"
  if [[ "$integrity" != sha512-* ]]; then
    echo "pnpm metadata for $PNPM_VERSION does not contain a sha512 integrity tarball" >&2
    return 1
  fi

  local tarball="$temp_dir/pnpm-$PNPM_VERSION.tgz"
  download_url "$tarball_url" "$tarball"
  local expected actual
  expected="${integrity#sha512-}"
  actual="$(sha512_base64_file "$tarball")"
  if [ "$expected" != "$actual" ]; then
    echo "SHA-512 integrity mismatch for pnpm $PNPM_VERSION" >&2
    return 1
  fi

  local extract_dir="$temp_dir/extract"
  extract_archive "$tarball" "$extract_dir"
  local package_root="$extract_dir/package"
  if [ ! -d "$package_root" ]; then
    echo "pnpm package root not found in tarball" >&2
    return 1
  fi
  ensure_dir "$(dirname "$pnpm_target")"
  mv "$package_root" "$pnpm_target"
  # The npm tarball does not preserve an executable bit, but the managed manifest exposes this shebang entry.
  # npm tarball 不保留可执行位，但受管清单会将这个含 shebang 的入口作为可执行文件暴露。
  chmod +x "$pnpm_entry"
  write_runtime_manifest "$pnpm_target" "pnpm" "$PNPM_VERSION" "any" "bin/pnpm.cjs" "$tarball_url"
  "$node_exe" "$pnpm_entry" --version >&2
}

IFS='|' read -r PLATFORM_KEY UV_ASSET NODE_ASSET_TEMPLATE NODE_EXTRACT_TEMPLATE < <(managed_runtime_platform)
RUNTIME_ROOT="$(python3 - "$RUNTIME_ROOT" <<'PY'
import os
import sys

path = os.path.abspath(sys.argv[1])
os.makedirs(path, exist_ok=True)
print(path)
PY
)"
# DistributionRoot defaults to the fixed runtime-root layout and otherwise uses the exact host path.
# DistributionRoot 默认使用固定的 runtime-root 布局，否则使用精确宿主路径。
if [ -z "$DISTRIBUTION_ROOT" ]; then
  DISTRIBUTION_ROOT="$RUNTIME_ROOT/dependencies/runtimes"
fi
DISTRIBUTION_ROOT="$(python3 - "$DISTRIBUTION_ROOT" <<'PY'
import os
import sys

path = os.path.abspath(sys.argv[1])
os.makedirs(path, exist_ok=True)
print(path)
PY
)"

case "$TARGET" in
  all)
    install_python_runtime "$PLATFORM_KEY" "$UV_ASSET"
    install_node_runtime "$PLATFORM_KEY" "$NODE_ASSET_TEMPLATE" "$NODE_EXTRACT_TEMPLATE" >/dev/null
    install_pnpm_runtime "$PLATFORM_KEY" "$NODE_ASSET_TEMPLATE" "$NODE_EXTRACT_TEMPLATE"
    ;;
  python)
    install_python_runtime "$PLATFORM_KEY" "$UV_ASSET"
    ;;
  node)
    install_node_runtime "$PLATFORM_KEY" "$NODE_ASSET_TEMPLATE" "$NODE_EXTRACT_TEMPLATE" >/dev/null
    install_pnpm_runtime "$PLATFORM_KEY" "$NODE_ASSET_TEMPLATE" "$NODE_EXTRACT_TEMPLATE"
    ;;
  package-managers)
    install_uv_runtime "$PLATFORM_KEY" "$UV_ASSET" >/dev/null
    install_pnpm_runtime "$PLATFORM_KEY" "$NODE_ASSET_TEMPLATE" "$NODE_EXTRACT_TEMPLATE"
    ;;
  *)
    echo "Unsupported target: $TARGET" >&2
    exit 1
    ;;
esac
