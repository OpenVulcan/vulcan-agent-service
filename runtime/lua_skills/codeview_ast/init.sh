#!/usr/bin/env bash
# init.sh — Download ast-grep binary for current platform
# 中文：初始化 ast-grep 运行时依赖，优先使用宿主注入目录，缺失时回退到脚本目录。
# English: Initialize the ast-grep runtime dependency. Prefer host-provided directories and fall back to the script directory when needed.

set -e

if [ -z "$SKILL_DIR" ]; then
    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
    SKILL_DIR="$SCRIPT_DIR"
fi

if [ -z "$TOOLS_DIR" ]; then
    TOOLS_DIR="$(cd "$(dirname "$SKILL_DIR")" && pwd)/tools"
fi

BIN_DIR="$SKILL_DIR/bin"
mkdir -p "$BIN_DIR"

# Skip if binary already exists
if [ -f "$BIN_DIR/ast-grep" ]; then
    echo "ast-grep already installed: $BIN_DIR/ast-grep"
    exit 0
fi

# Detect OS and architecture
detect_os() {
    case "$(uname -s)" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "macos" ;;
        *)       echo "unknown" ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64)  echo "x86_64" ;;
        arm64)   echo "aarch64" ;;
        aarch64) echo "aarch64" ;;
        armv7l)  echo "armv7l" ;;
        *)       echo "unknown" ;;
    esac
}

OS=$(detect_os)
ARCH=$(detect_arch)

if [ "$OS" = "unknown" ] || [ "$ARCH" = "unknown" ]; then
    echo "ERROR: Unsupported platform: $(uname -s) $(uname -m)" >&2
    exit 1
fi

# ast-grep release asset naming
if [ "$OS" = "macos" ]; then
    ASSET="app-${ARCH}-apple-darwin.zip"
elif [ "$OS" = "linux" ]; then
    ASSET="app-${ARCH}-unknown-linux-gnu.zip"
fi

if [ -z "$ASSET" ]; then
    echo "ERROR: No asset for $OS/$ARCH" >&2
    exit 1
fi

echo "Fetching latest ast-grep release..."
RELEASE_JSON=$(curl -sfL "https://api.github.com/repos/ast-grep/ast-grep/releases/latest")
TAG=$(echo "$RELEASE_JSON" | python3 -c "import sys,json; print(json.load(sys.stdin)['tag_name'])" 2>/dev/null || echo "latest")
DOWNLOAD_URL=$(echo "$RELEASE_JSON" | python3 -c "import sys,json; assets=json.load(sys.stdin)['assets']; print([a['browser_download_url'] for a in assets if a['name']=='${ASSET}'][0])" 2>/dev/null)

if [ -z "$DOWNLOAD_URL" ]; then
    echo "ERROR: Asset not found: $ASSET" >&2
    echo "Available assets:" >&2
    echo "$RELEASE_JSON" | python3 -c "import sys,json; [print(f'  {a[\"name\"]}') for a in json.load(sys.stdin)['assets']]" 2>/dev/null
    exit 1
fi

echo "Downloading ast-grep $TAG ($ASSET)..."
ZIP_PATH="$BIN_DIR/ast-grep.zip"
curl -sfL "$DOWNLOAD_URL" -o "$ZIP_PATH"

echo "Extracting..."
unzip -o "$ZIP_PATH" -d "$BIN_DIR" >/dev/null 2>&1 || {
    # If unzip not available, try python
    python3 -c "import zipfile; zipfile.ZipFile('$ZIP_PATH').extractall('$BIN_DIR')" 2>/dev/null || {
        echo "ERROR: No unzip tool available" >&2
        exit 1
    }
}

# Find and move ast-grep binary
EXE=$(find "$BIN_DIR" -name "ast-grep" -type f 2>/dev/null | head -1)
if [ -n "$EXE" ]; then
    if [ "$EXE" != "$BIN_DIR/ast-grep" ]; then
        mv -f "$EXE" "$BIN_DIR/ast-grep"
    fi
    chmod +x "$BIN_DIR/ast-grep"
fi

# Cleanup
rm -f "$ZIP_PATH"
find "$BIN_DIR" -mindepth 1 -not -name "ast-grep" -not -name "ast-grep.exe" -exec rm -rf {} + 2>/dev/null || true

if [ -f "$BIN_DIR/ast-grep" ]; then
    VER=$("$BIN_DIR/ast-grep" --version 2>&1 || echo "unknown")
    echo "ast-grep installed: $VER"
else
    echo "ERROR: ast-grep not found after extraction" >&2
    exit 1
fi
