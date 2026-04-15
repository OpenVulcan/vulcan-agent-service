#!/usr/bin/env python3
"""中文：批量验证 codekit-ast 备注摘要提取在多语言与多种注释格式下的回归行为。
English: Batch-verify codekit-ast comment summary extraction across multiple languages and comment styles.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(errors="backslashreplace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(errors="backslashreplace")


@dataclass(frozen=True)
class SymbolExpectation:
    """中文：描述单个符号的备注断言条件。
    English: Describe the note assertions for a single symbol.
    """

    symbol: str
    prefix: str
    forbidden: tuple[str, ...] = ("@param", "@returns", "========================================", "----------------------------------------", "�")


@dataclass(frozen=True)
class FixtureExpectation:
    """中文：描述单个测试夹具及其对应的符号断言集合。
    English: Describe one fixture file and its symbol-level expectations.
    """

    relative_path: str
    expectations: tuple[SymbolExpectation, ...]


FIXTURE_MATRIX: tuple[FixtureExpectation, ...] = (
    FixtureExpectation(
        relative_path="testdata/vmcp_ast_comment_notes/typescript_comment_cases.ts",
        expectations=(
            SymbolExpectation(symbol="formatDisplayName", prefix="中文：这是一个很长的 TypeScript 行注"),
            SymbolExpectation(symbol="normalizeHeadline", prefix="中文：这是一个很长的 TypeScript 块注"),
        ),
    ),
    FixtureExpectation(
        relative_path="testdata/vmcp_ast_comment_notes/rust_comment_cases.rs",
        expectations=(
            SymbolExpectation(symbol="collect_memory_key", prefix="中文：这是 Rust 文档注释备注"),
            SymbolExpectation(symbol="normalize_memory_key", prefix="日本語：これは Rust ブロックコメン"),
        ),
    ),
    FixtureExpectation(
        relative_path="testdata/vmcp_ast_comment_notes/python_comment_cases.py",
        expectations=(
            SymbolExpectation(symbol="build_profile_name", prefix="中文：这是 Python 行备注"),
            SymbolExpectation(symbol="render_turn_summary", prefix="한국어: 이 Python Docstring 은 여러 줄 설"),
        ),
    ),
    FixtureExpectation(
        relative_path="testdata/vmcp_ast_comment_notes/lua_comment_cases.lua",
        expectations=(
            SymbolExpectation(symbol="build_skill_name", prefix="中文：这是 Lua 行备注"),
            SymbolExpectation(symbol="normalize_skill_name", prefix="Español: Este bloque"),
        ),
    ),
    FixtureExpectation(
        relative_path="testdata/vmcp_ast_comment_notes/bash_comment_cases.sh",
        expectations=(
            SymbolExpectation(symbol="build_runtime_note", prefix="中文：这是 Shell 函数备注"),
        ),
    ),
    FixtureExpectation(
        relative_path="testdata/vmcp_ast_comment_notes/go_comment_cases.go",
        expectations=(
            SymbolExpectation(symbol="BuildPromptName", prefix="中文：这是 Go 行备注，用于验证双斜"),
        ),
    ),
)


def parse_args() -> argparse.Namespace:
    """中文：解析命令行参数，并为仓库内默认路径提供兜底值。
    English: Parse CLI arguments and provide repository-local default paths.
    """

    repo_root = Path(__file__).resolve().parent.parent
    default_binary = repo_root / "output" / "bin" / ("vulcan-mcp.exe" if sys.platform.startswith("win") else "vulcan-mcp")
    default_config = repo_root / "output" / "configs" / "config.yaml"
    parser = argparse.ArgumentParser(description="Verify codekit-ast comment summary extraction across fixtures.")
    parser.add_argument("--repo-root", type=Path, default=repo_root)
    parser.add_argument("--binary", type=Path, default=default_binary)
    parser.add_argument("--config", type=Path, default=default_config)
    return parser.parse_args()


def build_runtime_override_config(base_config_path: Path, runtime_skill_dir: Path) -> Path:
    """中文：基于现有配置生成临时配置，并强制 Lua skill 覆盖目录指向 runtime 源码。
    English: Build a temporary config that forces the Lua skill override directory to the runtime source tree.
    """

    base_content = base_config_path.read_text(encoding="utf-8")
    override_path = runtime_skill_dir.as_posix()
    temp_file = tempfile.NamedTemporaryFile("w", suffix=".yaml", delete=False, encoding="utf-8")
    with temp_file:
        temp_file.write(base_content.rstrip() + "\n")
        temp_file.write(f'lua_skills_override: "{override_path}"\n')
    return Path(temp_file.name)


def run_vmcp_ast(binary_path: Path, config_path: Path, fixture_path: Path, repo_root: Path) -> dict:
    """中文：调用真实的 codekit-ast 工具并返回 JSON 结果。
    English: Call the real codekit-ast tool and return the parsed JSON result.
    """

    arguments = json.dumps(
        {
            "path": str(fixture_path),
            "comment": True,
            "workdir": str(repo_root),
        },
        ensure_ascii=False,
    )
    command = [str(binary_path), "-config", str(config_path), "--call-tools", "codekit-ast", arguments]
    completed = subprocess.run(command, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if completed.returncode != 0:
        raise RuntimeError(
            "codekit-ast 执行失败 / codekit-ast execution failed\n"
            f"command: {' '.join(command)}\n"
            f"stdout:\n{completed.stdout}\n"
            f"stderr:\n{completed.stderr}"
        )
    try:
        return json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError(
            "无法解析 codekit-ast 输出 / Failed to parse codekit-ast output\n"
            f"stdout:\n{completed.stdout}\n"
            f"stderr:\n{completed.stderr}"
        ) from error


def find_note_for_symbol(file_content: str, symbol_name: str) -> str:
    """中文：在单文件结构摘要中定位指定符号的备注摘要。
    English: Locate the note summary for a target symbol within one file outline.
    """

    lines = file_content.splitlines()
    symbol_pattern = re.compile(rf"\b{re.escape(symbol_name)}\b")
    for index, line in enumerate(lines):
        if symbol_pattern.search(line) and re.search(r"\bL\d+-\d+\b", line):
            for offset in range(1, 4):
                next_index = index + offset
                if next_index >= len(lines):
                    break
                matched = re.search(r"note:\s*(.+)$", lines[next_index])
                if matched:
                    return matched.group(1).strip()
    raise AssertionError(f"未找到符号备注 / Note not found for symbol: {symbol_name}")


def assert_fixture_result(file_content: str, fixture: FixtureExpectation) -> list[str]:
    """中文：校验单个夹具的备注输出，并返回简短通过说明。
    English: Validate one fixture output and return short pass messages.
    """

    messages: list[str] = []
    for expectation in fixture.expectations:
        note = find_note_for_symbol(file_content, expectation.symbol)
        if not note.startswith(expectation.prefix):
            raise AssertionError(
                f"符号 {expectation.symbol} 的备注前缀不符合预期 / Unexpected note prefix.\n"
                f"expected prefix: {expectation.prefix}\nactual note: {note}"
            )
        if len(note.encode("utf-8")) > 50:
            raise AssertionError(
                f"符号 {expectation.symbol} 的备注超过 50 字节限制 / Note exceeds 50-byte limit.\nactual note: {note}"
            )
        for forbidden_token in expectation.forbidden:
            if forbidden_token in note:
                raise AssertionError(
                    f"符号 {expectation.symbol} 的备注包含禁用片段 / Forbidden token found in note.\n"
                    f"token: {forbidden_token}\nactual note: {note}"
                )
        messages.append(f"{expectation.symbol}: {note}")
    return messages


def main() -> int:
    """中文：执行所有夹具回归校验，并在控制台输出简洁结果。
    English: Execute all fixture validations and print a concise console summary.
    """

    args = parse_args()
    repo_root = args.repo_root.resolve()
    binary_path = args.binary.resolve()
    config_path = args.config.resolve()
    runtime_skill_dir = repo_root / "runtime" / "lua_skills"

    if not binary_path.exists():
        raise FileNotFoundError(f"未找到可执行文件 / Binary not found: {binary_path}")
    if not config_path.exists():
        raise FileNotFoundError(f"未找到配置文件 / Config not found: {config_path}")
    if not runtime_skill_dir.exists():
        raise FileNotFoundError(f"未找到 runtime skill 目录 / Runtime skill directory not found: {runtime_skill_dir}")

    print("开始验证 codekit-ast 备注摘要回归 / Start verifying codekit-ast comment summary regression")
    temp_config_path = build_runtime_override_config(config_path, runtime_skill_dir)
    try:
        for fixture in FIXTURE_MATRIX:
            fixture_path = (repo_root / fixture.relative_path).resolve()
            if not fixture_path.exists():
                raise FileNotFoundError(f"未找到测试夹具 / Fixture not found: {fixture_path}")
            result = run_vmcp_ast(binary_path, temp_config_path, fixture_path, repo_root)
            files = result.get("files") or []
            if not files:
                raise AssertionError(f"夹具未返回结构结果 / Fixture returned no file outline: {fixture_path}")
            file_content = str(files[0].get("content") or "")
            messages = assert_fixture_result(file_content, fixture)
            print(f"[PASS] {fixture.relative_path}")
            for message in messages:
                print(f"  - {message}")
    finally:
        temp_config_path.unlink(missing_ok=True)

    print("全部备注格式回归验证通过 / All comment summary regression checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
