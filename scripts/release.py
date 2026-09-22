#!/usr/bin/env python3
"""
Build and verify self-contained vulcan-agent-service release archives.
构建并校验自包含的 vulcan-agent-service 发布归档。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import zipfile
from pathlib import Path, PurePosixPath, PureWindowsPath
from typing import Iterable, NamedTuple


# RepositoryRoot points at the checkout that owns this release script.
# RepositoryRoot 指向拥有此发布脚本的仓库检出根目录。
REPOSITORY_ROOT = Path(__file__).resolve().parents[1]

# ProductName is the stable archive product name required by the release contract.
# ProductName 是发布契约要求的稳定归档产品名称。
PRODUCT_NAME = "vulcan-agent-service"

# RequiredConfigFiles are the five repository configuration templates shipped in every archive.
# RequiredConfigFiles 是每个归档都必须携带的五个仓库配置模板。
REQUIRED_CONFIG_FILES = (
    "config.yaml",
    "client_budgets.yaml",
    "model_config.yaml",
    "system_skills.json",
    "tool_configs.yaml",
)

# RequiredRuntimeNames are the runtime entries validated by the repository layout checker.
# RequiredRuntimeNames 是仓库布局校验器所验证的运行时条目集合。
REQUIRED_RUNTIME_NAMES = frozenset({"uv", "python", "node", "pnpm"})

# ForbiddenRuntimeComponents prevent user data or mutable runtime state from entering a release.
# ForbiddenRuntimeComponents 防止用户数据或可变运行时状态进入发布包。
FORBIDDEN_RUNTIME_COMPONENTS = frozenset({"skills", "state", "databases", "userdata"})

# SHA256_PATTERN describes the canonical lowercase digest syntax used by sidecar files.
# SHA256_PATTERN 描述 sidecar 文件使用的规范小写摘要格式。
SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")


# TargetSpec records the archive and executable details for one Rust target triple.
# TargetSpec 记录一个 Rust target triple 对应的归档和可执行文件信息。
class TargetSpec(NamedTuple):
    """
    Describe one supported release target.
    描述一个受支持的发布目标。

    Args:
        platform: Stable platform label used in the archive name and manifest.
        archive_suffix: Archive suffix selected by the target operating system.
        binary_name: Main executable name inside the archive.
    Returns:
        A fixed immutable target description.
    """

    platform: str
    archive_suffix: str
    binary_name: str


# TARGET_SPECS is the complete release matrix and is intentionally closed.
# TARGET_SPECS 是完整发布矩阵，并且刻意保持为封闭集合。
TARGET_SPECS = {
    "x86_64-pc-windows-msvc": TargetSpec("windows-x64", ".zip", "vulcan-agent-service.exe"),
    "x86_64-unknown-linux-gnu": TargetSpec("linux-x64", ".tar.gz", "vulcan-agent-service"),
    "aarch64-unknown-linux-gnu": TargetSpec("linux-arm64", ".tar.gz", "vulcan-agent-service"),
    "x86_64-apple-darwin": TargetSpec("macos-x64", ".tar.gz", "vulcan-agent-service"),
    "aarch64-apple-darwin": TargetSpec("macos-arm64", ".tar.gz", "vulcan-agent-service"),
}


# ReleaseError is the user-facing error type for deterministic validation failures.
# ReleaseError 是确定性校验失败使用的面向用户错误类型。
class ReleaseError(RuntimeError):
    """
    Report a release packaging or verification failure.
    报告发布打包或校验失败。
    """


# Metadata stores the product information read from Cargo.toml.
# Metadata 保存从 Cargo.toml 读取的产品信息。
class Metadata(NamedTuple):
    """
    Hold the Cargo package name and version.
    保存 Cargo 包名称与版本。

    Args:
        name: Product name from the Cargo package table.
        version: Product version from the Cargo package table.
    Returns:
        A fixed immutable metadata value.
    """

    name: str
    version: str


# _archive_exists handles dangling output symlinks when refusing overwrite.
# _archive_exists 在拒绝覆盖时也处理悬空输出符号链接。
def _archive_exists(path: Path) -> bool:
    """
    Return whether a path occupies an output name, including a dangling symlink.
    返回路径是否占用输出名称，包括悬空符号链接。

    Args:
        path: Candidate output path.
    Returns:
        True when the path exists or is a symbolic link.
    """

    return path.exists() or path.is_symlink()


# _is_relative_to provides a Python 3.11-compatible containment check.
# _is_relative_to 提供兼容 Python 3.11 的路径包含判断。
def _is_relative_to(path: Path, root: Path) -> bool:
    """
    Return whether path is contained by root after lexical resolution.
    返回 path 在词法解析后是否位于 root 之下。

    Args:
        path: Candidate path.
        root: Containing root.
    Returns:
        True when path is root itself or a descendant of root.
    """

    try:
        path.relative_to(root)
    except ValueError:
        return False
    return True


# _relative_target resolves a symlink target without permitting archive escape.
# _relative_target 解析符号链接目标，同时禁止目标逃逸归档根目录。
def _relative_target(link_path: Path, root: Path) -> Path:
    """
    Resolve and validate one relative symbolic link below root.
    解析并校验 root 下的一个相对符号链接。

    Args:
        link_path: Symbolic link whose target is checked.
        root: Root that must contain the resolved target.
    Returns:
        The resolved target path.
    Raises:
        ReleaseError: If the link is absolute, escapes root, or is dangling.
    """

    target_text = os.readlink(link_path)
    target_posix = PurePosixPath(target_text)
    target_windows = PureWindowsPath(target_text)
    if (
        target_posix.is_absolute()
        or target_windows.is_absolute()
        or bool(target_windows.drive)
        or os.path.isabs(target_text)
    ):
        raise ReleaseError(f"symbolic link must be relative: {link_path}")

    resolved_root = root.resolve()
    resolved_target = (link_path.parent / target_text).resolve(strict=False)
    if not _is_relative_to(resolved_target, resolved_root):
        raise ReleaseError(f"symbolic link escapes package root: {link_path}")
    if not resolved_target.exists():
        raise ReleaseError(f"symbolic link target does not exist: {link_path}")
    return resolved_target


# _validate_tree checks forbidden components and symlink safety before copying or archiving.
# _validate_tree 在复制或归档前校验禁止目录名与符号链接安全性。
def _validate_tree(root: Path) -> None:
    """
    Validate every path below one source or staging root.
    校验一个来源或暂存根下的全部路径。

    Args:
        root: Existing directory to inspect.
    Raises:
        ReleaseError: If a forbidden component or unsafe symlink is found.
    """

    if not root.is_dir() or root.is_symlink():
        raise ReleaseError(f"expected a real directory: {root}")

    resolved_root = root.resolve()
    for path in root.rglob("*"):
        relative = path.relative_to(root)
        if FORBIDDEN_RUNTIME_COMPONENTS.intersection(relative.parts):
            raise ReleaseError(f"forbidden runtime data path: {relative.as_posix()}")
        if path.is_symlink():
            _relative_target(path, resolved_root)


# _remove_existing_destination removes only an entry inside a temporary staging tree.
# _remove_existing_destination 只移除临时暂存树中的目标条目。
def _remove_existing_destination(path: Path) -> None:
    """
    Remove one staging destination entry before replacing it.
    在替换暂存目标前移除一个暂存条目。

    Args:
        path: Entry inside a temporary staging tree.
    Returns:
        None.
    """

    if path.is_symlink() or path.is_file():
        path.unlink()
    elif path.is_dir():
        shutil.rmtree(path)


# _copy_entry copies a source entry while preserving symbolic links and file modes.
# _copy_entry 复制来源条目，同时保留符号链接和文件权限。
def _copy_entry(source: Path, destination: Path) -> None:
    """
    Copy one filesystem entry into a staging tree.
    将一个文件系统条目复制到暂存树。

    Args:
        source: Source file, directory, or symbolic link.
        destination: Destination path inside staging.
    Raises:
        ReleaseError: If source and destination types cannot be merged safely.
    """

    if source.is_symlink():
        if _archive_exists(destination):
            _remove_existing_destination(destination)
        destination.parent.mkdir(parents=True, exist_ok=True)
        os.symlink(os.readlink(source), destination, target_is_directory=source.is_dir())
        return

    if source.is_dir():
        if destination.exists() and not destination.is_dir():
            raise ReleaseError(f"cannot merge directory into file: {destination}")
        destination.mkdir(parents=True, exist_ok=True)
        for child in sorted(source.iterdir(), key=lambda item: item.name):
            _copy_entry(child, destination / child.name)
        shutil.copystat(source, destination, follow_symlinks=False)
        return

    if not source.is_file():
        raise ReleaseError(f"unsupported source entry: {source}")
    if destination.exists() and destination.is_dir():
        raise ReleaseError(f"cannot replace directory with file: {destination}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, destination, follow_symlinks=False)


# _copy_tree_contents validates and merges a directory into a staging destination.
# _copy_tree_contents 校验并将一个目录合并到暂存目标。
def _copy_tree_contents(source: Path, destination: Path) -> None:
    """
    Copy the direct contents of source into destination.
    将 source 的直属内容复制到 destination。

    Args:
        source: Existing non-symlink source directory.
        destination: Destination directory that may already contain merged content.
    Raises:
        ReleaseError: If source is absent or contains unsafe content.
    """

    if not source.is_dir() or source.is_symlink():
        raise ReleaseError(f"required directory is missing: {source}")
    _validate_tree(source)
    destination.mkdir(parents=True, exist_ok=True)
    for child in sorted(source.iterdir(), key=lambda item: item.name):
        _copy_entry(child, destination / child.name)


# _add_macos_lua_module_aliases matches LuaSkills 0.5.7's dylib search paths to upstream .so modules.
# _add_macos_lua_module_aliases 使 LuaSkills 0.5.7 的 dylib 搜索路径匹配上游 .so 模块。
def _add_macos_lua_module_aliases(lua_packages: Path) -> None:
    """Create relative dylib aliases for packaged macOS Lua C modules.
    为已打包的 macOS Lua C 模块创建相对路径的 dylib 别名。

    Args:
        lua_packages: Staged lua_packages directory from the official runtime asset.
    Raises:
        ReleaseError: If the native module tree or an alias destination is invalid.
    Returns:
        None.
    """

    module_root = lua_packages / "lib" / "lua"
    if not module_root.is_dir() or module_root.is_symlink():
        raise ReleaseError(f"macOS Lua native module directory is missing: {module_root}")
    modules = sorted(module_root.rglob("*.so"))
    if not modules:
        raise ReleaseError(f"macOS Lua native modules are missing: {module_root}")
    for module in modules:
        if not module.is_file() or module.is_symlink():
            raise ReleaseError(f"macOS Lua native module is not a regular file: {module}")
        alias = module.with_suffix(".dylib")
        if _archive_exists(alias):
            raise ReleaseError(f"macOS Lua native module alias already exists: {alias}")
        # A sibling-relative link survives extraction and stays inside the release archive.
        # 指向同目录文件的相对链接在解压后仍有效，并始终处于发行包内。
        alias.symlink_to(module.name)


# _run_macos_linker_tool executes one native inspection or rewrite command with explicit errors.
# _run_macos_linker_tool 执行一次原生依赖检查或改写命令，并显式报告错误。
def _run_macos_linker_tool(arguments: list[str]) -> str:
    """Run a macOS linker tool and return stdout on success.
    运行 macOS 链接工具，并在成功时返回标准输出。

    Args:
        arguments: Executable and arguments without a shell.
    Returns:
        Command stdout.
    Raises:
        ReleaseError: If the tool is unavailable or exits unsuccessfully.
    """

    try:
        result = subprocess.run(arguments, capture_output=True, text=True, check=False)
    except OSError as error:
        raise ReleaseError(f"macOS linker tool is unavailable: {arguments[0]}: {error}") from error
    if result.returncode != 0:
        raise ReleaseError(f"macOS linker tool failed: {' '.join(arguments)}: {result.stderr.strip()}")
    return result.stdout


# _relocate_macos_libraries removes upstream CI paths from packaged Mach-O dependencies.
# _relocate_macos_libraries 从已打包 Mach-O 依赖中移除上游 CI 的绝对路径。
def _relocate_macos_libraries(lua_runtime: Path) -> None:
    """Bind packaged Mach-O modules and libraries to the bundled runtime lib directory.
    将已打包的 Mach-O 模块与动态库绑定到随包运行库目录。

    Args:
        lua_runtime: Staged lua_runtime root containing libs and lua_packages.
    Returns:
        None.
    Raises:
        ReleaseError: If an external dependency cannot map to a bundled library.
    """

    libs = lua_runtime / "libs"
    module_root = lua_runtime / "lua_packages" / "lib" / "lua"
    # The upstream curl dylib references an omitted Homebrew libssh2. Use Apple's stable curl ABI.
    # 上游 curl 动态库引用了未打包的 Homebrew libssh2，因此改用 Apple 稳定的 curl ABI。
    for curl_library in libs.glob("libcurl*.dylib"):
        if curl_library.is_symlink() or curl_library.is_file():
            curl_library.unlink()
    bundled_names = {entry.name for entry in libs.iterdir() if entry.is_file()}
    images = sorted(
        path for root in (libs, module_root) for path in root.rglob("*")
        if path.is_file() and not path.is_symlink() and path.suffix in {".dylib", ".so"}
    )
    if not images:
        raise ReleaseError(f"macOS runtime contains no Mach-O libraries: {lua_runtime}")
    for image in images:
        listing = _run_macos_linker_tool(["otool", "-L", str(image)])
        lines = listing.splitlines()
        if not lines or not lines[0].rstrip().endswith(":"):
            raise ReleaseError(f"unexpected otool dependency output for {image}")
        changed = False
        for dependency_index, line in enumerate(lines[1:]):
            dependency = line.strip().split(" (", 1)[0]
            if not dependency:
                continue
            if image.suffix == ".dylib" and dependency_index == 0:
                _run_macos_linker_tool([
                    "install_name_tool", "-id", f"@rpath/{image.name}", str(image),
                ])
                changed = True
                continue
            if dependency.startswith(("/usr/lib/", "/System/Library/")):
                continue
            if Path(dependency).name.startswith("libcurl."):
                _run_macos_linker_tool([
                    "install_name_tool", "-change", dependency,
                    "/usr/lib/libcurl.4.dylib", str(image),
                ])
                changed = True
                continue
            if dependency.startswith("@rpath/"):
                if Path(dependency).name not in bundled_names:
                    raise ReleaseError(f"unbundled macOS run-path dependency: {image}: {dependency}")
                continue
            if not dependency.startswith("/"):
                continue
            dependency_name = Path(dependency).name
            if dependency_name.startswith("libluajit"):
                dependency_name = "libluajit-5.1.dylib"
            if dependency_name not in bundled_names:
                raise ReleaseError(f"unbundled macOS absolute dependency: {image}: {dependency}")
            _run_macos_linker_tool([
                "install_name_tool", "-change", dependency,
                f"@rpath/{dependency_name}", str(image),
            ])
            changed = True
        if changed:
            # Re-sign only rewritten images after install_name_tool changes their load commands.
            # 仅对加载命令被改写的镜像重新签名，避免签名失效。
            _run_macos_linker_tool(["codesign", "--force", "--sign", "-", str(image)])


# _require_file returns a real file and gives missing assets one consistent error.
# _require_file 返回真实文件，并为缺失资源提供统一错误。
def _require_file(path: Path, label: str) -> Path:
    """
    Require one ordinary file.
    要求一个普通文件存在。

    Args:
        path: Candidate file path.
        label: Human-readable asset label.
    Returns:
        The original path when it is a regular non-symlink file.
    Raises:
        ReleaseError: If the path is missing, a directory, or a symlink.
    """

    if not path.is_file() or path.is_symlink():
        raise ReleaseError(f"required {label} is missing or not a regular file: {path}")
    return path


# _validate_windows_crt_dir validates the user-supplied MSVC redistributable directory.
# _validate_windows_crt_dir 校验用户提供的 MSVC 运行库目录。
def _validate_windows_crt_dir(path: Path) -> tuple[Path, ...]:
    """
    Return all DLL files from a safe MSVC CRT directory.
    返回安全 MSVC CRT 目录中的全部 DLL 文件。

    Args:
        path: Directory supplied by the Windows release runner.
    Returns:
        Sorted regular DLL files to copy into both executable directories.
    Raises:
        ReleaseError: If the directory is absent, unsafe, or lacks required CRT files.
    """

    if not path.is_dir() or path.is_symlink():
        raise ReleaseError(f"Windows CRT directory is missing or not a real directory: {path}")
    resolved_path = path.resolve()
    windows_root = Path(os.environ.get("WINDIR", r"C:\Windows"))
    system32 = (windows_root / "System32").resolve()
    if _is_relative_to(resolved_path, system32):
        raise ReleaseError(f"Windows CRT directory must not be under System32: {path}")
    # Check the Windows spelling as well when a Windows path is supplied on a non-Windows host.
    # 在非 Windows 主机上也检查 Windows 路径拼写，避免绕过 System32 限制。
    windows_text = str(path).replace("/", "\\").casefold()
    if "\\system32\\" in f"{windows_text}\\":
        raise ReleaseError(f"Windows CRT directory must not be under System32: {path}")

    dlls = tuple(
        sorted(
            (
                entry
                for entry in path.iterdir()
                if entry.is_file()
                and not entry.is_symlink()
                and entry.suffix.casefold() == ".dll"
            ),
            key=lambda entry: entry.name.casefold(),
        )
    )
    dll_names = {entry.name.casefold() for entry in dlls}
    required = {"vcruntime140.dll", "msvcp140.dll"}
    missing = sorted(required - dll_names)
    if missing:
        raise ReleaseError(f"Windows CRT directory is missing required DLLs: {', '.join(missing)}")
    return dlls


# _read_metadata reads only the Cargo package name and version used by release commands.
# _read_metadata 只读取发布命令使用的 Cargo 包名称和版本。
def _read_metadata(repository_root: Path) -> Metadata:
    """
    Read product metadata from Cargo.toml using Python's standard TOML parser.
    使用 Python 标准 TOML 解析器从 Cargo.toml 读取产品元数据。

    Args:
        repository_root: Repository root containing Cargo.toml.
    Returns:
        Parsed package name and version.
    Raises:
        ReleaseError: If Cargo metadata is absent or malformed.
    """

    cargo_path = _require_file(repository_root / "Cargo.toml", "Cargo.toml")
    try:
        document = tomllib.loads(cargo_path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        raise ReleaseError(f"failed to read Cargo.toml: {error}") from error

    package = document.get("package")
    if not isinstance(package, dict):
        raise ReleaseError("Cargo.toml must contain a [package] table")
    name = package.get("name")
    version = package.get("version")
    if not isinstance(name, str) or not name:
        raise ReleaseError("Cargo package name must be a non-empty string")
    if not isinstance(version, str) or not version:
        raise ReleaseError("Cargo package version must be a non-empty string")
    return Metadata(name=name, version=version)


# _validate_tag enforces the single tag spelling used by metadata and package commands.
# _validate_tag 强制 metadata 与 package 命令使用唯一的标签拼写。
def _validate_tag(version: str, tag: str | None) -> str:
    """
    Return the requested tag or the version-derived default after strict validation.
    严格校验后返回请求标签，未提供时返回由版本派生的默认标签。

    Args:
        version: Product version from Cargo.toml.
        tag: Optional caller-provided tag.
    Returns:
        Exactly ``v{version}``.
    Raises:
        ReleaseError: If the caller supplied a different tag.
    """

    expected = f"v{version}"
    if tag is not None and tag != expected:
        raise ReleaseError(f"tag must exactly match Cargo version: expected {expected}, got {tag}")
    return expected


# _validate_version prevents release metadata from creating unsafe output path names.
# _validate_version 防止发布元数据生成不安全的输出路径名称。
def _validate_version(version: str) -> str:
    """
    Validate the Cargo semantic version spelling used in archive names.
    校验归档名称使用的 Cargo 语义版本拼写。

    Args:
        version: Product version text.
    Returns:
        The original version text when safe.
    Raises:
        ReleaseError: If version is empty or is not a safe semantic version.
    """

    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?", version):
        raise ReleaseError(f"invalid release version: {version!r}")
    return version


# _target_spec resolves only the five release targets in the contract.
# _target_spec 只解析契约中规定的五个发布目标。
def _target_spec(target: str) -> TargetSpec:
    """
    Return the archive specification for one supported target triple.
    返回一个受支持 target triple 的归档规格。

    Args:
        target: Rust target triple.
    Returns:
        Immutable target specification.
    Raises:
        ReleaseError: If target is outside the release matrix.
    """

    try:
        return TARGET_SPECS[target]
    except KeyError as error:
        supported = ", ".join(TARGET_SPECS)
        raise ReleaseError(f"unsupported target {target!r}; expected one of: {supported}") from error


# _read_json_manifest decodes one runtime manifest without inventing alternate schemas.
# _read_json_manifest 解码一个运行时清单，不发明替代字段结构。
def _read_json_manifest(path: Path) -> dict[str, object]:
    """
    Read one UTF-8 JSON object.
    读取一个 UTF-8 JSON 对象。

    Args:
        path: Manifest file path.
    Returns:
        Decoded JSON object.
    Raises:
        ReleaseError: If the file is not a JSON object.
    """

    try:
        value = json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, json.JSONDecodeError) as error:
        raise ReleaseError(f"failed to parse runtime manifest {path}: {error}") from error
    if not isinstance(value, dict):
        raise ReleaseError(f"runtime manifest must be a JSON object: {path}")
    return value


# _validate_managed_runtimes checks real manifest fields and target ownership before copying.
# _validate_managed_runtimes 在复制前校验真实清单字段和目标归属。
def _validate_managed_runtimes(source_root: Path, platform: str) -> list[str]:
    """
    Validate every managed runtime manifest under the source root.
    校验来源根下的全部受管运行时清单。

    Args:
        source_root: Directory containing python and node runtime families.
        platform: Expected normalized target platform label.
    Returns:
        Sorted manifest paths relative to source_root for release traceability.
    Raises:
        ReleaseError: If a manifest is missing, malformed, mismatched, or incomplete.
    """

    if not source_root.is_dir() or source_root.is_symlink():
        raise ReleaseError(f"managed runtime directory is missing: {source_root}")
    _validate_tree(source_root)
    manifests = sorted(source_root.rglob("runtime-manifest.json"))
    if not manifests:
        raise ReleaseError(f"managed runtime manifests are missing: {source_root}")

    seen: dict[str, Path] = {}
    relative_names: list[str] = []
    for manifest_path in manifests:
        manifest = _read_json_manifest(manifest_path)
        required_fields = ("schema_version", "runtime", "version", "platform", "source", "executable")
        missing = [field for field in required_fields if field not in manifest]
        if missing:
            raise ReleaseError(f"runtime manifest missing {missing}: {manifest_path}")
        if manifest["schema_version"] != 1:
            raise ReleaseError(f"unsupported runtime manifest schema: {manifest_path}")

        runtime = manifest["runtime"]
        manifest_platform = manifest["platform"]
        if not isinstance(runtime, str) or not runtime:
            raise ReleaseError(f"runtime manifest runtime must be a non-empty string: {manifest_path}")
        if not isinstance(manifest_platform, str) or not manifest_platform:
            raise ReleaseError(f"runtime manifest platform must be a non-empty string: {manifest_path}")
        if manifest_platform not in {platform, "any"}:
            raise ReleaseError(
                f"runtime manifest platform mismatch: {manifest_path} declares {manifest_platform!r}, expected {platform!r} or 'any'"
            )
        if not isinstance(manifest["version"], str) or not manifest["version"]:
            raise ReleaseError(f"runtime manifest version must be a non-empty string: {manifest_path}")
        if not isinstance(manifest["source"], str) or not manifest["source"]:
            raise ReleaseError(f"runtime manifest source must be a non-empty string: {manifest_path}")

        executable = manifest["executable"]
        if not isinstance(executable, str) or not executable:
            raise ReleaseError(f"runtime manifest executable must be a non-empty string: {manifest_path}")
        executable_path = Path(executable)
        if (
            executable_path.is_absolute()
            or PureWindowsPath(executable).is_absolute()
            or bool(PureWindowsPath(executable).drive)
            or ".." in executable_path.parts
        ):
            raise ReleaseError(f"runtime manifest executable must stay under its install directory: {manifest_path}")
        candidate = manifest_path.parent / executable_path
        if not candidate.is_file():
            raise ReleaseError(f"runtime manifest executable is missing: {candidate}")
        if runtime in seen:
            raise ReleaseError(f"duplicate managed runtime manifest {runtime!r}: {manifest_path}")
        seen[runtime] = manifest_path
        relative_names.append(manifest_path.relative_to(source_root).as_posix())

    missing_runtime_names = REQUIRED_RUNTIME_NAMES.difference(seen)
    if missing_runtime_names:
        missing_text = ", ".join(sorted(missing_runtime_names))
        raise ReleaseError(f"required managed runtimes are missing: {missing_text}")
    return relative_names


# _run_managed_runtime_layout_check invokes the repository's authoritative validator.
# _run_managed_runtime_layout_check 调用仓库权威布局校验器。
def _run_managed_runtime_layout_check(
    repository_root: Path,
    runtime_root: Path,
    distribution_root: Path,
) -> None:
    """
    Run managed_runtime_layout_check.py against the staged runtime.
    对暂存运行时执行 managed_runtime_layout_check.py。

    Args:
        repository_root: Repository root containing the checker script.
        runtime_root: Staged lua_runtime root.
        distribution_root: Staged managed distribution directory.
    Raises:
        ReleaseError: If the checker is absent or returns a non-zero status.
    """

    checker = repository_root / "scripts" / "debug-tools" / "managed_runtime_layout_check.py"
    _require_file(checker, "managed runtime layout checker")
    environment_root = runtime_root / "dependencies" / "envs"
    command = [
        sys.executable,
        str(checker),
        str(runtime_root),
        "--distribution-root",
        str(distribution_root),
        "--environment-root",
        str(environment_root),
    ]
    completed = subprocess.run(
        command,
        cwd=repository_root,
        check=False,
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        details = (completed.stderr or completed.stdout).strip()
        raise ReleaseError(f"managed runtime layout check failed: {details}")


# _sha256 computes a streaming digest without loading release-sized assets into memory.
# _sha256 以流式方式计算摘要，避免将发布资产整体载入内存。
def _sha256(path: Path) -> str:
    """
    Return the lowercase SHA-256 digest of one regular file.
    返回一个普通文件的小写 SHA-256 摘要。

    Args:
        path: File to hash.
    Returns:
        A 64-character lowercase hexadecimal digest.
    """

    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


# _git_revision records the exact checkout commit used to build an archive.
# _git_revision 记录构建归档所使用 checkout 的确切提交。
def _git_revision(repository_root: Path) -> str:
    """
    Return the current Git commit hash for repository_root.
    返回 repository_root 当前 Git 提交的哈希。

    Args:
        repository_root: Git checkout containing the release.
    Returns:
        The commit hash printed by ``git rev-parse HEAD``.
    Raises:
        ReleaseError: If the checkout does not expose a valid commit.
    """

    completed = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=repository_root,
        check=False,
        capture_output=True,
        text=True,
    )
    revision = completed.stdout.strip()
    if completed.returncode != 0 or not revision:
        details = (completed.stderr or completed.stdout).strip()
        raise ReleaseError(f"unable to resolve checkout revision: {details}")
    return revision


# _write_json writes release metadata with stable key ordering and a final newline.
# _write_json 以稳定键顺序写入发布元数据，并保留末尾换行。
def _write_json(path: Path, value: object) -> None:
    """
    Write one UTF-8 JSON file.
    写入一个 UTF-8 JSON 文件。

    Args:
        path: Destination file.
        value: JSON-serializable value.
    Returns:
        None.
    """

    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


# _write_exclusive writes a text asset without overwriting an existing output.
# _write_exclusive 在不覆盖已有输出的前提下写入文本资产。
def _write_exclusive(path: Path, text: str) -> None:
    """
    Create one new text file exclusively.
    以独占方式创建一个新的文本文件。

    Args:
        path: Destination file.
        text: UTF-8 text content.
    Returns:
        None.
    Raises:
        ReleaseError: If the destination already exists.
    """

    try:
        with path.open("x", encoding="utf-8", newline="\n") as handle:
            handle.write(text)
    except FileExistsError as error:
        raise ReleaseError(f"refusing to overwrite existing output: {path}") from error


# _write_zip preserves Unix mode bits and stores symlinks as symlink entries.
# _write_zip 保留 Unix 权限位，并将符号链接写成符号链接条目。
def _write_zip(staging_root: Path, archive_path: Path, archive_prefix: str) -> None:
    """
    Create a zip archive from a validated staging tree.
    从已校验的暂存树创建 zip 归档。

    Args:
        staging_root: Root directory whose contents are archived.
        archive_path: Output zip path.
        archive_prefix: Required top-level directory name in the archive.
    Returns:
        None.
    """

    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        root_info = zipfile.ZipInfo(archive_prefix + "/")
        root_info.create_system = 3
        root_info.external_attr = ((stat.S_IFDIR | 0o755) << 16) | 0x10
        archive.writestr(root_info, b"")
        for path in sorted(staging_root.rglob("*"), key=lambda item: item.relative_to(staging_root).as_posix()):
            relative_name = path.relative_to(staging_root).as_posix()
            archive_name = f"{archive_prefix}/{relative_name}"
            info = zipfile.ZipInfo(archive_name + ("/" if path.is_dir() and not path.is_symlink() else ""))
            info.create_system = 3
            info.compress_type = zipfile.ZIP_DEFLATED
            mode = stat.S_IMODE(path.lstat().st_mode)
            if path.is_symlink():
                info.external_attr = ((stat.S_IFLNK | 0o777) << 16)
                archive.writestr(info, os.readlink(path))
            elif path.is_dir():
                info.external_attr = ((stat.S_IFDIR | mode) << 16) | 0x10
                archive.writestr(info, b"")
            else:
                info.external_attr = (stat.S_IFREG | mode) << 16
                with path.open("rb") as handle:
                    with archive.open(info, "w") as target:
                        shutil.copyfileobj(handle, target, length=1024 * 1024)


# _write_tar_gz preserves executable modes and validated relative symlinks.
# _write_tar_gz 保留可执行权限和已校验的相对符号链接。
def _write_tar_gz(staging_root: Path, archive_path: Path, archive_prefix: str) -> None:
    """
    Create a gzip-compressed tar archive from a validated staging tree.
    从已校验的暂存树创建 gzip 压缩 tar 归档。

    Args:
        staging_root: Root directory whose contents are archived.
        archive_path: Output tar.gz path.
        archive_prefix: Required top-level directory name in the archive.
    Returns:
        None.
    """

    with tarfile.open(archive_path, "w:gz") as archive:
        root_info = archive.gettarinfo(str(staging_root), arcname=archive_prefix)
        archive.addfile(root_info)
        paths = sorted(
            staging_root.rglob("*"),
            key=lambda item: item.relative_to(staging_root).as_posix(),
        )
        for path in paths:
            relative_name = path.relative_to(staging_root).as_posix()
            archive_name = f"{archive_prefix}/{relative_name}"
            info = archive.gettarinfo(str(path), arcname=archive_name)
            if path.is_file() and not path.is_symlink():
                with path.open("rb") as handle:
                    archive.addfile(info, handle)
            else:
                archive.addfile(info)


# _expected_archive_names builds the exact five-asset release matrix for verification.
# _expected_archive_names 构建校验使用的精确五资产发布矩阵。
def _expected_archive_names(version: str) -> tuple[str, ...]:
    """
    Return all expected archive basenames in target matrix order.
    返回目标矩阵顺序中的全部预期归档文件名。

    Args:
        version: Bare product version without a leading ``v``.
    Returns:
        Five archive basenames.
    Raises:
        ReleaseError: If version is empty or contains a path separator.
    """

    _validate_version(version)
    return tuple(
        f"{PRODUCT_NAME}-v{version}-{spec.platform}{spec.archive_suffix}"
        for spec in TARGET_SPECS.values()
    )


# _validate_staging_tree enforces the final package boundary before archiving.
# _validate_staging_tree 在归档前强制最终包边界。
def _validate_staging_tree(staging_root: Path) -> None:
    """
    Reject forbidden runtime data and unsafe links in final staging.
    拒绝最终暂存中的禁止运行时数据和不安全链接。

    Args:
        staging_root: Completed package staging root.
    Returns:
        None.
    Raises:
        ReleaseError: If forbidden content or an unsafe symlink exists.
    """

    _validate_tree(staging_root)


# package_release builds one target archive from existing build and dependency assets.
# package_release 使用已有构建产物和依赖资产构建一个目标归档。
def package_release(
    target: str,
    binary: Path,
    output_dir: Path = Path("dist"),
    tag: str | None = None,
    windows_crt_dir: Path | None = None,
    repository_root: Path = REPOSITORY_ROOT,
) -> Path:
    """
    Package one target into a zip or tar.gz archive and write its SHA-256 sidecar.
    将一个目标打包为 zip 或 tar.gz，并写出 SHA-256 sidecar。

    Args:
        target: Rust target triple from the supported release matrix.
        binary: Already compiled main executable.
        output_dir: Directory receiving the archive and sidecar.
        tag: Optional release tag, strictly checked against Cargo version.
        windows_crt_dir: MSVC CRT directory required for Windows targets.
        repository_root: Repository root containing release assets.
    Returns:
        The created archive path.
    Raises:
        ReleaseError: If any required asset or validation gate fails.
    """

    metadata = _read_metadata(repository_root)
    if metadata.name != PRODUCT_NAME:
        raise ReleaseError(f"Cargo package name must be {PRODUCT_NAME!r}, got {metadata.name!r}")
    _validate_version(metadata.version)
    resolved_tag = _validate_tag(metadata.version, tag)
    spec = _target_spec(target)
    if spec.archive_suffix == ".zip":
        if windows_crt_dir is None:
            raise ReleaseError("--windows-crt-dir is required for the Windows target")
        crt_dlls = _validate_windows_crt_dir(Path(windows_crt_dir))
    elif windows_crt_dir is not None:
        raise ReleaseError("--windows-crt-dir is only valid for the Windows target")
    else:
        crt_dlls = ()
    binary_path = Path(binary)
    if not binary_path.is_file() or binary_path.is_symlink():
        raise ReleaseError(f"compiled binary is missing or not a regular file: {binary_path}")

    output_root = Path(output_dir)
    if output_root.is_symlink():
        raise ReleaseError(f"output directory must not be a symbolic link: {output_root}")
    if output_root.exists() and not output_root.is_dir():
        raise ReleaseError(f"output directory is not a directory: {output_root}")
    output_root.mkdir(parents=True, exist_ok=True)
    archive_name = f"{metadata.name}-v{metadata.version}-{spec.platform}{spec.archive_suffix}"
    archive_path = output_root / archive_name
    sidecar_path = output_root / f"{archive_name}.sha256"
    if _archive_exists(archive_path) or _archive_exists(sidecar_path):
        raise ReleaseError(f"refusing to overwrite existing release output: {archive_name}")

    runtime_source = repository_root / "third_party" / "luaskills_runtime"
    managed_source = repository_root / "third_party" / "luaskills_managed_runtimes"
    controller_name = "vldb-controller.exe" if spec.archive_suffix == ".zip" else "vldb-controller"
    controller_source = repository_root / "third_party" / "vldb_controller" / "bin" / controller_name
    controller_source = _require_file(controller_source, "vldb-controller")
    runtime_manifests = _validate_managed_runtimes(managed_source, spec.platform)

    config_sources = [
        _require_file(repository_root / "configs" / name, f"configuration {name}")
        for name in REQUIRED_CONFIG_FILES
    ]
    license_source = _require_file(repository_root / "LICENSE", "LICENSE")
    readme_source = _require_file(repository_root / "README.md", "README.md")
    overflow_source = repository_root / "resources" / "overflow_templates"
    if not overflow_source.is_dir() or overflow_source.is_symlink():
        raise ReleaseError(f"required overflow template directory is missing: {overflow_source}")
    _validate_tree(overflow_source)
    if not any(overflow_source.iterdir()):
        raise ReleaseError(f"required overflow template directory is empty: {overflow_source}")

    archive_stem = archive_name[: -len(spec.archive_suffix)]
    source_revision = _git_revision(repository_root)
    with tempfile.TemporaryDirectory(prefix="v-") as temporary_root:
        # Keep the physical staging name short; archive_prefix retains the public release stem.
        # 保持物理暂存名称短小，由 archive_prefix 保留公开发布 stem。
        staging_root = Path(temporary_root) / "app"
        staging_root.mkdir()
        bin_destination = staging_root / "bin" / spec.binary_name
        bin_destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(binary_path, bin_destination)
        for crt_dll in crt_dlls:
            shutil.copy2(crt_dll, staging_root / "bin" / crt_dll.name)

        config_destination = staging_root / "configs"
        config_destination.mkdir()
        for config_source in config_sources:
            shutil.copy2(config_source, config_destination / config_source.name)

        lua_runtime = staging_root / "lua_runtime"
        for directory_name in ("libs", "lua_packages", "resources", "licenses"):
            source_directory = runtime_source / directory_name
            if not source_directory.is_dir() or source_directory.is_symlink():
                raise ReleaseError(f"required LuaSkills runtime directory is missing: {source_directory}")
            if not any(source_directory.iterdir()):
                raise ReleaseError(f"required LuaSkills runtime directory is empty: {source_directory}")
            _copy_tree_contents(source_directory, lua_runtime / directory_name)

        if spec.platform.startswith("macos-"):
            _add_macos_lua_module_aliases(lua_runtime / "lua_packages")
            if sys.platform == "darwin":
                _relocate_macos_libraries(lua_runtime)

        _copy_tree_contents(overflow_source, lua_runtime / "resources" / "overflow_templates")
        managed_destination = lua_runtime / "dependencies" / "runtimes"
        for manifest_name in runtime_manifests:
            manifest_path = managed_source / Path(manifest_name)
            install_directory = manifest_path.parent
            relative_install_directory = install_directory.relative_to(managed_source)
            _copy_entry(install_directory, managed_destination / relative_install_directory)
        _run_managed_runtime_layout_check(
            repository_root,
            lua_runtime,
            lua_runtime / "dependencies" / "runtimes",
        )
        _copy_entry(controller_source, lua_runtime / "bin" / controller_name)
        for crt_dll in crt_dlls:
            shutil.copy2(crt_dll, lua_runtime / "bin" / crt_dll.name)
        _copy_entry(license_source, staging_root / "LICENSE")
        _copy_entry(readme_source, staging_root / "README.md")

        release_manifest = {
            "schema_version": 1,
            "product_name": metadata.name,
            "version": metadata.version,
            "tag": resolved_tag,
            "target": target,
            "platform": spec.platform,
            "archive": archive_name,
            "contents": {
                "binary": f"bin/{spec.binary_name}",
                "configs": list(REQUIRED_CONFIG_FILES),
                "controller": f"lua_runtime/bin/{controller_name}",
                "windows_crt": [f"bin/{dll.name}" for dll in crt_dlls],
                "managed_runtime_root": "lua_runtime/dependencies/runtimes",
            },
            "traceability": {
                "binary_sha256": _sha256(binary_path),
                "managed_runtime_manifests": runtime_manifests,
                "managed_runtime_source": "third_party/luaskills_managed_runtimes",
                "source_revision": source_revision,
            },
        }
        _write_json(staging_root / "release-manifest.json", release_manifest)
        _validate_staging_tree(staging_root)

        if spec.archive_suffix == ".zip":
            _write_zip(staging_root, archive_path, archive_stem)
        else:
            _write_tar_gz(staging_root, archive_path, archive_stem)

    digest = _sha256(archive_path)
    _write_exclusive(sidecar_path, f"{digest}  {archive_name}\n")
    return archive_path


# verify_assets validates the exact five archives and creates SHA256SUMS.
# verify_assets 校验精确五个归档，并生成 SHA256SUMS。
def verify_assets(directory: Path, version: str) -> Path:
    """
    Validate release archive sidecars and write the aggregate checksum file.
    校验发布归档 sidecar，并写出聚合校验文件。

    Args:
        directory: Directory containing release archives and sidecars.
        version: Bare product version without a leading ``v``.
    Returns:
        The generated SHA256SUMS path.
    Raises:
        ReleaseError: If archives, sidecars, or checksums do not match exactly.
    """

    directory = Path(directory)
    if not directory.is_dir() or directory.is_symlink():
        raise ReleaseError(f"asset directory is missing: {directory}")
    expected_archives = set(_expected_archive_names(version))
    archive_paths = {
        path.name
        for path in directory.iterdir()
        if path.name.casefold().endswith(".zip") or path.name.casefold().endswith(".tar.gz")
    }
    if archive_paths != expected_archives:
        missing = sorted(expected_archives - archive_paths)
        extra = sorted(archive_paths - expected_archives)
        raise ReleaseError(f"release archive set mismatch: missing={missing}, extra={extra}")
    for archive_name in expected_archives:
        archive_path = directory / archive_name
        if not archive_path.is_file() or archive_path.is_symlink():
            raise ReleaseError(f"release archive is not a regular file: {archive_path}")

    expected_sidecars = {f"{name}.sha256" for name in expected_archives}
    actual_sidecars = {
        path.name
        for path in directory.iterdir()
        if path.name.endswith(".sha256")
    }
    if actual_sidecars != expected_sidecars:
        missing = sorted(expected_sidecars - actual_sidecars)
        extra = sorted(actual_sidecars - expected_sidecars)
        raise ReleaseError(f"release sidecar set mismatch: missing={missing}, extra={extra}")
    for sidecar_name in expected_sidecars:
        sidecar_path = directory / sidecar_name
        if not sidecar_path.is_file() or sidecar_path.is_symlink():
            raise ReleaseError(f"release sidecar is not a regular file: {sidecar_path}")

    digests: dict[str, str] = {}
    for archive_name in sorted(expected_archives):
        archive_path = directory / archive_name
        sidecar_path = directory / f"{archive_name}.sha256"
        sidecar_text = sidecar_path.read_text(encoding="utf-8")
        lines = sidecar_text.splitlines()
        if len(lines) != 1:
            raise ReleaseError(f"sidecar must contain one checksum line: {sidecar_path}")
        match = re.fullmatch(r"([0-9a-f]{64})  (.+)", lines[0])
        if match is None or match.group(2) != archive_name:
            raise ReleaseError(f"invalid sidecar format: {sidecar_path}")
        actual_digest = _sha256(archive_path)
        if match.group(1) != actual_digest:
            raise ReleaseError(f"checksum mismatch: {archive_name}")
        digests[archive_name] = actual_digest

    aggregate_path = directory / "SHA256SUMS"
    if _archive_exists(aggregate_path):
        raise ReleaseError(f"refusing to overwrite existing aggregate checksum file: {aggregate_path}")
    aggregate_text = "".join(f"{digests[name]}  {name}\n" for name in sorted(digests))
    _write_exclusive(aggregate_path, aggregate_text)
    return aggregate_path


# _append_github_output appends metadata outputs for GitHub Actions without truncating them.
# _append_github_output 追加 GitHub Actions 输出，并且不会截断已有内容。
def _append_github_output(path: Path, metadata: Metadata, tag: str) -> None:
    """
    Append version and tag keys to a GitHub output file.
    将 version 和 tag 键追加到 GitHub output 文件。

    Args:
        path: GitHub output file path.
        metadata: Parsed Cargo metadata.
        tag: Strictly validated release tag.
    Returns:
        None.
    Raises:
        ReleaseError: If the path is a directory or cannot be written.
    """

    if path.exists() and path.is_dir():
        raise ReleaseError(f"GitHub output path is a directory: {path}")
    try:
        with path.open("a", encoding="utf-8", newline="\n") as handle:
            handle.write(f"version={metadata.version}\n")
            handle.write(f"tag={tag}\n")
    except OSError as error:
        raise ReleaseError(f"failed to append GitHub output: {error}") from error


# _metadata_command implements the metadata CLI operation.
# _metadata_command 实现 metadata CLI 操作。
def _metadata_command(args: argparse.Namespace) -> int:
    """
    Print validated version and tag metadata.
    输出经过校验的版本和标签元数据。

    Args:
        args: Parsed command arguments.
    Returns:
        Zero on success.
    """

    metadata = _read_metadata(REPOSITORY_ROOT)
    tag = _validate_tag(metadata.version, args.tag)
    print(f"version={metadata.version}")
    print(f"tag={tag}")
    if args.github_output is not None:
        _append_github_output(Path(args.github_output), metadata, tag)
    return 0


# _package_command implements the package CLI operation.
# _package_command 实现 package CLI 操作。
def _package_command(args: argparse.Namespace) -> int:
    """
    Build one target archive and print its path.
    构建一个目标归档并输出其路径。

    Args:
        args: Parsed command arguments.
    Returns:
        Zero on success.
    """

    archive_path = package_release(
        target=args.target,
        binary=Path(args.binary),
        output_dir=Path(args.output_dir),
        tag=args.tag,
        windows_crt_dir=Path(args.windows_crt_dir) if args.windows_crt_dir is not None else None,
    )
    print(archive_path)
    return 0


# _verify_assets_command implements the verify-assets CLI operation.
# _verify_assets_command 实现 verify-assets CLI 操作。
def _verify_assets_command(args: argparse.Namespace) -> int:
    """
    Verify all release assets and print the aggregate checksum path.
    校验全部发布资产并输出聚合校验文件路径。

    Args:
        args: Parsed command arguments.
    Returns:
        Zero on success.
    """

    aggregate_path = verify_assets(Path(args.directory), args.version)
    print(aggregate_path)
    return 0


# _build_parser defines the three stable release CLI interfaces.
# _build_parser 定义三个稳定的发布 CLI 接口。
def _build_parser() -> argparse.ArgumentParser:
    """
    Build the command-line argument parser.
    构建命令行参数解析器。

    Returns:
        Configured argument parser.
    """

    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    metadata_parser = subparsers.add_parser("metadata", help="read and validate Cargo release metadata")
    metadata_parser.add_argument("--tag", default=None, help="release tag; must equal v{version}")
    metadata_parser.add_argument("--github-output", default=None, help="append version and tag to this file")
    metadata_parser.set_defaults(handler=_metadata_command)

    package_parser = subparsers.add_parser("package", help="package one compiled release target")
    package_parser.add_argument("--target", required=True, help="Rust target triple")
    package_parser.add_argument("--binary", required=True, help="already compiled main binary")
    package_parser.add_argument("--output-dir", default="dist", help="release output directory")
    package_parser.add_argument("--tag", default=None, help="release tag; must equal v{version}")
    package_parser.add_argument(
        "--windows-crt-dir",
        default=None,
        help="MSVC CRT redistributable directory required for the Windows target",
    )
    package_parser.set_defaults(handler=_package_command)

    verify_parser = subparsers.add_parser("verify-assets", help="verify the complete five-target release")
    verify_parser.add_argument("--directory", required=True, help="release asset directory")
    verify_parser.add_argument("--version", required=True, help="bare product version")
    verify_parser.set_defaults(handler=_verify_assets_command)
    return parser


# main converts deterministic release errors into a concise non-zero CLI result.
# main 将确定性发布错误转换为简洁的非零 CLI 结果。
def main(argv: Iterable[str] | None = None) -> int:
    """
    Parse arguments and dispatch one release operation.
    解析参数并分发一个发布操作。

    Args:
        argv: Optional argument sequence; defaults to sys.argv.
    Returns:
        Zero on success, one on validation or filesystem failure.
    """

    parser = _build_parser()
    args = parser.parse_args(list(argv) if argv is not None else None)
    try:
        return args.handler(args)
    except (OSError, ReleaseError, ValueError) as error:
        print(f"release error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
