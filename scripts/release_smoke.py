"""Run the extracted application without installing network skills.
运行解压后的应用，验证过程不安装联网技能。
"""

from __future__ import annotations

import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import zipfile
from collections.abc import Iterator


@contextmanager
def extraction_root() -> Iterator[Path]:
    """Yield an owned temporary directory that supports long Windows runtime paths.
    提供支持 Windows 长运行时路径的独占临时目录。
    Remove only this newly allocated directory after the smoke test completes.
    冒烟检查结束后仅清理本函数新分配的目录。
    """
    # Validate ownership before adding the Windows extended-length path prefix.
    # 添加 Windows 扩展长度路径前缀前，先校验目录归属。
    temporary = Path(tempfile.mkdtemp(prefix="vas-")).resolve()
    if not temporary.is_relative_to(Path(tempfile.gettempdir()).resolve()):
        raise RuntimeError("Smoke extraction directory escaped the temporary root")
    directory = temporary
    if os.name == "nt":
        # Drive and UNC paths have different documented extended-length spellings.
        # 驱动器路径和 UNC 路径使用不同的标准扩展长度写法。
        spelling = str(temporary)
        if not spelling.startswith("\\\\?\\"):
            spelling = "\\\\?\\UNC\\" + spelling[2:] if spelling.startswith("\\\\") else "\\\\?\\" + spelling
        directory = Path(spelling)
    try:
        yield directory
    finally:
        shutil.rmtree(directory)


def check_command(arguments: list[str], cwd: Path, expected_returncode: int = 0) -> str:
    """Run arguments in cwd and return combined output, requiring the expected exit code.
    在 cwd 下运行 arguments 并返回合并输出，要求退出码与 expected_returncode 一致。
    """
    # Bound startup time so a broken package cannot hold the release job indefinitely.
    # 限制启动时间，避免损坏的发行包无限占用发布任务。
    result = subprocess.run(
        arguments, cwd=cwd, text=True, encoding="utf-8", errors="replace",
        capture_output=True, timeout=120, check=False,
    )
    print(result.stdout)
    print(result.stderr)
    if result.returncode != expected_returncode:
        raise RuntimeError(f"Smoke command failed ({result.returncode}): {arguments[0]}")
    return result.stdout + result.stderr


def smoke_test(directory: Path, tag: str, platform: str) -> None:
    """Extract the named release and verify init, engine loading, and controller launch.
    解压指定目录、标签和平台的发行包，验证初始化、引擎加载及控制器启动。
    Returns normally only after all subprocesses complete successfully.
    仅在全部子进程验证成功后正常返回。
    """
    # Select the exact artifact name defined by the application release contract.
    # 选择应用发布契约定义的精确资产名称。
    stem = f"vulcan-agent-service-{tag}-{platform}"
    suffix = ".zip" if platform == "windows-x64" else ".tar.gz"
    archive = directory / f"{stem}{suffix}"
    with extraction_root() as extraction:
        # Use an isolated extraction root; smoke configuration never changes the shipped archive.
        # 使用独立解压根，冒烟配置不会修改交付的归档文件。
        if suffix == ".zip":
            with zipfile.ZipFile(archive) as package:
                for member in package.namelist():
                    if not (extraction / member).resolve().is_relative_to(extraction):
                        raise RuntimeError(f"Archive member escapes extraction root: {member}")
                package.extractall(extraction)
        else:
            with tarfile.open(archive) as package:
                package.extractall(extraction, filter="data")
        # The archive contains exactly one versioned application directory.
        # 归档包含唯一带版本号的应用目录。
        application = extraction / stem
        executable_suffix = ".exe" if platform == "windows-x64" else ""
        binary = application / "bin" / f"vulcan-agent-service{executable_suffix}"
        controller = application / "lua_runtime" / "bin" / f"vldb-controller{executable_suffix}"
        # Keep mutable skill configuration and USER packages inside the disposable fixture.
        # 将可变技能配置与 USER 技能包限制在可丢弃的夹具内。
        user_skills = application / "user-skills"
        user_skills.mkdir()
        config = {
            "format_version": 1,
            "vmm_enable": False,
            "skill_config_root": str(application / "user-config"),
            "skill_roots": [
                {"name": "ROOT", "path": "lua_runtime/skills"},
                {"name": "USER", "path": "user-skills"},
            ],
            "space_controller": {"auto_spawn": False},
        }
        (application / "configs" / "config.yaml").write_text(json.dumps(config), encoding="utf-8")
        (application / "configs" / "system_skills.json").write_text(
            json.dumps({"format_version": 1, "auto_install": False, "skills": []}),
            encoding="utf-8",
        )
        check_command([str(binary), "init", "--runtime-root", str(application)], extraction)
        # The help path constructs the full Lua engine and validates packaged runtime resources.
        # 帮助调用路径构造完整 Lua 引擎并校验包内运行资源。
        output = check_command(
            [str(binary), "--call-tools", "vulcan-help-list", "--runtime-root", str(application)],
            extraction,
        )
        if "# Vulcan Help List" not in output:
            raise RuntimeError("The extracted application did not return the expected help result")
        # Exercise actual native module loading, including curl and zlib shared dependencies.
        # 实际加载原生模块，包括 curl 与 zlib 的共享依赖。
        skill_directory = application / "lua_runtime" / "skills" / "release-smoke"
        (skill_directory / "runtime").mkdir(parents=True)
        skill_manifest = {
            "name": "release-smoke", "version": "0.1.0", "enable": True, "debug": False,
            "entries": [{"name": "native", "description": "Verify packaged native modules.",
                         "lua_entry": "runtime/native.lua", "lua_module": "release-smoke.native"}],
        }
        (skill_directory / "skill.yaml").write_text(json.dumps(skill_manifest), encoding="utf-8")
        (skill_directory / "runtime" / "native.lua").write_text(
            "-- Load native packages from the extracted release.\n"
            "-- 从解压后的发行包加载原生模块。\n"
            "return function(args)\n"
            "  require('cjson')\n  require('lfs')\n  require('lcurl')\n  require('zlib')\n"
            "  return 'release-native-ok'\nend\n",
            encoding="utf-8",
        )
        output = check_command(
            [str(binary), "--call-tools", "release-smoke-native", "--runtime-root", str(application)],
            extraction,
        )
        if "release-native-ok" not in output:
            raise RuntimeError("Packaged native module loading did not complete")
        # The published controller 0.2.3 returns its help text as an InvalidInput error (exit 1).
        # 已发布的 controller 0.2.3 通过 InvalidInput 错误返回帮助文本，退出码为 1。
        controller_help = check_command([str(controller), "--help"], extraction, expected_returncode=1)
        if "Supported arguments:" not in controller_help or "--bind" not in controller_help:
            raise RuntimeError("The packaged controller did not return its supported CLI arguments")
        print(f"Release smoke passed: {stem}")


def main() -> None:
    """Parse the release artifact location and run its isolated smoke checks.
    解析发行资产位置并运行隔离的冒烟检查。
    """
    # Parse only the explicit artifact coordinates supplied by the build matrix.
    # 仅解析构建矩阵提供的显式资产坐标。
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--platform", required=True, choices=(
        "windows-x64", "linux-x64", "linux-arm64", "macos-x64", "macos-arm64",
    ))
    arguments = parser.parse_args()
    smoke_test(arguments.directory.resolve(), arguments.tag, arguments.platform)


if __name__ == "__main__":
    main()
