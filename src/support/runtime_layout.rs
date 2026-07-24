use std::path::Path;

/// Fixed child directory that contains the complete LuaSkills runtime package.
/// 包含完整 LuaSkills 运行时包的固定子目录。
pub(crate) const LUASKILLS_RUNTIME_DIR_NAME: &str = "lua_runtime";

/// Resolve the stable hosted application root above an absolute `<application_root>/<binary_dir>/<executable>` path.
/// 从绝对的 `<application_root>/<binary_dir>/<executable>` 路径解析稳定宿主应用根。
/// Parameters: `executable_path` is the executable path supplied by runtime discovery.
/// 参数：`executable_path` 是运行时发现流程提供的可执行文件路径。
/// Returns the executable grandparent only for absolute paths, otherwise `None`.
/// 仅对绝对路径返回可执行文件的祖父目录，否则返回 `None`。
pub(crate) fn hosted_application_root_from_executable(executable_path: &Path) -> Option<&Path> {
    if !executable_path.is_absolute() {
        return None;
    }
    executable_path.parent()?.parent()
}

/// Derive the owned LuaSkills runtime root from one application root.
/// 从应用根目录推导其拥有的 LuaSkills 运行时根目录。
/// Parameters: `application_root` is the root containing host binaries, configs, and logs.
/// 参数：`application_root` 是包含宿主二进制、配置与日志的根目录。
/// Returns the fixed `<application_root>/lua_runtime` path without probing the filesystem.
/// 返回固定的 `<application_root>/lua_runtime` 路径，不执行文件系统探测。
pub(crate) fn luaskills_runtime_root(application_root: &Path) -> std::path::PathBuf {
    application_root.join(LUASKILLS_RUNTIME_DIR_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A basename-only relative executable cannot establish a stable hosted runtime root.
    /// 仅含文件名的相对可执行文件不能建立稳定的宿主运行根。
    #[test]
    fn hosted_application_root_rejects_relative_executable() {
        assert_eq!(
            hosted_application_root_from_executable(Path::new("host.exe")),
            None
        );
    }

    /// An absolute executable below `bin` resolves to its runtime-root grandparent.
    /// 位于 `bin` 下的绝对可执行文件应解析到其运行根祖父目录。
    #[test]
    fn hosted_application_root_returns_absolute_executable_grandparent() {
        // Build a synthetic absolute runtime root without touching the filesystem.
        // 构建一个不触碰文件系统的合成绝对运行根。
        let application_root = std::env::temp_dir().join("vulcan-hosted-application-root");
        // Place the synthetic executable under the standard runtime `bin` directory.
        // 将合成可执行文件放在标准运行时 `bin` 目录下。
        let executable_path = application_root.join("bin").join("host");

        assert_eq!(
            hosted_application_root_from_executable(&executable_path),
            Some(application_root.as_path())
        );
    }

    /// The LuaSkills root is always isolated under the application root.
    /// LuaSkills 根目录始终隔离在应用根目录之下。
    #[test]
    fn luaskills_runtime_root_uses_fixed_child_directory() {
        // Build one application root without requiring it to exist.
        // 构造一个无需实际存在的应用根目录。
        let application_root = std::env::temp_dir().join("vulcan-application-root");

        assert_eq!(
            luaskills_runtime_root(&application_root),
            application_root.join("lua_runtime")
        );
    }
}
