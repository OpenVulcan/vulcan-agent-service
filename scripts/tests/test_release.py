"""
Focused tests for the release packaging contract.
发布打包契约的针对性测试。
"""

from __future__ import annotations

import hashlib
import json
import os
import sys
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock


# ScriptRoot makes the release module importable when unittest runs from the repository root.
# ScriptRoot 让 unittest 从仓库根目录运行时可以导入 release 模块。
SCRIPT_ROOT = Path(__file__).resolve().parents[1]
if str(SCRIPT_ROOT) not in sys.path:
    sys.path.insert(0, str(SCRIPT_ROOT))

import release  # noqa: E402


# ReleaseFixture constructs a small repository-shaped asset tree for contract tests.
# ReleaseFixture 构造一个用于契约测试的小型仓库资产树。
class ReleaseFixture:
    """
    Build isolated release input assets without relying on checkout output directories.
    构造隔离的发布输入资产，不依赖 checkout 的 output 目录。
    """

    # __init__ creates the temporary repository root and its stable source files.
    # __init__ 创建临时仓库根目录及其稳定来源文件。
    def __init__(self, temporary_directory: tempfile.TemporaryDirectory[str], target: str):
        """
        Initialize one target-specific fixture repository.
        初始化一个目标专用的 fixture 仓库。

        Args:
            temporary_directory: Owner of the fixture temporary directory.
            target: Supported Rust target triple.
        Returns:
            None.
        """

        self.temporary_directory = temporary_directory
        self.root = Path(temporary_directory.name)
        self.target = target
        self.spec = release.TARGET_SPECS[target]
        self.platform = self.spec.platform
        self._write("Cargo.toml", '[package]\nname = "vulcan-agent-service"\nversion = "0.1.0"\nedition = "2024"\n')
        self._write("LICENSE", "license\n")
        self._write("README.md", "readme\n")
        for name in release.REQUIRED_CONFIG_FILES:
            self._write(f"configs/{name}", f"{name}\n")
        self._write("resources/overflow_templates/overflow_page.md", "page\n")
        for directory_name in ("libs", "lua_packages", "resources", "licenses"):
            self._write(f"third_party/luaskills_runtime/{directory_name}/asset.txt", f"{directory_name}\n")
        self._write(
            "third_party/luaskills_runtime/resources/base.txt",
            "runtime resource\n",
        )

        controller_name = "vldb-controller.exe" if self.spec.archive_suffix == ".zip" else "vldb-controller"
        self._write(f"third_party/vldb_controller/bin/{controller_name}", "controller\n", 0o751)
        self.binary = self._write(
            f"target/{self.spec.platform}/vulcan-agent-service{'.exe' if self.spec.archive_suffix == '.zip' else ''}",
            "agent\n",
            0o751,
        )
        self._create_managed_runtimes()
        self.crt_directory = self.root / "msvc-crt"
        if self.spec.archive_suffix == ".zip":
            self._write("msvc-crt/vcruntime140.dll", "crt\n")
            self._write("msvc-crt/msvcp140.dll", "crt\n")
            self._write("msvc-crt/concrt140.dll", "crt\n")

    # _write creates one fixture file and applies the requested mode.
    # _write 创建一个 fixture 文件并应用指定权限。
    def _write(self, relative_path: str, content: str, mode: int | None = None) -> Path:
        """
        Write one relative fixture file.
        写入一个相对 fixture 文件。

        Args:
            relative_path: Path relative to fixture root.
            content: UTF-8 file content.
            mode: Optional POSIX mode to apply.
        Returns:
            Created file path.
        """

        path = self.root / relative_path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
        if mode is not None:
            path.chmod(mode)
        return path

    # _create_managed_runtimes creates the four manifest entries required by the checker contract.
    # _create_managed_runtimes 创建布局校验契约要求的四个清单条目。
    def _create_managed_runtimes(self) -> None:
        """
        Create valid target-specific managed runtime manifests and entries.
        创建有效的目标专用受管运行时清单和入口。

        Returns:
            None.
        """

        entries = (
            ("python", f"uv-0.11.28-{self.platform}", "uv", "uv"),
            ("python", f"cpython-3.14.6-{self.platform}", "python", "python"),
            ("node", f"node-24.18.0-{self.platform}", "node", "node"),
            ("node", "pnpm-11.11.0", "pnpm", "pnpm.cjs"),
        )
        for family, directory_name, runtime, executable in entries:
            install_root = self.root / "third_party/luaskills_managed_runtimes" / family / directory_name
            executable_path = install_root / executable
            executable_path.parent.mkdir(parents=True, exist_ok=True)
            executable_path.write_text(runtime, encoding="utf-8")
            executable_path.chmod(0o755)
            manifest = {
                "schema_version": 1,
                "runtime": runtime,
                "version": {
                    "uv": "0.11.28",
                    "python": "3.14.6",
                    "node": "24.18.0",
                    "pnpm": "11.11.0",
                }[runtime],
                "platform": "any" if runtime == "pnpm" else self.platform,
                "source": "fixture",
                "executable": executable,
            }
            self._write(
                str((install_root / "runtime-manifest.json").relative_to(self.root)),
                json.dumps(manifest) + "\n",
            )


# ReleaseTests exercises metadata, packaging, archive, and five-asset verification behavior.
# ReleaseTests 覆盖 metadata、打包、归档和五资产校验行为。
class ReleaseTests(unittest.TestCase):
    """
    Verify the bounded release script behavior using isolated fixtures.
    使用隔离 fixture 校验限定发布脚本的行为。
    """

    # setUp creates one temporary directory retained until tearDown.
    # setUp 创建一个临时目录，并保留到 tearDown。
    def setUp(self) -> None:
        """
        Allocate the per-test temporary directory.
        分配每个测试使用的临时目录。

        Returns:
            None.
        """

        self.temporary_directory = tempfile.TemporaryDirectory()

    # tearDown releases the per-test temporary directory.
    # tearDown 释放每个测试使用的临时目录。
    def tearDown(self) -> None:
        """
        Remove fixture files created by the test.
        删除测试创建的 fixture 文件。

        Returns:
            None.
        """

        self.temporary_directory.cleanup()

    # _fixture creates a target-specific repository fixture.
    # _fixture 创建一个目标专用仓库 fixture。
    def _fixture(self, target: str = "x86_64-pc-windows-msvc") -> ReleaseFixture:
        """
        Return a fresh fixture for target.
        返回 target 对应的新 fixture。

        Args:
            target: Supported Rust target triple.
        Returns:
            Isolated release fixture.
        """

        return ReleaseFixture(self.temporary_directory, target)

    # _patched_package packages a fixture while replacing only external git and checker calls.
    # _patched_package 打包 fixture，仅替换外部 git 和校验器调用。
    def _patched_package(self, fixture: ReleaseFixture, **kwargs: object) -> Path:
        """
        Package one fixture with deterministic test hooks.
        使用确定性测试 hook 打包一个 fixture。

        Args:
            fixture: Fixture repository.
            kwargs: Optional package_release keyword overrides.
        Returns:
            Created archive path.
        """

        crt_directory = kwargs.pop(
            "windows_crt_dir",
            fixture.crt_directory if fixture.spec.archive_suffix == ".zip" else None,
        )
        with mock.patch.object(release, "_git_revision", return_value="fixture-commit"), mock.patch.object(
            release, "_run_managed_runtime_layout_check"
        ) as checker:
            result = release.package_release(
                target=fixture.target,
                binary=fixture.binary,
                output_dir=fixture.root / "dist",
                repository_root=fixture.root,
                windows_crt_dir=crt_directory,
                **kwargs,
            )
            checker.assert_called_once()
            return result

    # test_metadata_rejects_wrong_tag ensures version-derived tags are strict.
    # test_metadata_rejects_wrong_tag 确保版本派生标签严格匹配。
    def test_metadata_rejects_wrong_tag(self) -> None:
        """
        Reject a tag that differs from v0.1.0.
        拒绝不同于 v0.1.0 的标签。
        """

        fixture = self._fixture()
        metadata = release._read_metadata(fixture.root)
        self.assertEqual(metadata.version, "0.1.0")
        with self.assertRaises(release.ReleaseError):
            release._validate_tag(metadata.version, "v0.1.1")

    # test_metadata_appends_github_output verifies append rather than truncation.
    # test_metadata_appends_github_output 验证追加而不是截断。
    def test_metadata_appends_github_output(self) -> None:
        """
        Append version and tag keys to an existing GitHub output file.
        将版本和标签键追加到已有 GitHub output 文件。
        """

        fixture = self._fixture()
        output_path = fixture.root / "github-output.txt"
        output_path.write_text("existing=value\n", encoding="utf-8")
        metadata = release._read_metadata(fixture.root)
        release._append_github_output(output_path, metadata, "v0.1.0")
        self.assertEqual(
            output_path.read_text(encoding="utf-8"),
            "existing=value\nversion=0.1.0\ntag=v0.1.0\n",
        )

    # test_package_rejects_wrong_tag verifies package enforces the same metadata contract.
    # test_package_rejects_wrong_tag 验证 package 同样强制 metadata 契约。
    def test_package_rejects_wrong_tag(self) -> None:
        """
        Reject a mismatched package tag before writing output.
        在写出输出前拒绝不匹配的 package 标签。
        """

        fixture = self._fixture()
        with self.assertRaises(release.ReleaseError):
            release.package_release(
                target=fixture.target,
                binary=fixture.binary,
                output_dir=fixture.root / "dist",
                repository_root=fixture.root,
                tag="v9.9.9",
                windows_crt_dir=fixture.crt_directory,
            )

    # test_package_rejects_missing_resource verifies required runtime assets fail closed.
    # test_package_rejects_missing_resource 验证必要运行时资源缺失时安全失败。
    def test_package_rejects_missing_resource(self) -> None:
        """
        Reject a missing mandatory LuaSkills runtime directory.
        拒绝缺失的必需 LuaSkills 运行时目录。
        """

        fixture = self._fixture()
        shutil_path = fixture.root / "third_party/luaskills_runtime/libs"
        for child in shutil_path.iterdir():
            child.unlink()
        shutil_path.rmdir()
        with self.assertRaises(release.ReleaseError):
            self._patched_package(fixture)

    # test_package_rejects_platform_mismatch verifies manifest platform ownership.
    # test_package_rejects_platform_mismatch 验证清单平台归属校验。
    def test_package_rejects_platform_mismatch(self) -> None:
        """
        Reject one managed runtime manifest for another platform.
        拒绝属于另一平台的受管运行时清单。
        """

        fixture = self._fixture()
        manifest_path = next((fixture.root / "third_party/luaskills_managed_runtimes").rglob("runtime-manifest.json"))
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        manifest["platform"] = "linux-x64"
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        with self.assertRaises(release.ReleaseError):
            self._patched_package(fixture)

    # test_windows_package_copies_crt_and_excludes_runtime_data verifies Windows DLL placement and boundary.
    # test_windows_package_copies_crt_and_excludes_runtime_data 验证 Windows DLL 位置和运行时边界。
    def test_windows_package_copies_crt_and_excludes_runtime_data(self) -> None:
        """
        Copy every CRT DLL twice and keep forbidden runtime roots absent.
        将全部 CRT DLL 复制两份，并确保禁止运行时目录不存在。
        """

        fixture = self._fixture()
        archive_path = self._patched_package(fixture)
        with zipfile.ZipFile(archive_path) as archive:
            names = set(archive.namelist())
            prefix = "vulcan-agent-service-v0.1.0-windows-x64/"
            self.assertIn(prefix, names)
            self.assertIn(prefix + "bin/vcruntime140.dll", names)
            self.assertIn(prefix + "bin/msvcp140.dll", names)
            self.assertIn(prefix + "bin/concrt140.dll", names)
            self.assertIn(prefix + "lua_runtime/bin/vcruntime140.dll", names)
            self.assertIn(prefix + "lua_runtime/bin/msvcp140.dll", names)
            for name in names:
                self.assertFalse(any(part in release.FORBIDDEN_RUNTIME_COMPONENTS for part in Path(name).parts))

            manifest = json.loads(archive.read(prefix + "release-manifest.json"))
            self.assertEqual(manifest["target"], fixture.target)
            self.assertEqual(manifest["platform"], "windows-x64")
            self.assertEqual(manifest["version"], "0.1.0")
            self.assertEqual(manifest["traceability"]["source_revision"], "fixture-commit")

    # test_unix_archive_preserves_executable_mode verifies tar mode retention and stem prefix.
    # test_unix_archive_preserves_executable_mode 验证 tar 权限保留和 stem 前缀。
    @unittest.skipIf(os.name == "nt", "Windows filesystem does not expose POSIX executable bits")
    def test_unix_archive_preserves_executable_mode(self) -> None:
        """
        Preserve the executable mode of a Unix main binary in tar.gz.
        在 tar.gz 中保留 Unix 主程序的可执行权限。
        """

        fixture = self._fixture("x86_64-unknown-linux-gnu")
        archive_path = self._patched_package(fixture)
        self.assertTrue(archive_path.name.endswith("linux-x64.tar.gz"))
        with tarfile.open(archive_path, "r:gz") as archive:
            member = archive.getmember("vulcan-agent-service-v0.1.0-linux-x64/bin/vulcan-agent-service")
            self.assertEqual(member.mode & 0o777, 0o751)
            self.assertTrue(archive.getmember("vulcan-agent-service-v0.1.0-linux-x64").isdir())

    # test_macos_archive_aliases_upstream_native_modules verifies LuaSkills' dylib lookup contract.
    # test_macos_archive_aliases_upstream_native_modules 验证 LuaSkills 的 dylib 查找契约。
    @unittest.skipIf(os.name == "nt", "Windows fixtures cannot reliably create Unix symbolic links")
    def test_macos_archive_aliases_upstream_native_modules(self) -> None:
        """Keep official .so modules and add relative .dylib aliases for macOS.
        保留官方 .so 模块，并为 macOS 增加相对路径的 .dylib 别名。
        """

        fixture = self._fixture("aarch64-apple-darwin")
        fixture._write("third_party/luaskills_runtime/lua_packages/lib/lua/cjson.so", "native\n")
        archive_path = self._patched_package(fixture)
        prefix = "vulcan-agent-service-v0.1.0-macos-arm64/lua_runtime/lua_packages/lib/lua/"
        with tarfile.open(archive_path, "r:gz") as archive:
            self.assertTrue(archive.getmember(prefix + "cjson.so").isfile())
            alias = archive.getmember(prefix + "cjson.dylib")
            self.assertTrue(alias.issym())
            self.assertEqual(alias.linkname, "cjson.so")

    # test_macos_relocation_uses_system_curl verifies the incomplete bundled curl is not shipped.
    # test_macos_relocation_uses_system_curl 验证不发布依赖不完整的随包 curl。
    def test_macos_relocation_uses_system_curl(self) -> None:
        """Bind the Lua curl module to Apple's curl ABI and re-sign the changed image.
        将 Lua curl 模块绑定到 Apple curl ABI，并重新签名被修改的镜像。
        """

        root = Path(self.temporary_directory.name) / "lua_runtime"
        libs = root / "libs"
        modules = root / "lua_packages" / "lib" / "lua"
        libs.mkdir(parents=True)
        modules.mkdir(parents=True)
        for path in (libs / "libcurl.4.dylib", libs / "libssl.3.dylib", modules / "lcurl.so"):
            path.write_bytes(b"fixture")

        # The mocked otool listing reproduces the packaged Lua curl module dependency.
        # 模拟的 otool 列表复现已打包 Lua curl 模块的依赖。
        def tool_output(arguments: list[str]) -> str:
            """Return one fixture otool listing or acknowledge a rewrite command.
            返回一次 fixture 的 otool 列表，或确认一次改写命令。
            """

            if arguments[:2] != ["otool", "-L"]:
                return ""
            image = Path(arguments[2])
            dependencies = ""
            if image.suffix == ".dylib":
                dependencies += f"\t/usr/local/lib/{image.name} (compatibility version 1.0.0)\n"
            dependencies += "\t/usr/lib/libSystem.B.dylib (compatibility version 1.0.0)\n"
            if image.name == "lcurl.so":
                dependencies += (
                    "\t@rpath/libcurl.4.dylib (compatibility version 4.0.0)\n"
                )
            return f"{image}:\n{dependencies}"

        with mock.patch.object(release, "_run_macos_linker_tool", side_effect=tool_output) as tool:
            release._relocate_macos_libraries(root)
        commands = [call.args[0] for call in tool.call_args_list]
        self.assertIn(
            ["install_name_tool", "-change",
             "@rpath/libcurl.4.dylib", "/usr/lib/libcurl.4.dylib", str(modules / "lcurl.so")],
            commands,
        )
        self.assertIn(["codesign", "--force", "--sign", "-", str(modules / "lcurl.so")], commands)
        self.assertIn(
            ["install_name_tool", "-id", "@rpath/libssl.3.dylib", str(libs / "libssl.3.dylib")],
            commands,
        )
        self.assertFalse((libs / "libcurl.4.dylib").exists())

    # test_windows_requires_crt verifies that a Windows package cannot omit the CRT directory.
    # test_windows_requires_crt 验证 Windows 包不能省略 CRT 目录。
    def test_windows_requires_crt(self) -> None:
        """
        Require --windows-crt-dir for Windows packaging.
        Windows 打包必须提供 --windows-crt-dir。
        """

        fixture = self._fixture()
        with mock.patch.object(release, "_git_revision", return_value="fixture-commit"):
            with self.assertRaises(release.ReleaseError):
                release.package_release(
                    target=fixture.target,
                    binary=fixture.binary,
                    output_dir=fixture.root / "dist",
                    repository_root=fixture.root,
                )

    # test_unix_rejects_crt verifies the option is not accepted on Unix targets.
    # test_unix_rejects_crt 验证 Unix 目标不接受该参数。
    def test_unix_rejects_crt(self) -> None:
        """
        Reject a Windows CRT directory on a Unix target.
        在 Unix 目标上拒绝 Windows CRT 目录。
        """

        fixture = self._fixture("x86_64-unknown-linux-gnu")
        with self.assertRaises(release.ReleaseError):
            self._patched_package(fixture, windows_crt_dir=fixture.root)

    # test_verify_assets_checks_five_archives verifies exact archive and sidecar sets.
    # test_verify_assets_checks_five_archives 验证精确归档和 sidecar 集合。
    def test_verify_assets_checks_five_archives(self) -> None:
        """
        Verify five archives, five sidecars, and generated SHA256SUMS.
        校验五个归档、五个 sidecar 以及生成的 SHA256SUMS。
        """

        fixture = self._fixture()
        asset_directory = fixture.root / "assets"
        asset_directory.mkdir()
        archive_names = release._expected_archive_names("0.1.0")
        for index, archive_name in enumerate(archive_names):
            archive_path = asset_directory / archive_name
            archive_path.write_bytes(f"asset-{index}".encode("ascii"))
            digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
            (asset_directory / f"{archive_name}.sha256").write_text(
                f"{digest}  {archive_name}\n",
                encoding="utf-8",
            )
        aggregate = release.verify_assets(asset_directory, "0.1.0")
        self.assertEqual(aggregate.name, "SHA256SUMS")
        self.assertEqual(len(aggregate.read_text(encoding="utf-8").splitlines()), 5)

        extra = asset_directory / "extra.zip"
        extra.write_bytes(b"extra")
        with self.assertRaises(release.ReleaseError):
            release.verify_assets(asset_directory, "0.1.0")

    # test_verify_assets_rejects_bad_checksum verifies sidecar digest mismatch.
    # test_verify_assets_rejects_bad_checksum 验证 sidecar 摘要不匹配时失败。
    def test_verify_assets_rejects_bad_checksum(self) -> None:
        """
        Reject one incorrect sidecar checksum.
        拒绝一个错误的 sidecar 校验和。
        """

        fixture = self._fixture()
        asset_directory = fixture.root / "assets"
        asset_directory.mkdir()
        for archive_name in release._expected_archive_names("0.1.0"):
            archive_path = asset_directory / archive_name
            archive_path.write_bytes(b"asset")
            (asset_directory / f"{archive_name}.sha256").write_text(
                f"{'0' * 64}  {archive_name}\n",
                encoding="utf-8",
            )
        with self.assertRaises(release.ReleaseError):
            release.verify_assets(asset_directory, "0.1.0")


if __name__ == "__main__":
    unittest.main()
