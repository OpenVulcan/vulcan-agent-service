#!/usr/bin/env bash
# make.sh provides the shell-native task entry for local build and run workflows.
# make.sh 用于提供本地构建与运行工作流的 shell 原生入口。

set -euo pipefail

# SCRIPT_DIR stores the repository root so delegated scripts resolve relative paths consistently.
# SCRIPT_DIR 用于保存仓库根目录，确保被转发脚本的相对路径解析保持一致。
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# COMMAND_MODE captures the first argument that selects build, run, or release behavior.
# COMMAND_MODE 用于承接第一个参数，以选择 build、run 或 release 行为。
COMMAND_MODE="${1:-}"

# COMMAND_VARIANT captures the optional second argument such as release after run.
# COMMAND_VARIANT 用于承接可选的第二个参数，例如 run 后面的 release。
COMMAND_VARIANT="${2:-}"

# BUILD_SCRIPT_PATH points at the dedicated shell build script so packaging logic stays centralized.
# BUILD_SCRIPT_PATH 用于指向专用的 shell 构建脚本，保证打包逻辑集中维护。
BUILD_SCRIPT_PATH="${SCRIPT_DIR}/scripts/build.sh"

# DEFAULT_BIN_PATH points at the debug artifact location used by the default run flow.
# DEFAULT_BIN_PATH 用于指向默认运行流程使用的 debug 产物位置。
DEFAULT_BIN_PATH="${SCRIPT_DIR}/output/debug/vulcan-mcp"

# RELEASE_BIN_PATH points at the release artifact location used by the release run flow.
# RELEASE_BIN_PATH 用于指向 release 运行流程使用的产物位置。
RELEASE_BIN_PATH="${SCRIPT_DIR}/output/bin/vulcan-mcp"

# normalize_command converts raw input into a trimmed lower-case token so command dispatch stays stable.
# normalize_command 用于把原始输入转换为去空白的小写标记，确保命令分发保持稳定。
normalize_command() {
    local value="${1:-}"
    value="${value#"${value%%[![:space:]]*}"}"
    value="${value%"${value##*[![:space:]]}"}"
    printf '%s' "${value,,}"
}

# resolve_binary_path returns the executable path for the requested profile and accepts either native or Windows .exe artifacts.
# resolve_binary_path 用于返回请求 profile 对应的可执行路径，并兼容原生产物或 Windows 的 .exe 产物。
resolve_binary_path() {
    local base_path="$1"
    if [ -f "${base_path}" ]; then
        printf '%s' "${base_path}"
        return 0
    fi
    if [ -f "${base_path}.exe" ]; then
        printf '%s' "${base_path}.exe"
        return 0
    fi
    return 1
}

# invoke_build forwards the selected profile to scripts/build.sh and preserves the child exit semantics.
# invoke_build 用于把选中的 profile 转发给 scripts/build.sh，并保持子进程退出语义。
invoke_build() {
    local is_release="$1"
    if [ ! -f "${BUILD_SCRIPT_PATH}" ]; then
        echo "Missing build script: ${BUILD_SCRIPT_PATH}" >&2
        exit 1
    fi

    if [ "${is_release}" = "true" ]; then
        bash "${BUILD_SCRIPT_PATH}" release
    else
        bash "${BUILD_SCRIPT_PATH}"
    fi
}

# invoke_run builds the target binary on demand and then executes it without routing through a Windows batch wrapper.
# invoke_run 用于按需构建目标二进制，然后直接执行，避免再经过 Windows 批处理包装层。
invoke_run() {
    local is_release="$1"
    local base_path=""
    local binary_path=""

    if [ "${is_release}" = "true" ]; then
        base_path="${RELEASE_BIN_PATH}"
    else
        base_path="${DEFAULT_BIN_PATH}"
    fi

    if ! binary_path="$(resolve_binary_path "${base_path}")"; then
        invoke_build "${is_release}"
        binary_path="$(resolve_binary_path "${base_path}")"
    fi

    echo "==> Running vulcan-mcp (${binary_path})..."
    echo
    "${binary_path}"
}

# show_usage prints the supported command forms so invalid input is easy to correct.
# show_usage 用于输出支持的命令形式，便于快速纠正无效输入。
show_usage() {
    cat <<'EOF'
Usage:
  ./make.sh             # debug build
  ./make.sh build       # debug build
  ./make.sh release     # release build
  ./make.sh run         # run debug build
  ./make.sh run release # run release build
EOF
}

# NORMALIZED_MODE stores the canonical top-level command token used by the dispatcher.
# NORMALIZED_MODE 用于保存分发器使用的规范化顶层命令标记。
NORMALIZED_MODE="$(normalize_command "${COMMAND_MODE}")"

# NORMALIZED_VARIANT stores the canonical secondary token used to distinguish release submodes.
# NORMALIZED_VARIANT 用于保存规范化的二级标记，以区分 release 子模式。
NORMALIZED_VARIANT="$(normalize_command "${COMMAND_VARIANT}")"

case "${NORMALIZED_MODE}" in
    "")
        invoke_build "false"
        ;;
    build)
        if [ "${NORMALIZED_VARIANT}" = "release" ]; then
            invoke_build "true"
        else
            invoke_build "false"
        fi
        ;;
    release)
        invoke_build "true"
        ;;
    run)
        if [ "${NORMALIZED_VARIANT}" = "release" ]; then
            invoke_run "true"
        else
            invoke_run "false"
        fi
        ;;
    *)
        echo "Unsupported command: '${COMMAND_MODE}'" >&2
        show_usage
        exit 1
        ;;
esac
