#!/usr/bin/env bash
# update_skills.sh updates managed LuaSkills in one selected runtime directory.
# update_skills.sh 用于在一个指定的运行根目录中更新受管 LuaSkills。

set -euo pipefail

# SCRIPT_DIR stores the script directory so repository-relative paths stay stable.
# SCRIPT_DIR 保存脚本目录，确保仓库相对路径保持稳定。
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# PROJECT_ROOT stores the MCP repository root.
# PROJECT_ROOT 保存 MCP 仓库根目录。
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# OUTPUT_RUNTIME_ROOT is the runtime root to update.
# OUTPUT_RUNTIME_ROOT 是要更新的运行根。
OUTPUT_RUNTIME_ROOT="${OUTPUT_RUNTIME_ROOT:-output/lua_runtime}"

LUASKILLS_LIB_ARG=()
if [[ -n "${LUASKILLS_LIB:-}" ]]; then
  LUASKILLS_LIB_ARG=(--luaskills-lib "$LUASKILLS_LIB")
fi

SKIP_BUILD_ARG=()
if [[ "${SKIP_BUILD:-0}" == "1" ]]; then
  SKIP_BUILD_ARG=(--skip-build)
fi

DRY_RUN_ARG=()
if [[ "${DRY_RUN:-0}" == "1" ]]; then
  DRY_RUN_ARG=(--dry-run)
fi

cd "$PROJECT_ROOT"

if command -v python3 >/dev/null 2>&1; then
  PYTHON_BIN="python3"
elif command -v python >/dev/null 2>&1; then
  PYTHON_BIN="python"
else
  echo "Unable to find python3 or python on PATH." >&2
  exit 1
fi

"$PYTHON_BIN" "$PROJECT_ROOT/scripts/update_skills.py" \
  --output-runtime-root "$OUTPUT_RUNTIME_ROOT" \
  "${LUASKILLS_LIB_ARG[@]}" \
  "${SKIP_BUILD_ARG[@]}" \
  "${DRY_RUN_ARG[@]}" \
  "$@"
