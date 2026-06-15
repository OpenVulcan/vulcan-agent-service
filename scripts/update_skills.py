"""
Update managed LuaSkills from output/ and sync them back into runtime/.
从 output/ 更新受管 LuaSkills，并同步回 runtime/。
"""

from __future__ import annotations

import argparse
import ctypes
import json
import os
import platform
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Iterable


class FfiBorrowedBuffer(ctypes.Structure):
    """
    Borrowed byte-buffer view passed into LuaSkills JSON FFI requests.
    传入 LuaSkills JSON FFI 请求的借用字节缓冲视图。
    """

    _fields_ = [
        ("ptr", ctypes.POINTER(ctypes.c_uint8)),
        ("len", ctypes.c_size_t),
    ]


class FfiOwnedBuffer(ctypes.Structure):
    """
    Owned byte-buffer container returned by LuaSkills JSON FFI calls.
    由 LuaSkills JSON FFI 调用返回的拥有型字节缓冲容器。
    """

    _fields_ = [
        ("ptr", ctypes.POINTER(ctypes.c_uint8)),
        ("len", ctypes.c_size_t),
    ]


# DLL_DIRECTORY_HANDLES keeps Windows DLL search handles alive for the process lifetime.
# DLL_DIRECTORY_HANDLES 用于在进程生命周期内保持 Windows DLL 搜索句柄有效。
DLL_DIRECTORY_HANDLES: list[object] = []


def project_root() -> Path:
    """
    Resolve the MCP repository root from this script location.
    从当前脚本位置解析 MCP 仓库根目录。
    """

    return Path(__file__).resolve().parent.parent


def resolve_repo_relative_path(root: Path, value: str) -> Path:
    """
    Resolve relative command-line paths against the repository root.
    将相对命令行路径按仓库根目录解析。
    """

    path = Path(value).expanduser()
    if not path.is_absolute():
        path = root / path
    return path.resolve()


def normalized_path(path: Path) -> str:
    """
    Convert one filesystem path into a JSON-FFI-friendly string.
    将文件系统路径转换为适合 JSON FFI 的字符串。
    """

    return str(path.resolve()).replace("\\", "/")


def ensure_runtime_layout(root: Path) -> None:
    """
    Create the runtime directories required by LuaSkills lifecycle updates.
    创建 LuaSkills 生命周期更新所需的运行根目录。
    """

    for relative_path in [
        "skills",
        "dependencies",
        "state",
        "databases",
        "temp",
        "resources",
        "lua_packages",
        "bin/tools",
        "libs",
    ]:
        (root / relative_path).mkdir(parents=True, exist_ok=True)


def luaskills_library_patterns() -> list[str]:
    """
    Return platform-specific LuaSkills dynamic library glob patterns.
    返回当前平台对应的 LuaSkills 动态库匹配模式。
    """

    system = platform.system().lower()
    if system == "windows":
        return ["luaskills-*.dll", "vulcan_luaskills-*.dll", "luaskills.dll", "vulcan_luaskills.dll"]
    if system == "darwin":
        return ["libluaskills-*.dylib", "libvulcan_luaskills-*.dylib", "libluaskills.dylib", "libvulcan_luaskills.dylib"]
    return ["libluaskills-*.so", "libvulcan_luaskills-*.so", "libluaskills.so", "libvulcan_luaskills.so"]


def unique_paths(paths: Iterable[Path]) -> list[Path]:
    """
    Preserve path order while removing duplicates.
    在去重的同时保留路径顺序。
    """

    seen: set[Path] = set()
    ordered: list[Path] = []
    for path in paths:
        resolved = path.resolve()
        if resolved not in seen:
            seen.add(resolved)
            ordered.append(resolved)
    return ordered


def assert_path_within(root: Path, path: Path, description: str) -> None:
    """
    Ensure a filesystem path remains inside the expected root before destructive sync operations.
    在执行破坏性同步操作前，确保文件系统路径仍位于预期根目录内。
    """

    root_path = root.resolve()
    target_path = path.resolve()
    try:
        target_path.relative_to(root_path)
    except ValueError as error:
        raise RuntimeError(f"{description} is outside allowed root: {target_path}") from error


def candidate_library_paths(root: Path) -> list[Path]:
    """
    Build an ordered list of LuaSkills FFI library candidates from Cargo outputs.
    从 Cargo 产物构造 LuaSkills FFI 动态库候选列表。
    """

    candidates: list[Path] = []
    patterns = luaskills_library_patterns()
    for profile in ["release", "debug"]:
        for directory in [root / "target" / profile / "deps", root / "target" / profile]:
            if not directory.exists():
                continue
            for pattern in patterns:
                candidates.extend(directory.glob(pattern))
    candidates.sort(key=lambda path: path.stat().st_mtime, reverse=True)
    return unique_paths(path for path in candidates if path.is_file() and path.stat().st_size > 4096)


def resolve_library_path(root: Path, explicit_path: str | None, skip_build: bool) -> Path:
    """
    Resolve the LuaSkills FFI dynamic library, building the MCP crate when needed.
    解析 LuaSkills FFI 动态库，并在需要时构建 MCP crate。
    """

    if explicit_path:
        path = resolve_repo_relative_path(root, explicit_path)
        if not path.exists():
            raise RuntimeError(f"LUASKILLS_LIB does not exist: {path}")
        return path

    env_path = os.environ.get("LUASKILLS_LIB")
    if env_path:
        path = resolve_repo_relative_path(root, env_path)
        if not path.exists():
            raise RuntimeError(f"LUASKILLS_LIB does not exist: {path}")
        return path

    candidates = candidate_library_paths(root)
    if candidates:
        return candidates[0]

    if not skip_build:
        print("[update-skills] Building MCP debug target to produce LuaSkills FFI library")
        subprocess.run(["cargo", "build"], cwd=root, check=True)
        candidates = candidate_library_paths(root)
        if candidates:
            return candidates[0]

    raise RuntimeError(
        "Unable to find a usable LuaSkills FFI dynamic library. "
        "Run ./make.ps1 build or set LUASKILLS_LIB explicitly."
    )


def add_windows_dll_search_directories(paths: Iterable[Path]) -> None:
    """
    Add Windows DLL search directories so native dependencies resolve next to the selected library.
    添加 Windows DLL 搜索目录，使原生依赖可从所选动态库附近解析。
    """

    if platform.system().lower() != "windows" or not hasattr(os, "add_dll_directory"):
        return
    for path in unique_paths(directory for directory in paths if directory.exists()):
        DLL_DIRECTORY_HANDLES.append(os.add_dll_directory(str(path)))


def load_library(root: Path, library_path: Path, output_runtime_root: Path) -> ctypes.CDLL:
    """
    Load the LuaSkills dynamic library and configure shared JSON FFI signatures.
    加载 LuaSkills 动态库并配置共享 JSON FFI 签名。
    """

    add_windows_dll_search_directories(
        [
            library_path.parent,
            root / "output" / "libs",
            output_runtime_root / "libs",
            root / "third_party" / "luaskills_runtime" / "libs",
        ]
    )
    library = ctypes.CDLL(str(library_path))
    try:
        library.luaskills_ffi_buffer_free.argtypes = [FfiOwnedBuffer]
        library.luaskills_ffi_buffer_free.restype = None
    except AttributeError as error:
        raise RuntimeError(
            f"Selected library is not a LuaSkills JSON FFI library: {library_path}"
        ) from error
    return library


def decode_json_response(raw_buffer: FfiOwnedBuffer, library: ctypes.CDLL) -> dict:
    """
    Decode one JSON envelope returned by a LuaSkills JSON FFI function.
    解码 LuaSkills JSON FFI 函数返回的 JSON 包络。
    """

    if not raw_buffer.ptr and raw_buffer.len != 0:
        raise RuntimeError("FFI JSON call returned a null buffer with non-zero length")
    text = (
        ctypes.string_at(raw_buffer.ptr, raw_buffer.len).decode("utf-8")
        if raw_buffer.len
        else ""
    )
    library.luaskills_ffi_buffer_free(raw_buffer)
    payload = json.loads(text)
    if payload.get("ok") is not True:
        raise RuntimeError(payload.get("error") or "Unknown JSON FFI error")
    return payload.get("result") or {}


def call_json_ffi(library: ctypes.CDLL, function_name: str, payload: dict) -> dict:
    """
    Call one LuaSkills JSON FFI function with one JSON payload.
    使用一个 JSON 载荷调用单个 LuaSkills JSON FFI 函数。
    """

    ffi_function = getattr(library, function_name)
    ffi_function.argtypes = [FfiBorrowedBuffer]
    ffi_function.restype = FfiOwnedBuffer
    input_bytes = json.dumps(payload, separators=(",", ":")).encode("utf-8")
    input_array = (ctypes.c_uint8 * len(input_bytes)).from_buffer_copy(input_bytes)
    input_buffer = FfiBorrowedBuffer(
        ptr=ctypes.cast(input_array, ctypes.POINTER(ctypes.c_uint8)),
        len=len(input_bytes),
    )
    return decode_json_response(ffi_function(input_buffer), library)


def build_engine_options(runtime_root: Path) -> dict:
    """
    Build JSON engine options aligned with the MCP output runtime layout.
    构造与 MCP output 运行根布局对齐的 JSON 引擎选项。
    """

    return {
        "pool_config": {
            "min_size": 1,
            "max_size": 1,
            "idle_ttl_secs": 30,
        },
        "host_options": {
            "temp_dir": normalized_path(runtime_root / "temp"),
            "resources_dir": normalized_path(runtime_root / "resources"),
            "lua_packages_dir": normalized_path(runtime_root / "lua_packages"),
            "host_provided_tool_root": normalized_path(runtime_root / "bin" / "tools"),
            "host_provided_lua_root": normalized_path(runtime_root / "lua_packages"),
            "host_provided_ffi_root": normalized_path(runtime_root / "libs"),
            "system_lua_lib_dir": normalized_path(runtime_root / "system_lua_lib"),
            "download_cache_root": normalized_path(runtime_root / "temp" / "downloads"),
            "dependency_dir_name": "dependencies",
            "state_dir_name": "state",
            "database_dir_name": "databases",
            "skill_config_file_path": normalized_path(runtime_root / "configs" / "skill_config.json"),
            "allow_network_download": True,
            "github_base_url": None,
            "github_api_base_url": None,
            "official_skill_hub_base_url": None,
            "enable_private_url_skill_install": False,
            "private_skill_source_allowlist": [],
            "default_text_encoding": None,
            "sqlite_library_path": None,
            "sqlite_provider_mode": "dynamic_library",
            "sqlite_callback_mode": "standard",
            "lancedb_library_path": None,
            "lancedb_provider_mode": "dynamic_library",
            "lancedb_callback_mode": "standard",
            "space_controller": {
                "endpoint": None,
                "auto_spawn": False,
                "executable_path": None,
                "process_mode": "managed",
                "minimum_uptime_secs": 300,
                "idle_timeout_secs": 900,
                "default_lease_ttl_secs": 120,
                "connect_timeout_secs": 5,
                "startup_timeout_secs": 15,
                "startup_retry_interval_ms": 250,
                "lease_renew_interval_secs": 30,
            },
            "cache_config": None,
            "runlua_pool_config": None,
            "reserved_entry_names": [],
            "ignored_skill_ids": [],
            "capabilities": {
                "enable_skill_management_bridge": False,
                "enable_managed_io_compat": True,
            },
            "protection": {
                "protected_skill_ids": [],
            },
        },
    }


def discover_skill_ids(output_runtime_root: Path) -> list[str]:
    """
    Discover managed skill ids from output/state/installs records.
    从 output/state/installs 安装记录发现受管技能标识。
    """

    install_root = output_runtime_root / "state" / "installs"
    if not install_root.exists():
        return []
    return sorted(path.stem for path in install_root.glob("*.yaml") if path.is_file())


def unique_skill_ids(skill_ids: Iterable[str]) -> list[str]:
    """
    Preserve skill-id order while removing duplicates.
    在去重的同时保留技能标识顺序。
    """

    seen: set[str] = set()
    ordered: list[str] = []
    for skill_id in skill_ids:
        normalized_skill_id = skill_id.strip()
        if not normalized_skill_id:
            continue
        if "/" in normalized_skill_id or "\\" in normalized_skill_id or normalized_skill_id in {".", ".."}:
            raise RuntimeError(f"Invalid skill id: {skill_id}")
        if normalized_skill_id not in seen:
            seen.add(normalized_skill_id)
            ordered.append(normalized_skill_id)
    return ordered


def update_skill(library: ctypes.CDLL, engine_id: int, output_runtime_root: Path, skill_id: str) -> dict:
    """
    Update one managed skill through the system JSON FFI lifecycle surface.
    通过 system JSON FFI 生命周期接口更新单个受管技能。
    """

    root = {
        "name": "ROOT",
        "skills_dir": normalized_path(output_runtime_root / "skills"),
    }
    return call_json_ffi(
        library,
        "luaskills_ffi_system_update_skill_json",
        {
            "engine_id": engine_id,
            "authority": "system",
            "skill_roots": [root],
            "target_root": root,
            "request": {
                "skill_id": skill_id,
                "source_type": "github",
            },
        },
    )


def sync_directory(source: Path, destination: Path, guard_root: Path | None = None) -> None:
    """
    Replace one destination directory with one source directory.
    使用源目录替换目标目录。
    """

    if not source.exists():
        raise RuntimeError(f"Source directory does not exist: {source}")
    if guard_root is not None:
        assert_path_within(guard_root, destination, "Directory sync destination")
    if destination.exists():
        shutil.rmtree(destination)
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source, destination)


def sync_file(source: Path, destination: Path, guard_root: Path | None = None) -> None:
    """
    Replace one destination file with one source file.
    使用源文件替换目标文件。
    """

    if not source.exists():
        raise RuntimeError(f"Source file does not exist: {source}")
    if guard_root is not None:
        assert_path_within(guard_root, destination, "File sync destination")
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, destination)


def sync_skill_to_runtime(
    output_runtime_root: Path,
    target_runtime_root: Path,
    skill_id: str,
    sync_dependencies: bool,
) -> None:
    """
    Sync one updated skill directory and install record into runtime/.
    将单个已更新技能目录与安装记录同步到 runtime/。
    """

    sync_directory(
        output_runtime_root / "skills" / skill_id,
        target_runtime_root / "skills" / skill_id,
        guard_root=target_runtime_root,
    )
    sync_file(
        output_runtime_root / "state" / "installs" / f"{skill_id}.yaml",
        target_runtime_root / "state" / "installs" / f"{skill_id}.yaml",
        guard_root=target_runtime_root,
    )

    if not sync_dependencies:
        return
    for dependency_kind in ["tools", "lua", "ffi"]:
        source = output_runtime_root / "dependencies" / dependency_kind / skill_id
        if source.exists():
            sync_directory(
                source,
                target_runtime_root / "dependencies" / dependency_kind / skill_id,
                guard_root=target_runtime_root,
            )


def parse_args(argv: list[str]) -> argparse.Namespace:
    """
    Parse command-line options for the MCP skill update workflow.
    解析 MCP 技能更新工作流的命令行选项。
    """

    parser = argparse.ArgumentParser(
        description="Update managed LuaSkills from output/ and sync them into runtime/."
    )
    parser.add_argument(
        "--output-runtime-root",
        default="output",
        help="Runtime root used as the update staging area. Defaults to ./output.",
    )
    parser.add_argument(
        "--target-runtime-root",
        default="runtime",
        help="Runtime root that receives updated skills and install records. Defaults to ./runtime.",
    )
    parser.add_argument(
        "--skill-id",
        action="append",
        default=[],
        help="Skill id to update. Repeat to update multiple skills. Defaults to all install records.",
    )
    parser.add_argument(
        "--luaskills-lib",
        default=None,
        help="Explicit LuaSkills FFI dynamic library path. Defaults to the newest target/*/deps LuaSkills DLL.",
    )
    parser.add_argument(
        "--skip-build",
        action="store_true",
        help="Do not run cargo build when no LuaSkills FFI library is found.",
    )
    parser.add_argument(
        "--no-sync-dependencies",
        action="store_true",
        help="Only sync skills and state install records, leaving dependency directories unchanged.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print the resolved operation without updating or syncing files.",
    )
    parser.add_argument(
        "skill_ids",
        nargs="*",
        help="Optional positional skill ids for wrapper convenience.",
    )
    return parser.parse_args(argv)


def run(argv: list[str]) -> int:
    """
    Execute the full MCP output-to-runtime update workflow.
    执行完整的 MCP output 到 runtime 更新工作流。
    """

    args = parse_args(argv)
    root = project_root()
    output_runtime_root = resolve_repo_relative_path(root, args.output_runtime_root)
    target_runtime_root = resolve_repo_relative_path(root, args.target_runtime_root)

    requested_skill_ids = [*args.skill_id, *args.skill_ids]
    skill_ids = unique_skill_ids(requested_skill_ids or discover_skill_ids(output_runtime_root))
    if not skill_ids:
        raise RuntimeError(
            "No skill ids were provided and no install records were found under "
            f"{output_runtime_root / 'state' / 'installs'}"
        )

    print("[update-skills] Output runtime:", output_runtime_root)
    print("[update-skills] Target runtime:", target_runtime_root)
    print("[update-skills] Skills:", ", ".join(skill_ids))

    if args.dry_run:
        print("[update-skills] Dry run finished without file changes.")
        return 0

    ensure_runtime_layout(output_runtime_root)
    ensure_runtime_layout(target_runtime_root)

    library_path = resolve_library_path(root, args.luaskills_lib, args.skip_build)
    print("[update-skills] FFI library:", library_path)
    library = load_library(root, library_path, output_runtime_root)

    engine_result = call_json_ffi(
        library,
        "luaskills_ffi_engine_new_json",
        {"options": build_engine_options(output_runtime_root)},
    )
    engine_id = int(engine_result["engine_id"])

    try:
        roots_payload = {
            "engine_id": engine_id,
            "skill_roots": [
                {
                    "name": "ROOT",
                    "skills_dir": normalized_path(output_runtime_root / "skills"),
                }
            ],
        }
        call_json_ffi(library, "luaskills_ffi_load_from_roots_json", roots_payload)

        for skill_id in skill_ids:
            result = update_skill(library, engine_id, output_runtime_root, skill_id)
            status = result.get("status", "unknown")
            version = result.get("version") or "n/a"
            message = result.get("message") or ""
            print(f"[update-skills] {skill_id}: {status} (version={version}) {message}")
    finally:
        call_json_ffi(
            library,
            "luaskills_ffi_engine_free_json",
            {"engine_id": engine_id},
        )

    if output_runtime_root == target_runtime_root:
        print("[update-skills] Output and target runtime roots are identical; sync skipped.")
        return 0

    for skill_id in skill_ids:
        sync_skill_to_runtime(
            output_runtime_root,
            target_runtime_root,
            skill_id,
            sync_dependencies=not args.no_sync_dependencies,
        )
        print(f"[update-skills] Synced {skill_id} into target runtime.")

    return 0


def main() -> None:
    """
    Run the command-line entrypoint and map failures to exit code 1.
    运行命令行入口，并将失败映射为退出码 1。
    """

    try:
        raise SystemExit(run(sys.argv[1:]))
    except Exception as error:
        print(f"[update-skills] ERROR: {error}", file=sys.stderr)
        raise SystemExit(1)


if __name__ == "__main__":
    main()
