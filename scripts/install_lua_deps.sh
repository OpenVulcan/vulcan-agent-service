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

ensure_dir() { mkdir -p "$1"; }

# ============================================================
# Local tool paths (populated by detect_ functions)
# ============================================================
declare -A LOCAL_TOOLS

# Helper: prepend a directory to our local tool PATH
add_local_tool() {
    if [ -d "$1" ]; then
        LOCAL_TOOLS["$1"]=1
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
if [ ${#LOCAL_TOOLS[@]} -eq 0 ]; then
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

declare -A DEP_URLS DEP_METHODS
PACKAGES=()
CURRENT_PKG=""

while IFS= read -r line; do
    line=$(echo "$line" | sed 's/^[[:space:]]*//' | sed 's/[[:space:]]*$//')
    [[ -z "$line" || "$line" =~ ^# ]] && continue

    if [[ "$line" =~ ^pkg[[:space:]]+([^[:space:]]+) ]]; then
        CURRENT_PKG="${BASH_REMATCH[1]}"
        PACKAGES+=("$CURRENT_PKG")
    elif [[ "$line" =~ ^dep[[:space:]]+([^[:space:]]+)[[:space:]]+([^[:space:]]+)[[:space:]]+([^[:space:]]+)[[:space:]]+([^[:space:]]+) ]]; then
        dep_name="${BASH_REMATCH[1]}"
        dep_os="${BASH_REMATCH[2]}"
        dep_method="${BASH_REMATCH[3]}"
        dep_url="${BASH_REMATCH[4]}"
        if [ "$dep_os" = "linux" ] || [ "$dep_os" = "any" ] || [ "$dep_os" = "macos" ]; then
            DEP_URLS["$dep_name"]="$dep_url"
            DEP_METHODS["$dep_name"]="$dep_method"
        fi
    fi
done < "$PACKAGES_FILE"

echo ""
echo "==> Packages from $PACKAGES_FILE:"
for pkg in "${PACKAGES[@]}"; do
    dep_list=""
    for dep_name in "${!DEP_URLS[@]}"; do
        for p in "${PACKAGES[@]}"; do
            if [ "$p" = "$pkg" ]; then
                dep_list="$dep_list $dep_name"
            fi
        done
    done
    if [ -n "$dep_list" ]; then
        echo "  - $pkg [deps:$dep_list]"
    else
        echo "  - $pkg [pure lua]"
    fi
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

download_prebuilt_deps() {
    local platform
    case "$(uname -s)" in
        Linux*)   platform="linux-x64" ;;
        Darwin*)  platform="macos-x64" ;;
        *)        echo "  ==> Unsupported platform."; return 1 ;;
    esac

    local asset_name="lua-deps-${platform}.tar.gz"
    local marker="$DEPS_DIR/.prebuilt-${asset_name}.installed"

    [ -f "$marker" ] && { echo "  ==> Pre-built deps already installed ($asset_name)."; return 0; }

    echo "  ==> Checking GitHub Releases for pre-built deps ($asset_name)..."

    local api_url="https://api.github.com/repos/${GITHUB_REPO}/releases/tags/${RELEASE_TAG}"
    local release_data
    release_data=$(curl -fSL -s "$api_url" 2>/dev/null) || {
        echo "  ==> Release not found. Will compile locally."
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
    local archive="$DEPS_DIR/prebuilt.tar.gz"
    curl -fSL "$download_url" -o "$archive" 2>/dev/null || { echo "  ==> Download failed."; return 1; }
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
for ext in dylib a; do
    f=$(find "$LUAJIT_DIR" -maxdepth 1 -name "libluajit-5.1.$ext" 2>/dev/null | head -1)
    [ -n "$f" ] && LUAJIT_DYLIB="$f" && break
done
LUA_INCLUDE="$LUAJIT_DIR/include"

if { [ -n "$LUAJIT_SO" ] && [ -f "$LUAJIT_SO" ]; } || { [ -n "$LUAJIT_DYLIB" ] && [ -f "$LUAJIT_DYLIB" ]; } || [ -f "$LUAJIT_BIN" ]; then
    if [ -d "$LUA_INCLUDE" ]; then
        echo "==> LuaJIT SDK already exists at $LUAJIT_DIR (reusing)"
    fi
fi

if ! { [ -f "$LUAJIT_SO" ] || [ -f "$LUAJIT_DYLIB" ] || [ -f "$LUAJIT_BIN" ]; } || [ ! -d "$LUA_INCLUDE" ]; then
    echo "==> Searching cargo target for LuaJIT build output..."
    BUILD_SRC=""
    while IFS= read -r build_dir; do
        src="$build_dir/src"
        [ -f "$src/lua.h" ] && { BUILD_SRC="$src"; break; }
    done < <(find "$PROJECT_DIR/target" -path "*/mlua-sys*/out/luajit-build" -type d 2>/dev/null | sort -r)

    if [ -z "$BUILD_SRC" ]; then
        echo "==> LuaJIT build output not found. Running cargo build..."
        cargo build
        while IFS= read -r build_dir; do
            src="$build_dir/src"
            [ -f "$src/lua.h" ] && { BUILD_SRC="$src"; break; }
        done < <(find "$PROJECT_DIR/target" -path "*/mlua-sys*/out/luajit-build" -type d 2>/dev/null | sort -r)
    fi

    [ -z "$BUILD_SRC" ] && { echo "ERROR: LuaJIT build artifacts not found." >&2; exit 1; }

    ensure_dir "$LUAJIT_DIR"
    ensure_dir "$LUA_INCLUDE"

    if [ ! -f "$BUILD_SRC/libluajit-5.1.so" ] && [ ! -f "$BUILD_SRC/libluajit-5.1.a" ]; then
        echo "==> Building LuaJIT..."
        make -C "$BUILD_SRC" -j"$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 2)"
    fi

    cp "$BUILD_SRC"/libluajit-5.1.so* "$LUAJIT_DIR/" 2>/dev/null || true
    cp "$BUILD_SRC"/libluajit-5.1.a* "$LUAJIT_DIR/" 2>/dev/null || true
    cp "$BUILD_SRC"/luajit "$LUAJIT_DIR/" 2>/dev/null || true
    [ -f "$BUILD_SRC/lua.h" ] && cp "$BUILD_SRC"/*.h "$LUA_INCLUDE/" 2>/dev/null || true

    echo "==> LuaJIT SDK ready at $LUAJIT_DIR"
fi

if [ -f "$LUAJIT_DIR/luajit" ]; then
    LUAJIT_CMD="$LUAJIT_DIR/luajit"
else
    # Try to find the binary
    LUAJIT_CMD=$(find "$LUAJIT_DIR" -maxdepth 1 -name "luajit*" -type f | head -1)
    [ -z "$LUAJIT_CMD" ] && { echo "ERROR: luajit binary not found at $LUAJIT_DIR" >&2; exit 1; }
fi
echo "==> Using LuaJIT: $LUAJIT_CMD"

# ============================================================
# Step 2: Install luarocks
# ============================================================
echo ""
echo "=== Step 2: luarocks ==="

LUAROCKS_BIN=""
[ -f "$LUAROCKS_DIR/luarocks" ] && LUAROCKS_BIN="$LUAROCKS_DIR/luarocks"

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
    ./configure --lua-dir="$LUAJIT_DIR" --lua-version=5.1 \
        --with-lua-include="$LUA_INCLUDE" --prefix="$LUAROCKS_DIR" \
        --rocks-tree="$LUA_PACKAGES"
    make build && make install
    cd "$PROJECT_DIR"
    rm -rf "$BUILD_TEMP"
    LUAROCKS_BIN="$LUAROCKS_DIR/luarocks"
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

declare -A DEP_INSTALLS BUILT_DEPS

# Priority 1: Pre-built from GitHub Releases
PREBUILT_OK=false
if [ "$GITHUB_REPO" != "{{GITHUB_USER}}/{{GITHUB_REPO}}" ]; then
    if download_prebuilt_deps; then
        for dep_name in openssl zlib pcre2 libyaml; do
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
for dep_name in openssl zlib pcre2 libyaml; do
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
    extra_args=""

    for dep_name in "${!DEP_INSTALLS[@]}"; do
        d="${DEP_INSTALLS[$dep_name]}"
        if [ -n "$d" ]; then
            extra_args="$extra_args --with-${dep_name}-libdir=$d/lib --with-${dep_name}-incdir=$d/include"
        fi
    done

    if $LUAROCKS_BIN install "$pkg" --tree="$LUA_PACKAGES" --lua-dir="$LUAJIT_DIR" $extra_args; then
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
