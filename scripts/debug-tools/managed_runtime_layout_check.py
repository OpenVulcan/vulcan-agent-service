#!/usr/bin/env python3
"""
Validate one prepared LuaSkills managed runtime layout across host-selected roots.
跨宿主指定根校验一个已准备好的 LuaSkills 受管运行时布局。
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import stat
import sys
from pathlib import Path


# PythonVersion is the exact managed CPython layout version validated by default.
# PythonVersion 是默认校验的精确受管 CPython 布局版本。
PYTHON_VERSION = "3.14.6"
# UvVersion is the exact standalone uv layout version validated by default.
# UvVersion 是默认校验的精确独立 uv 布局版本。
UV_VERSION = "0.11.28"
# NodeVersion is the exact managed Node.js layout version validated by default.
# NodeVersion 是默认校验的精确受管 Node.js 布局版本。
NODE_VERSION = "24.18.0"
# PnpmVersion is the exact standalone pnpm layout version validated by default.
# PnpmVersion 是默认校验的精确独立 pnpm 布局版本。
PNPM_VERSION = "11.11.0"


def current_platform_key() -> str:
    """
    Return the managed runtime platform key for the current host.
    返回当前宿主的受管运行时平台键。
    """
    system = platform.system().lower()
    machine = platform.machine().lower()
    if system == "windows":
        os_key = "windows"
    elif system == "linux":
        os_key = "linux"
    elif system == "darwin":
        os_key = "macos"
    else:
        raise RuntimeError(f"unsupported operating system: {platform.system()}")

    if machine in {"amd64", "x86_64"}:
        arch_key = "x64"
    elif machine in {"arm64", "aarch64"}:
        arch_key = "arm64"
    else:
        raise RuntimeError(f"unsupported architecture: {platform.machine()}")

    if os_key == "windows" and arch_key == "arm64":
        raise RuntimeError("windows_arm_is_not_supported")
    return f"{os_key}-{arch_key}"


def read_manifest(path: Path) -> dict:
    """
    Read one runtime-manifest.json file as UTF-8 while accepting a BOM.
    以 UTF-8 读取单个 runtime-manifest.json 文件，同时兼容 BOM。
    """
    text = path.read_text(encoding="utf-8-sig")
    return json.loads(text)


def executable_exists(path: Path) -> bool:
    """
    Return whether one executable path exists and is runnable enough for the host platform.
    返回单个可执行路径是否存在，并且对当前宿主而言具备足够的可运行属性。
    """
    if not path.is_file():
        return False
    if os.name == "nt":
        return True
    return bool(path.stat().st_mode & stat.S_IXUSR)


def validate_install(
    distribution_root: Path,
    family: str,
    directory_name: str,
    runtime: str,
    version: str,
    platform_key: str,
) -> list[str]:
    """
    Validate one managed runtime installation directory and manifest.
    校验单个受管运行时安装目录与清单。

    Args:
        distribution_root: Root that directly contains python and node families.
        family: Distribution family directory, either python or node.
        directory_name: Exact installation directory name.
        runtime: Exact manifest runtime identifier.
        version: Exact manifest version.
        platform_key: Exact normalized manifest platform key.
    Returns:
        Every layout or manifest validation error.

    参数：
        distribution_root：直接包含 python 与 node 目录的根。
        family：发行族目录，只能是 python 或 node。
        directory_name：精确安装目录名。
        runtime：精确清单运行时标识。
        version：精确清单版本。
        platform_key：精确规范化清单平台键。
    返回：
        全部布局或清单校验错误。
    """
    errors: list[str] = []
    install_dir = distribution_root / family / directory_name
    manifest_path = install_dir / "runtime-manifest.json"
    if not manifest_path.is_file():
        return [f"missing manifest: {manifest_path}"]

    try:
        manifest = read_manifest(manifest_path)
    except Exception as error:  # noqa: BLE001
        return [f"failed to parse manifest {manifest_path}: {error}"]

    expected = {
        "schema_version": 1,
        "runtime": runtime,
        "version": version,
        "platform": platform_key,
    }
    for key, value in expected.items():
        if manifest.get(key) != value:
            errors.append(
                f"{manifest_path}: expected {key}={value!r}, got {manifest.get(key)!r}"
            )

    executable = manifest.get("executable")
    if not isinstance(executable, str) or not executable:
        errors.append(f"{manifest_path}: executable must be a non-empty string")
    elif Path(executable).is_absolute() or ".." in Path(executable).parts:
        errors.append(f"{manifest_path}: executable must stay under install directory")
    else:
        executable_path = install_dir / executable
        if not executable_exists(executable_path):
            errors.append(f"{manifest_path}: executable not found or not runnable: {executable_path}")

    source = manifest.get("source")
    if not isinstance(source, str) or not source:
        errors.append(f"{manifest_path}: source must be a non-empty string")

    return errors


def validate_env_markers(environment_root: Path) -> list[str]:
    """
    Validate managed runtime environment marker files when environments exist.
    在环境存在时校验受管运行时环境 marker 文件。

    Args:
        environment_root: Writable root that contains managed environments.
    Returns:
        Every schema or identity-field validation error.

    参数：
        environment_root：包含受管环境的可写根。
    返回：
        全部 schema 或身份字段校验错误。
    """
    errors: list[str] = []
    if not environment_root.exists():
        return errors

    for marker_path in environment_root.rglob(".luaskills-env.json"):
        try:
            marker = read_manifest(marker_path)
        except Exception as error:  # noqa: BLE001
            errors.append(f"failed to parse env marker {marker_path}: {error}")
            continue

        for key in (
            "schema_version",
            "runtime",
            "runtime_version",
            "platform",
            "package_manager",
            "package_manager_version",
            "lock_hash",
            "runtime_install_manifest_hash",
            "runtime_executable_hash",
            "package_manager_install_manifest_hash",
            "package_manager_executable_hash",
            "env_hash",
        ):
            if key not in marker:
                errors.append(f"{marker_path}: missing marker field {key}")
        if marker.get("schema_version") != 2:
            errors.append(f"{marker_path}: schema_version must be 2")
        if marker.get("runtime") not in {"python", "node"}:
            errors.append(f"{marker_path}: runtime must be python or node")
        for key in (
            "lock_hash",
            "runtime_install_manifest_hash",
            "runtime_executable_hash",
            "package_manager_install_manifest_hash",
            "package_manager_executable_hash",
            "env_hash",
        ):
            if key in marker and not is_sha256(marker[key]):
                errors.append(f"{marker_path}: {key} must be one lowercase SHA-256 digest")

    return errors


def is_sha256(value: object) -> bool:
    """
    Return whether one value is a canonical lowercase SHA-256 digest.
    返回某个值是否为规范小写 SHA-256 摘要。

    Args:
        value: Arbitrary decoded marker value.
    Returns:
        True only for a 64-character lowercase hexadecimal string.

    参数：
        value：任意已解码 marker 值。
    返回：
        仅在值为 64 字符小写十六进制字符串时返回 True。
    """

    return (
        isinstance(value, str)
        and len(value) == 64
        and all(character in "0123456789abcdef" for character in value)
    )


def validate_layout(
    runtime_root: Path,
    distribution_root: Path | None = None,
    environment_root: Path | None = None,
) -> list[str]:
    """
    Validate all first-class managed runtime layout entries across host-selected roots.
    跨宿主指定根校验全部一等受管运行时布局项。

    Args:
        runtime_root: LuaSkills data root used only for compatible defaults.
        distribution_root: Optional explicit interpreter distribution root.
        environment_root: Optional explicit writable environment root.
    Returns:
        Every installation and environment-marker validation error.

    参数：
        runtime_root：仅用于兼容默认值的 LuaSkills 数据根。
        distribution_root：可选显式解释器发行根。
        environment_root：可选显式可写环境根。
    返回：
        全部安装与环境 marker 校验错误。
    """
    # DistributionRoot uses the explicit host root or the compatible runtime-root default.
    # DistributionRoot 使用显式宿主根或兼容的 runtime-root 默认值。
    resolved_distribution_root = (
        distribution_root or runtime_root / "dependencies" / "runtimes"
    )
    # EnvironmentRoot uses the explicit host root or the compatible runtime-root default.
    # EnvironmentRoot 使用显式宿主根或兼容的 runtime-root 默认值。
    resolved_environment_root = (
        environment_root or runtime_root / "dependencies" / "envs"
    )
    platform_key = current_platform_key()
    errors: list[str] = []
    errors.extend(
        validate_install(
            resolved_distribution_root,
            "python",
            f"uv-{UV_VERSION}-{platform_key}",
            "uv",
            UV_VERSION,
            platform_key,
        )
    )
    errors.extend(
        validate_install(
            resolved_distribution_root,
            "python",
            f"cpython-{PYTHON_VERSION}-{platform_key}",
            "python",
            PYTHON_VERSION,
            platform_key,
        )
    )
    errors.extend(
        validate_install(
            resolved_distribution_root,
            "node",
            f"node-{NODE_VERSION}-{platform_key}",
            "node",
            NODE_VERSION,
            platform_key,
        )
    )
    errors.extend(
        validate_install(
            resolved_distribution_root,
            "node",
            f"pnpm-{PNPM_VERSION}",
            "pnpm",
            PNPM_VERSION,
            "any",
        )
    )
    errors.extend(validate_env_markers(resolved_environment_root))
    return errors


def main() -> int:
    """
    Parse CLI arguments and validate one managed runtime layout.
    解析命令行参数并校验单个受管运行时布局。

    Returns:
        Zero when all selected roots are valid, otherwise one.

    返回：
        全部所选根有效时返回零，否则返回一。
    """
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("runtime_root", help="Prepared LuaSkills runtime root to validate.")
    parser.add_argument(
        "--distribution-root",
        help="Optional host-configured root that directly contains python and node.",
    )
    parser.add_argument(
        "--environment-root",
        help="Optional host-configured writable managed environment root.",
    )
    args = parser.parse_args()

    # RuntimeRoot is the LuaSkills data root used for compatible defaults.
    # RuntimeRoot 是用于兼容默认值的 LuaSkills 数据根。
    runtime_root = Path(args.runtime_root).resolve()
    # DistributionRoot remains independent from runtime_root when the host supplies it.
    # DistributionRoot 在宿主提供时保持与 runtime_root 相互独立。
    distribution_root = (
        Path(args.distribution_root).resolve() if args.distribution_root else None
    )
    # EnvironmentRoot remains independent from runtime_root when the host supplies it.
    # EnvironmentRoot 在宿主提供时保持与 runtime_root 相互独立。
    environment_root = (
        Path(args.environment_root).resolve() if args.environment_root else None
    )
    errors = validate_layout(runtime_root, distribution_root, environment_root)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1

    print(f"Managed runtime layout ok: runtime_root={runtime_root}")
    print(
        f"distribution_root={distribution_root or runtime_root / 'dependencies' / 'runtimes'}"
    )
    print(
        f"environment_root={environment_root or runtime_root / 'dependencies' / 'envs'}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
