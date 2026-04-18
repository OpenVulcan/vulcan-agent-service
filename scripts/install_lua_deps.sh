#!/usr/bin/env bash
# install_lua_deps.sh — Install Lua C modules via luarocks into third_party/lua_packages/
# Developer/build use only. End users do not need luarocks.
# Reuses LuaJIT source from cargo target — no network download needed.
# Reads packages AND C dependencies from scripts/lua_packages.txt.
# All build tools are detected/installed to third_party/tools/ — system is NOT modified.
# Usage: bash scripts/install_lua_deps.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

THIRD_PARTY="$PROJECT_DIR/third_party"
TOOLS_DIR="$THIRD_PARTY/tools"
LUAJIT_DIR="$THIRD_PARTY/luajit"
LUA_PACKAGES="$THIRD_PARTY/lua_packages"
LUAROCKS_DIR="$THIRD_PARTY/luarocks"
DEPS_DIR="$THIRD_PARTY/deps"
LIST_SEP=$'\036'

ensure_dir() { mkdir -p "$1"; }

get_current_platform() {
    # 将当前系统规范化为配置文件使用的 os 键。
    # Normalize the current system to the os key used by the config file.
    case "$(uname -s)" in
        Darwin) echo "macos" ;;
        *)      echo "linux" ;;
    esac
}

config_os_matches() {
    # 判断配置行是否适用于当前平台。
    # Check whether a config line applies to the current platform.
    local config_os="$1"
    local current_platform
    current_platform="$(get_current_platform)"
    [[ "$config_os" = "any" || "$config_os" = "$current_platform" ]]
}

append_assoc_list() {
    # 以分隔符追加关联数组中的列表值。
    # Append a list item to an associative-array entry using a stable separator.
    local array_name="$1" key="$2" value="$3"
    local -n arr="$array_name"
    if [ -n "${arr[$key]:-}" ]; then
        arr["$key"]+="$LIST_SEP$value"
    else
        arr["$key"]="$value"
    fi
}

resolve_config_ref() {
    # 解析配置引用：
    # 1. dep:<name>[/subpath]
    # 2. tool:<subpath>
    # 3. path:<subpath>
    # 其他值按字面量返回。
    # Resolve config references:
    # 1. dep:<name>[/subpath]
    # 2. tool:<subpath>
    # 3. path:<subpath>
    # Any other value is returned literally.
    local ref="$1"
    if [[ "$ref" =~ ^dep:([^/]+)(/(.+))?$ ]]; then
        local dep_name="${BASH_REMATCH[1]}"
        local relative_path="${BASH_REMATCH[3]:-}"
        local dep_root="${DEP_INSTALLS[$dep_name]:-}"
        if [ -z "$dep_root" ]; then
            echo "ERROR: dependency path not resolved for $ref" >&2
            return 1
        fi
        if [ -n "$relative_path" ]; then
            echo "$dep_root/$relative_path"
        else
            echo "$dep_root"
        fi
        return 0
    fi
    if [[ "$ref" =~ ^tool:(.+)$ ]]; then
        echo "$TOOLS_DIR/${BASH_REMATCH[1]}"
        return 0
    fi
    if [[ "$ref" =~ ^path:(.+)$ ]]; then
        echo "$PROJECT_DIR/${BASH_REMATCH[1]}"
        return 0
    fi
    echo "$ref"
}

# ============================================================
# Local tool paths (populated by detect_ functions)
# ============================================================
declare -A LOCAL_TOOLS
LOCAL_TOOL_COUNT=0

# Helper: prepend a directory to our local tool PATH
add_local_tool() {
    if [ -d "$1" ]; then
        if [ -z "${LOCAL_TOOLS["$1"]+x}" ]; then
            LOCAL_TOOLS["$1"]=1
            LOCAL_TOOL_COUNT=$((LOCAL_TOOL_COUNT + 1))
        fi
        export PATH="$1:$PATH"
    fi
}

# ============================================================
# Dependency detection & local install
# ============================================================

detect_tool() {
    local name="$1" desc="$2"
    shift 2
    # Remaining args: check_cmd install_cmd
    # check_cmd should return 0 if found
    local check_cmd="$1"
    local install_cmd="${2:-}"

    if eval "$check_cmd" 2>/dev/null; then
        echo "  [OK] $desc"
        return 0
    fi

    echo "  [MISSING] $desc"

    if [ -n "$install_cmd" ]; then
        echo "    Installing to third_party/tools/..."
        if eval "$install_cmd"; then
            echo "  [OK] $desc (project-local)"
            return 0
        fi
    fi

    echo "  [FAIL] $desc — please install manually"
    return 1
}

# --- perl ---
check_perl() { command -v perl >/dev/null 2>&1; }
install_perl() {
    # perl is pre-installed on virtually all Unix-like systems
    # On minimal containers, try package managers
    if command -v apt-get &>/dev/null; then
        apt-get update -qq && apt-get install -y -qq perl 2>/dev/null
    elif command -v dnf &>/dev/null; then
        dnf install -y -q perl 2>/dev/null
    elif command -v yum &>/dev/null; then
        yum install -y -q perl 2>/dev/null
    elif command -v pacman &>/dev/null; then
        pacman -S --noconfirm --quiet perl 2>/dev/null
    elif command -v brew &>/dev/null; then
        brew install perl 2>/dev/null
    else
        return 1
    fi
    command -v perl >/dev/null 2>&1
}

# --- cmake ---
check_cmake() { command -v cmake >/dev/null 2>&1; }
install_cmake() {
    local cmake_dir="$TOOLS_DIR/cmake"
    ensure_dir "$cmake_dir"

    local version="3.31.6"
    local arch="x86_64"
    local os_name
    os_name=$(uname -s)

    local tar_name archive_url
    if [[ "$os_name" == "Linux" ]]; then
        local machine
        machine=$(uname -m)
        [[ "$machine" == "aarch64" ]] && arch="aarch64"
        tar_name="cmake-${version}-linux-${arch}"
        archive_url="https://github.com/Kitware/CMake/releases/download/v${version}/${tar_name}.tar.gz"
    elif [[ "$os_name" == "Darwin" ]]; then
        tar_name="cmake-${version}-macos-universal"
        archive_url="https://github.com/Kitware/CMake/releases/download/v${version}/${tar_name}.tar.gz"
    else
        return 1
    fi

    local archive="$cmake_dir/cmake.tar.gz"
    if [ ! -f "$archive" ]; then
        curl -fSL "$archive_url" -o "$archive"
    fi
    tar -xzf "$archive" -C "$cmake_dir"
    rm -f "$archive"

    local cmake_bin="$cmake_dir/${tar_name}/bin"
    if [ -f "$cmake_bin/cmake" ]; then
        add_local_tool "$cmake_bin"
        return 0
    fi
    return 1
}

# --- make ---
check_make() { command -v make >/dev/null 2>&1 || command -v gmake >/dev/null 2>&1; }
install_make() {
    if command -v apt-get &>/dev/null; then
        apt-get update -qq && apt-get install -y -qq make 2>/dev/null
    elif command -v dnf &>/dev/null; then
        dnf install -y -q make 2>/dev/null
    elif command -v yum &>/dev/null; then
        yum install -y -q make 2>/dev/null
    elif command -v pacman &>/dev/null; then
        pacman -S --noconfirm --quiet make 2>/dev/null
    elif command -v brew &>/dev/null; then
        brew install make 2>/dev/null
    else
        return 1
    fi
    check_make
}

# --- curl ---
check_curl() { command -v curl >/dev/null 2>&1; }
install_curl() {
    if command -v apt-get &>/dev/null; then
        apt-get update -qq && apt-get install -y -qq curl 2>/dev/null
    elif command -v dnf &>/dev/null; then
        dnf install -y -q curl 2>/dev/null
    elif command -v yum &>/dev/null; then
        yum install -y -q curl 2>/dev/null
    elif command -v pacman &>/dev/null; then
        pacman -S --noconfirm --quiet curl 2>/dev/null
    elif command -v brew &>/dev/null; then
        brew install curl 2>/dev/null
    else
        return 1
    fi
    command -v curl >/dev/null 2>&1
}

# ============================================================
# Step 0: Detect and install build tools
# ============================================================
echo ""
echo "=== Step 0: Detect Build Tools ==="

TOOLS_OK=true

detect_tool "perl" "perl" "check_perl" "install_perl" || TOOLS_OK=false
detect_tool "curl" "curl" "check_curl" "install_curl" || TOOLS_OK=false
detect_tool "make" "make/gmake" "check_make" "install_make" || TOOLS_OK=false
detect_tool "cmake" "cmake (for zlib/pcre2/libyaml builds)" "check_cmake" "install_cmake" || true
# cmake is optional — only needed for cmake-based deps

if [ "$TOOLS_OK" = false ]; then
    echo ""
    echo "ERROR: Required build tools are not available. Please install them and re-run."
    exit 1
fi

echo ""
echo "  Active local tool dirs:"
for dir in "${!LOCAL_TOOLS[@]}"; do
    echo "    - $dir"
done
if [ "$LOCAL_TOOL_COUNT" -eq 0 ]; then
    echo "    (all tools found in system PATH)"
fi

# ============================================================
# Parse lua_packages.txt
# ============================================================
PACKAGES_FILE="$SCRIPT_DIR/lua_packages.txt"
if [ ! -f "$PACKAGES_FILE" ]; then
    echo "ERROR: $PACKAGES_FILE not found" >&2
    exit 1
fi

declare -A DEP_URLS DEP_METHODS PKG_INSTALL_TARGETS PKG_ARGS PKG_ENV_REFS PKG_DEPVAR_REFS PKG_DEPS
PACKAGES=()
CURRENT_PKG=""

while IFS= read -r line; do
    line=$(echo "$line" | sed 's/^[[:space:]]*//' | sed 's/[[:space:]]*$//')
    [[ -z "$line" || "$line" =~ ^# ]] && continue

    if [[ "$line" =~ ^pkg[[:space:]]+([^[:space:]]+) ]]; then
        CURRENT_PKG="${BASH_REMATCH[1]}"
        PACKAGES+=("$CURRENT_PKG")
        PKG_INSTALL_TARGETS["$CURRENT_PKG"]="$CURRENT_PKG"
    elif [[ "$line" =~ ^install[[:space:]]+([^[:space:]]+)[[:space:]]+(.+)$ ]]; then
        config_os="${BASH_REMATCH[1]}"
        target_ref="${BASH_REMATCH[2]}"
        if [ -n "$CURRENT_PKG" ] && config_os_matches "$config_os"; then
            PKG_INSTALL_TARGETS["$CURRENT_PKG"]="$target_ref"
        fi
    elif [[ "$line" =~ ^arg[[:space:]]+([^[:space:]]+)[[:space:]]+(.+)$ ]]; then
        config_os="${BASH_REMATCH[1]}"
        arg_value="${BASH_REMATCH[2]}"
        if [ -n "$CURRENT_PKG" ] && config_os_matches "$config_os"; then
            append_assoc_list PKG_ARGS "$CURRENT_PKG" "$arg_value"
        fi
    elif [[ "$line" =~ ^env[[:space:]]+([^[:space:]]+)[[:space:]]+([^[:space:]]+)[[:space:]]+(.+)$ ]]; then
        config_os="${BASH_REMATCH[1]}"
        env_name="${BASH_REMATCH[2]}"
        env_value="${BASH_REMATCH[3]}"
        if [ -n "$CURRENT_PKG" ] && config_os_matches "$config_os"; then
            PKG_ENV_REFS["$CURRENT_PKG|$env_name"]="$env_value"
        fi
    elif [[ "$line" =~ ^dep[[:space:]]+([^[:space:]]+)[[:space:]]+([^[:space:]]+)[[:space:]]+([^[:space:]]+)[[:space:]]+([^[:space:]]+) ]]; then
        dep_name="${BASH_REMATCH[1]}"
        dep_os="${BASH_REMATCH[2]}"
        dep_method="${BASH_REMATCH[3]}"
        dep_url="${BASH_REMATCH[4]}"
        if [ -n "$CURRENT_PKG" ] && config_os_matches "$dep_os"; then
            DEP_URLS["$dep_name"]="$dep_url"
            DEP_METHODS["$dep_name"]="$dep_method"
            append_assoc_list PKG_DEPS "$CURRENT_PKG" "$dep_name"
        fi
    elif [[ "$line" =~ ^depvar[[:space:]]+([^[:space:]]+)[[:space:]]+([^[:space:]]+)[[:space:]]+([^[:space:]]+)[[:space:]]+(.+)$ ]]; then
        dep_name="${BASH_REMATCH[1]}"
        dep_os="${BASH_REMATCH[2]}"
        var_name="${BASH_REMATCH[3]}"
        value_ref="${BASH_REMATCH[4]}"
        if [ -n "$CURRENT_PKG" ] && config_os_matches "$dep_os"; then
            PKG_DEPVAR_REFS["$CURRENT_PKG|$dep_name|$var_name"]="$value_ref"
        fi
    fi
done < "$PACKAGES_FILE"

echo ""
echo "==> Packages from $PACKAGES_FILE:"
for pkg in "${PACKAGES[@]}"; do
    dep_list="${PKG_DEPS[$pkg]:-}"
    if [ -n "$dep_list" ]; then
        dep_display="${dep_list//$LIST_SEP/, }"
        dep_segment=" [deps: $dep_display]"
    else
        dep_segment=" [pure lua]"
    fi
    target_display="${PKG_INSTALL_TARGETS[$pkg]:-$pkg}"
    if [ "$target_display" != "$pkg" ]; then
        target_segment=" [target: $target_display]"
    else
        target_segment=""
    fi
    echo "  - $pkg$dep_segment$target_segment"
done

declare -A REQUIRED_DEP_SET
REQUIRED_DEPS=()
for pkg in "${PACKAGES[@]}"; do
    dep_list="${PKG_DEPS[$pkg]:-}"
    [ -z "$dep_list" ] && continue
    IFS="$LIST_SEP" read -r -a pkg_dep_items <<< "$dep_list"
    for dep_name in "${pkg_dep_items[@]}"; do
        [ -z "$dep_name" ] && continue
        if [ -z "${REQUIRED_DEP_SET[$dep_name]:-}" ]; then
            REQUIRED_DEP_SET["$dep_name"]=1
            REQUIRED_DEPS+=( "$dep_name" )
        fi
    done
done

# ============================================================
# Helper: download and extract tar.gz
# ============================================================
download_extract() {
    local url="$1" dest="$2"
    local archive="$dest/source.tar.gz"
    [ -f "$archive" ] || curl -fSL "$url" -o "$archive"
    tar -xzf "$archive" -C "$dest"
    rm -f "$archive"
    find "$dest" -maxdepth 1 -type d ! -name "$(basename "$dest")" | head -1
}

# ============================================================
# Pre-built C deps from GitHub Releases
# ============================================================

GITHUB_REPO="OpenVulcan/vulcan-mcp"
RELEASE_TAG="deps-v1"

find_local_archive() {
    # 在 third_party 顶层及其直接子目录中查找匹配的本地压缩包。
    # Find a matching local archive under third_party and its direct child directories.
    local asset_name="$1"
    find "$THIRD_PARTY" -maxdepth 2 -type f -name "$asset_name" | sort -r | head -1
}

get_prebuilt_deps_platform() {
    # 根据当前操作系统与架构，推导 lua-deps 预编译资产名后缀。
    # Derive the lua-deps prebuilt asset suffix for the current OS and architecture.
    local machine
    machine="$(uname -m)"

    case "$(uname -s)" in
        Linux*)
            case "$machine" in
                aarch64|arm64) echo "linux-arm64" ;;
                x86_64|amd64) echo "linux-x64" ;;
                *) echo "unsupported" ;;
            esac
            ;;
        Darwin*)
            case "$machine" in
                aarch64|arm64) echo "macos-arm64" ;;
                x86_64|amd64) echo "macos-x64" ;;
                *) echo "unsupported" ;;
            esac
            ;;
        *)
            echo "unsupported"
            ;;
    esac
}

download_prebuilt_deps() {
    local platform
    platform="$(get_prebuilt_deps_platform)"
    if [ "$platform" = "unsupported" ]; then
        echo "  ==> Unsupported platform."
        return 1
    fi

    local asset_name="lua-deps-${platform}.tar.gz"
    local marker="$DEPS_DIR/.prebuilt-${asset_name}.installed"
    local local_archive=""

    [ -f "$marker" ] && { echo "  ==> Pre-built deps already installed ($asset_name)."; return 0; }

    local archive="$DEPS_DIR/prebuilt.tar.gz"
    local_archive="$(find_local_archive "$asset_name")"
    if [ -n "$local_archive" ]; then
        echo "  ==> Using local pre-built deps package: $local_archive"
        cp "$local_archive" "$archive"
    else
        echo "  ==> Checking GitHub Releases for pre-built deps ($asset_name)..."

        local api_url="https://api.github.com/repos/${GITHUB_REPO}/releases/tags/${RELEASE_TAG}"
        local release_data
        release_data=$(curl -fSL -s "$api_url" 2>/dev/null) || {
            echo "  ==> Release '$RELEASE_TAG' not reachable. It may be missing or the repository may still be private. Will compile locally."
            return 1
        }

        local download_url
        download_url=$(echo "$release_data" | python3 -c "
import sys, json
data = json.load(sys.stdin)
for a in data.get('assets', []):
    if a['name'] == '${asset_name}':
        print(a['browser_download_url'])
        sys.exit(0)
" 2>/dev/null) || {
            echo "  ==> Could not parse release data or asset not found."
            return 1
        }

        if [ -z "$download_url" ]; then
            echo "  ==> Pre-built asset not found in release."
            return 1
        fi

        echo "  ==> Downloading pre-built deps..."
        curl -fSL "$download_url" -o "$archive" 2>/dev/null || { echo "  ==> Download failed."; return 1; }
    fi
    tar -xzf "$archive" -C "$DEPS_DIR"
    rm -f "$archive"
    touch "$marker"
    echo "  ==> Pre-built deps installed successfully."
    return 0
}

# ============================================================
# Build dependency functions
# ============================================================
build_openssl() {
    local url="$1" build_dir="$2"
    local install_dir="$DEPS_DIR/openssl"
    [ -f "$install_dir/lib/libssl.a" ] && { echo "$install_dir"; return 0; }
    echo "  ==> Downloading OpenSSL..."
    ensure_dir "$build_dir"
    local src_dir; src_dir=$(download_extract "$url" "$build_dir")
    echo "  ==> Building OpenSSL..."
    pushd "$src_dir" >/dev/null
    ./config --prefix="$install_dir" --openssldir="$install_dir/ssl" no-tests no-shared
    make -j"$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 2)"
    make install_sw
    popd >/dev/null
    echo "$install_dir"
}

build_zlib() {
    local url="$1" build_dir="$2"
    local install_dir="$DEPS_DIR/zlib"
    [ -f "$install_dir/lib/libz.a" ] && { echo "$install_dir"; return 0; }
    echo "  ==> Downloading Zlib..."
    ensure_dir "$build_dir"
    local src_dir; src_dir=$(download_extract "$url" "$build_dir")
    echo "  ==> Building Zlib (cmake + make)..."
    pushd "$src_dir" >/dev/null
    local build_sub="$src_dir/build"
    ensure_dir "$build_sub"
    pushd "$build_sub" >/dev/null
    cmake .. -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$install_dir" -DBUILD_SHARED_LIBS=ON
    make -j"$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 2)"
    make install
    popd >/dev/null
    popd >/dev/null
    echo "$install_dir"
}

build_pcre2() {
    local url="$1" build_dir="$2"
    local install_dir="$DEPS_DIR/pcre2"
    [ -f "$install_dir/lib/libpcre2-8.a" ] && { echo "$install_dir"; return 0; }
    echo "  ==> Downloading PCRE2..."
    ensure_dir "$build_dir"
    local src_dir; src_dir=$(download_extract "$url" "$build_dir")
    echo "  ==> Building PCRE2 (cmake + make)..."
    pushd "$src_dir" >/dev/null
    local build_sub="$src_dir/build"
    ensure_dir "$build_sub"
    pushd "$build_sub" >/dev/null
    cmake .. -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$install_dir" \
        -DBUILD_SHARED_LIBS=OFF -DPCRE2_BUILD_PCRE2GREP=OFF -DPCRE2_SUPPORT_JIT=ON
    make -j"$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 2)"
    make install
    popd >/dev/null
    popd >/dev/null
    echo "$install_dir"
}

build_libyaml() {
    local url="$1" build_dir="$2"
    local install_dir="$DEPS_DIR/libyaml"
    [ -f "$install_dir/lib/libyaml.a" ] && { echo "$install_dir"; return 0; }
    echo "  ==> Downloading LibYAML..."
    ensure_dir "$build_dir"
    local src_dir; src_dir=$(download_extract "$url" "$build_dir")
    echo "  ==> Building LibYAML (cmake + make)..."
    pushd "$src_dir" >/dev/null
    local build_sub="$src_dir/build"
    ensure_dir "$build_sub"
    pushd "$build_sub" >/dev/null
    cmake .. -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$install_dir" -DBUILD_SHARED_LIBS=OFF
    make -j"$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 2)"
    make install
    popd >/dev/null
    popd >/dev/null
    echo "$install_dir"
}

# ============================================================
# Step 1: Build LuaJIT SDK from cargo target
# ============================================================
echo ""
echo "=== Step 1: LuaJIT SDK ==="

LUAJIT_BIN="$LUAJIT_DIR/luajit"
LUAJIT_SO="$LUAJIT_DIR/libluajit-5.1.so"
LUAJIT_DYLIB=""
if [ -d "$LUAJIT_DIR" ]; then
    for ext in dylib a; do
        f=$(find "$LUAJIT_DIR" -maxdepth 1 -name "libluajit-5.1.$ext" 2>/dev/null | head -1)
        [ -n "$f" ] && LUAJIT_DYLIB="$f" && break
    done
fi
LUA_INCLUDE="$LUAJIT_DIR/include"

if { [ -n "$LUAJIT_SO" ] && [ -f "$LUAJIT_SO" ]; } || { [ -n "$LUAJIT_DYLIB" ] && [ -f "$LUAJIT_DYLIB" ]; } || [ -f "$LUAJIT_BIN" ]; then
    if [ -d "$LUA_INCLUDE" ]; then
        echo "==> LuaJIT SDK already exists at $LUAJIT_DIR (reusing)"
    fi
fi

if ! { [ -f "$LUAJIT_SO" ] || [ -f "$LUAJIT_DYLIB" ] || [ -f "$LUAJIT_BIN" ]; } || [ ! -d "$LUA_INCLUDE" ]; then
    echo "==> Searching cargo target for LuaJIT build output..."
    select_luajit_build_out() {
        # Prefer candidates that already contain built artifacts, then sort by artifact timestamp before directory freshness.
        # 优先选择已经带构建产物的候选目录，再按产物时间排序，最后才回退到目录时间。
        python3 - "$PROJECT_DIR" <<'PY'
import os
import sys

project_dir = sys.argv[1]
target_dir = os.path.join(project_dir, "target")
lib_names = [
    "libluajit-5.1.so",
    "libluajit-5.1.a",
    "libluajit-5.1.dylib",
    "libluajit.so",
    "libluajit.a",
    "libluajit.dylib",
    "lua51.dll",
]
candidates = []

for root, dirs, files in os.walk(target_dir):
    if os.path.basename(root) != "out" or "mlua-sys" not in root:
        continue
    src = os.path.join(root, "luajit-build", "src")
    include = os.path.join(root, "include")
    lib = os.path.join(root, "lib")
    has_header = os.path.isfile(os.path.join(include, "lua.h")) or os.path.isfile(os.path.join(src, "lua.h"))
    if not has_header:
        continue

    artifact_paths = []
    for base in (lib, src):
        for name in lib_names:
            candidate = os.path.join(base, name)
            if os.path.isfile(candidate):
                artifact_paths.append(candidate)

    has_artifact = bool(artifact_paths)
    artifact_time = max((os.path.getmtime(path) for path in artifact_paths), default=0.0)
    dir_time = os.path.getmtime(root)
    candidates.append((1 if has_artifact else 0, artifact_time, dir_time, root))

if candidates:
    candidates.sort(key=lambda item: (item[0], item[1], item[2]), reverse=True)
    print(candidates[0][3])
PY
    }

    BUILD_OUT="$(select_luajit_build_out)"
    BUILD_SRC=""
    BUILD_LIB=""
    BUILD_INCLUDE=""
    if [ -n "$BUILD_OUT" ]; then
        BUILD_SRC="$BUILD_OUT/luajit-build/src"
        BUILD_LIB="$BUILD_OUT/lib"
        BUILD_INCLUDE="$BUILD_OUT/include"
    fi

    if [ -z "$BUILD_OUT" ]; then
        echo "==> LuaJIT build output not found. Running cargo build..."
        cargo build
        BUILD_OUT="$(select_luajit_build_out)"
        if [ -n "$BUILD_OUT" ]; then
            BUILD_SRC="$BUILD_OUT/luajit-build/src"
            BUILD_LIB="$BUILD_OUT/lib"
            BUILD_INCLUDE="$BUILD_OUT/include"
        fi
    fi

    [ -z "$BUILD_OUT" ] && { echo "ERROR: LuaJIT build artifacts not found." >&2; exit 1; }

    ensure_dir "$LUAJIT_DIR"
    ensure_dir "$LUA_INCLUDE"

    HAVE_LIB=false
    if [ -n "$BUILD_LIB" ] && {
        [ -f "$BUILD_LIB/libluajit-5.1.so" ] || [ -f "$BUILD_LIB/libluajit-5.1.a" ] || [ -f "$BUILD_LIB/libluajit-5.1.dylib" ] ||
        [ -f "$BUILD_LIB/libluajit.so" ] || [ -f "$BUILD_LIB/libluajit.a" ] || [ -f "$BUILD_LIB/libluajit.dylib" ];
    }; then
        HAVE_LIB=true
    elif [ -n "$BUILD_SRC" ] && {
        [ -f "$BUILD_SRC/libluajit-5.1.so" ] || [ -f "$BUILD_SRC/libluajit-5.1.a" ] || [ -f "$BUILD_SRC/libluajit-5.1.dylib" ] ||
        [ -f "$BUILD_SRC/libluajit.so" ] || [ -f "$BUILD_SRC/libluajit.a" ] || [ -f "$BUILD_SRC/libluajit.dylib" ];
    }; then
        HAVE_LIB=true
    fi

    HAVE_INCLUDE=false
    if [ -n "$BUILD_INCLUDE" ] && [ -f "$BUILD_INCLUDE/lua.h" ]; then
        HAVE_INCLUDE=true
    elif [ -n "$BUILD_SRC" ] && [ -f "$BUILD_SRC/lua.h" ]; then
        HAVE_INCLUDE=true
    fi

    HAVE_BIN=false
    if [ -n "$BUILD_SRC" ] && [ -f "$BUILD_SRC/luajit" ]; then
        HAVE_BIN=true
    fi

    if [ "$HAVE_LIB" = false ] && [ "$HAVE_INCLUDE" = true ] && [ -n "$BUILD_SRC" ]; then
        echo "==> Building LuaJIT..."
        make -C "$BUILD_SRC" -j"$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 2)"
        if [ -f "$BUILD_SRC/libluajit-5.1.so" ] || [ -f "$BUILD_SRC/libluajit-5.1.a" ] || [ -f "$BUILD_SRC/libluajit-5.1.dylib" ] ||
           [ -f "$BUILD_SRC/libluajit.so" ] || [ -f "$BUILD_SRC/libluajit.a" ] || [ -f "$BUILD_SRC/libluajit.dylib" ]; then
            HAVE_LIB=true
        fi
        if [ -f "$BUILD_SRC/luajit" ]; then
            HAVE_BIN=true
        fi
    fi

    [ -n "$BUILD_LIB" ] && cp "$BUILD_LIB"/libluajit-5.1.so* "$LUAJIT_DIR/" 2>/dev/null || true
    [ -n "$BUILD_LIB" ] && cp "$BUILD_LIB"/libluajit-5.1.a* "$LUAJIT_DIR/" 2>/dev/null || true
    [ -n "$BUILD_LIB" ] && cp "$BUILD_LIB"/libluajit-5.1.dylib* "$LUAJIT_DIR/" 2>/dev/null || true
    [ -n "$BUILD_LIB" ] && cp "$BUILD_LIB"/libluajit.so* "$LUAJIT_DIR/" 2>/dev/null || true
    [ -n "$BUILD_LIB" ] && cp "$BUILD_LIB"/libluajit.a* "$LUAJIT_DIR/" 2>/dev/null || true
    [ -n "$BUILD_LIB" ] && cp "$BUILD_LIB"/libluajit.dylib* "$LUAJIT_DIR/" 2>/dev/null || true
    [ -n "$BUILD_SRC" ] && cp "$BUILD_SRC"/libluajit-5.1.so* "$LUAJIT_DIR/" 2>/dev/null || true
    [ -n "$BUILD_SRC" ] && cp "$BUILD_SRC"/libluajit-5.1.a* "$LUAJIT_DIR/" 2>/dev/null || true
    [ -n "$BUILD_SRC" ] && cp "$BUILD_SRC"/libluajit-5.1.dylib* "$LUAJIT_DIR/" 2>/dev/null || true
    [ -n "$BUILD_SRC" ] && cp "$BUILD_SRC"/libluajit.so* "$LUAJIT_DIR/" 2>/dev/null || true
    [ -n "$BUILD_SRC" ] && cp "$BUILD_SRC"/libluajit.a* "$LUAJIT_DIR/" 2>/dev/null || true
    [ -n "$BUILD_SRC" ] && cp "$BUILD_SRC"/libluajit.dylib* "$LUAJIT_DIR/" 2>/dev/null || true
    [ -n "$BUILD_SRC" ] && cp "$BUILD_SRC"/luajit "$LUAJIT_DIR/" 2>/dev/null || true
    if [ -n "$BUILD_INCLUDE" ] && [ -f "$BUILD_INCLUDE/lua.h" ]; then
        cp "$BUILD_INCLUDE"/*.h "$LUA_INCLUDE/" 2>/dev/null || true
    elif [ -n "$BUILD_SRC" ] && [ -f "$BUILD_SRC/lua.h" ]; then
        cp "$BUILD_SRC"/*.h "$LUA_INCLUDE/" 2>/dev/null || true
    fi

    if [ -f "$LUAJIT_DIR/libluajit.a" ] && [ ! -f "$LUAJIT_DIR/libluajit-5.1.a" ]; then
        cp "$LUAJIT_DIR/libluajit.a" "$LUAJIT_DIR/libluajit-5.1.a"
    fi
    if [ -f "$LUAJIT_DIR/libluajit.so" ] && [ ! -f "$LUAJIT_DIR/libluajit-5.1.so" ]; then
        cp "$LUAJIT_DIR/libluajit.so" "$LUAJIT_DIR/libluajit-5.1.so"
    fi
    if [ -f "$LUAJIT_DIR/libluajit.dylib" ] && [ ! -f "$LUAJIT_DIR/libluajit-5.1.dylib" ]; then
        cp "$LUAJIT_DIR/libluajit.dylib" "$LUAJIT_DIR/libluajit-5.1.dylib"
    fi

    if ! { [ -f "$LUAJIT_DIR/libluajit-5.1.so" ] || [ -f "$LUAJIT_DIR/libluajit-5.1.a" ] || [ -f "$LUAJIT_DIR/libluajit-5.1.dylib" ] || [ -f "$LUAJIT_DIR/libluajit.so" ] || [ -f "$LUAJIT_DIR/libluajit.a" ] || [ -f "$LUAJIT_DIR/libluajit.dylib" ] || [ -f "$LUAJIT_DIR/luajit" ]; } || [ ! -f "$LUA_INCLUDE/lua.h" ]; then
        echo "ERROR: LuaJIT SDK is still incomplete after collecting cargo build artifacts." >&2
        echo "       out dir: $BUILD_OUT" >&2
        echo "       lib dir: $BUILD_LIB" >&2
        echo "       src dir: $BUILD_SRC" >&2
        echo "       include dir: $BUILD_INCLUDE" >&2
        exit 1
    fi

    echo "==> LuaJIT SDK ready at $LUAJIT_DIR"
fi

if [ -f "$LUAJIT_DIR/luajit" ]; then
    LUAJIT_CMD="$LUAJIT_DIR/luajit"
else
    # Try to find the binary
    LUAJIT_CMD=""
    if [ -d "$LUAJIT_DIR" ]; then
        LUAJIT_CMD=$(find "$LUAJIT_DIR" -maxdepth 1 -name "luajit*" -type f | head -1)
    fi
    [ -z "$LUAJIT_CMD" ] && { echo "ERROR: luajit binary not found at $LUAJIT_DIR" >&2; exit 1; }
fi
echo "==> Using LuaJIT: $LUAJIT_CMD"

# ============================================================
# Step 2: Install luarocks
# ============================================================
echo ""
echo "=== Step 2: luarocks ==="

LUAROCKS_BIN=""
[ -f "$LUAROCKS_DIR/bin/luarocks" ] && LUAROCKS_BIN="$LUAROCKS_DIR/bin/luarocks"

if [ -z "$LUAROCKS_BIN" ]; then
    echo "==> Installing luarocks..."
    BUILD_TEMP="$PROJECT_DIR/target/luarocks_build"
    ensure_dir "$BUILD_TEMP"

    LUAROCKS_VERSION="3.12.1"
    LUAROCKS_URL="https://luarocks.org/releases/luarocks-${LUAROCKS_VERSION}.tar.gz"
    ARCHIVE="$BUILD_TEMP/luarocks.tar.gz"
    [ -f "$ARCHIVE" ] || curl -fSL "$LUAROCKS_URL" -o "$ARCHIVE"

    tar -xzf "$ARCHIVE" -C "$BUILD_TEMP"
    LUAROCKS_SRC=$(find "$BUILD_TEMP" -maxdepth 1 -name "luarocks-*" -type d | head -1)
    [ -z "$LUAROCKS_SRC" ] && { echo "ERROR: luarocks source not found" >&2; exit 1; }

    cd "$LUAROCKS_SRC"
    ./configure --with-lua="$LUAJIT_DIR" --with-lua-bin="$LUAJIT_DIR" \
        --with-lua-include="$LUA_INCLUDE" --with-lua-lib="$LUAJIT_DIR" \
        --lua-version=5.1 --prefix="$LUAROCKS_DIR" \
        --rocks-tree="$LUA_PACKAGES"
    make build && make install
    cd "$PROJECT_DIR"
    rm -rf "$BUILD_TEMP"
    LUAROCKS_BIN="$LUAROCKS_DIR/bin/luarocks"
fi

# Create luarocks config
echo "==> Creating luarocks config..."
ensure_dir "$LUA_PACKAGES"
cat > "$LUAROCKS_DIR/config.lua" << LUAEOF
rocks_trees = {
    { name = [[project]], root = [[${LUA_PACKAGES}]] },
}
lua_interpreter = [[luajit]]
lua_dir = [[${LUAJIT_DIR}]]
variables = {
    LUA_INCDIR = [[${LUA_INCLUDE}]],
    LUA_LIBDIR = [[${LUAJIT_DIR}]],
}
LUAEOF

# ============================================================
# Step 3: C dependencies — pre-built → source compile
# ============================================================
echo ""
echo "=== Step 3: C Dependencies ==="
ensure_dir "$DEPS_DIR"
echo "  ==> Installing host native dependencies via install_host_deps.sh"
bash "$SCRIPT_DIR/install_host_deps.sh"

declare -A DEP_INSTALLS BUILT_DEPS

# Priority 1: Pre-built from GitHub Releases
PREBUILT_OK=false
if [ "$GITHUB_REPO" != "{{GITHUB_USER}}/{{GITHUB_REPO}}" ]; then
    if download_prebuilt_deps; then
        for dep_name in "${REQUIRED_DEPS[@]}"; do
            dep_dir="$DEPS_DIR/$dep_name"
            if [ -d "$dep_dir" ]; then
                DEP_INSTALLS["$dep_name"]="$dep_dir"
                BUILT_DEPS["$dep_name"]=1
                PREBUILT_OK=true
            fi
        done
        if [ "$PREBUILT_OK" = true ]; then
            echo "  ==> Using pre-built deps. No local compilation needed."
        fi
    fi
fi

# Priority 2: Source compile (skip deps already satisfied by pre-built)
for dep_name in "${REQUIRED_DEPS[@]}"; do
    [ "${BUILT_DEPS[$dep_name]:-}" = "1" ] && continue
    method="${DEP_METHODS[$dep_name]:-none}"
    url="${DEP_URLS[$dep_name]:-}"
    build_dir="$DEPS_DIR/build/$dep_name"

    echo "==> Dependency: $dep_name ($method) — compiling from source"
    install_dir=""
    case "$dep_name" in
        openssl)  install_dir=$(build_openssl "$url" "$build_dir") ;;
        zlib)     install_dir=$(build_zlib "$url" "$build_dir") ;;
        pcre2)    install_dir=$(build_pcre2 "$url" "$build_dir") ;;
        libyaml)  install_dir=$(build_libyaml "$url" "$build_dir") ;;
        *)        echo "  ==> Unknown dep: $dep_name, skipping" ;;
    esac
    if [ -n "$install_dir" ]; then
        DEP_INSTALLS["$dep_name"]="$install_dir"
        BUILT_DEPS["$dep_name"]=1
    fi
done

# ============================================================
# Step 4: Install Lua packages
# ============================================================
echo ""
echo "=== Step 4: Installing Lua packages ==="

# Use only project-local tools in PATH for luarocks
export PATH="$LUAJIT_DIR:$PATH"
for dep_name in "${!DEP_INSTALLS[@]}"; do
    d="${DEP_INSTALLS[$dep_name]}"
    [ -n "$d" ] && export PATH="$d/bin:$PATH"
done
# Add cmake to PATH if installed locally
for dir in "${!LOCAL_TOOLS[@]}"; do
    export PATH="$dir:$PATH"
done

FAILED_PKGS=()
OK_PKGS=()

for pkg in "${PACKAGES[@]}"; do
    echo "==> Installing $pkg..."
    install_target="$(resolve_config_ref "${PKG_INSTALL_TARGETS[$pkg]:-$pkg}")"
    install_cmd=( "$LUAROCKS_BIN" install "$install_target" "--no-doc" "--tree=$LUA_PACKAGES" "--lua-dir=$LUAJIT_DIR" )
    cmd_env=()
    dep_args=()

    if [ -n "${PKG_ARGS[$pkg]:-}" ]; then
        IFS="$LIST_SEP" read -r -a pkg_args <<< "${PKG_ARGS[$pkg]}"
        for arg in "${pkg_args[@]}"; do
            [ -n "$arg" ] && install_cmd+=( "$arg" )
        done
    fi

    for env_key in "${!PKG_ENV_REFS[@]}"; do
        if [[ "$env_key" == "$pkg|"* ]]; then
            env_name="${env_key#"$pkg|"}"
            env_value="$(resolve_config_ref "${PKG_ENV_REFS[$env_key]}")"
            cmd_env+=( "$env_name=$env_value" )
        fi
    done

    for depvar_key in "${!PKG_DEPVAR_REFS[@]}"; do
        if [[ "$depvar_key" == "$pkg|"* ]]; then
            rest="${depvar_key#"$pkg|"}"
            dep_name="${rest%%|*}"
            var_name="${rest##*|}"
            if [ -n "${DEP_INSTALLS[$dep_name]:-}" ]; then
                resolved_value="$(resolve_config_ref "${PKG_DEPVAR_REFS[$depvar_key]}")"
                dep_args+=( "$var_name=$resolved_value" )
            fi
        fi
    done

    if env "${cmd_env[@]}" "${install_cmd[@]}" "${dep_args[@]}"; then
        OK_PKGS+=("$pkg")
    else
        FAILED_PKGS+=("$pkg")
        echo "==> WARNING: Failed to install $pkg" >&2
    fi
done

echo ""
echo "==> Install results:"
for pkg in "${OK_PKGS[@]}"; do echo "  $pkg : OK"; done
for pkg in "${FAILED_PKGS[@]}"; do echo "  $pkg : FAILED"; done

echo ""
echo "==> Installed files:"
find "$LUA_PACKAGES" \( -name "*.so" -o -name "*.dll" -o -name "*.dylib" -o -name "*.lua" \) -type f 2>/dev/null | sort | while read -r f; do
    echo "  $f"
done

echo ""
echo "==> Done."
echo "    LuaJIT SDK: $LUAJIT_DIR"
echo "    Deps:       $DEPS_DIR"
echo "    Packages:   $LUA_PACKAGES"
echo "    Tools:      $TOOLS_DIR (project-local)"
