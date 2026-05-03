//! CLI and runtime-root path helpers for application config discovery.
//! 应用配置发现使用的 CLI 与运行根路径辅助函数。

/// Reject the removed legacy `--config` entry so runtime configuration stays anchored to one runtime root.
/// 拒绝已移除的历史 `--config` 入口，从而让运行时配置始终锚定到唯一运行根。
pub(super) fn reject_legacy_config_flag(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.iter().any(|arg| is_removed_config_flag_arg(arg)) {
        return Err(
            "Unsupported CLI flag: -config/--config. Use --runtime-root and place config at <runtime_root>/configs/config.yaml.".into(),
        );
    }
    Ok(())
}

/// Return whether one raw argv token still uses the removed `--config` / `-config` CLI entry, including `--config=...` inline forms.
/// 返回某个原始 argv 片段是否仍在使用已移除的 `--config` / `-config` CLI 入口，包含 `--config=...` 内联写法。
fn is_removed_config_flag_arg(arg: &str) -> bool {
    arg == "-config"
        || arg == "--config"
        || arg.starts_with("-config=")
        || arg.starts_with("--config=")
}

/// Parse one CLI path flag from argv and fail early when the flag is missing a concrete value.
/// 从 argv 解析单个 CLI 路径标志，并在缺少实际取值时尽早失败。
pub(super) fn parse_cli_path_flag_from_args(
    args: &[String],
    flags: &[&str],
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    for i in 0..args.len() {
        if let Some((flag, value)) = parse_inline_cli_path_flag_value(args[i].as_str(), flags) {
            if value.is_empty() {
                return Err(format!("{flag} requires a value").into());
            }
            return Ok(Some(value.to_string()));
        }
        if flags.iter().any(|flag| args[i] == *flag) {
            let flag = args[i].as_str();
            let Some(value) = args.get(i + 1) else {
                return Err(format!("{flag} requires a value").into());
            };
            if value.starts_with("--") || value.starts_with('-') {
                return Err(format!("{flag} requires a value").into());
            }
            return Ok(Some(value.clone()));
        }
    }
    Ok(None)
}

/// Parse one inline `--flag=value` style CLI path token and return the matched canonical flag with its value.
/// 解析一条 `--flag=value` 风格的内联 CLI 路径参数，并返回匹配到的规范标志及其取值。
fn parse_inline_cli_path_flag_value<'a>(
    arg: &'a str,
    flags: &[&'a str],
) -> Option<(&'a str, &'a str)> {
    flags.iter().find_map(|flag| {
        arg.strip_prefix(flag)
            .and_then(|remainder| remainder.strip_prefix('='))
            .map(|value| (*flag, value))
    })
}

/// Resolve the config path under one explicit runtime root.
/// 从显式给定的运行根目录下解析配置文件路径。
pub(super) fn find_runtime_root_config(runtime_root: &str) -> Option<String> {
    let config_path = normalize_cli_runtime_root_arg(runtime_root)?
        .join("configs")
        .join("config.yaml");
    if config_path.exists() {
        Some(config_path.to_string_lossy().to_string())
    } else {
        None
    }
}

/// Normalize one CLI runtime-root argument so relative paths are anchored to the current working directory immediately.
/// 规范化一份 CLI runtime-root 参数，使相对路径立即锚定到当前工作目录。
pub(super) fn normalize_cli_runtime_root_arg(runtime_root: &str) -> Option<std::path::PathBuf> {
    let candidate_root = std::path::PathBuf::from(runtime_root);
    if candidate_root.is_absolute() {
        Some(candidate_root)
    } else {
        std::env::current_dir()
            .ok()
            .map(|cwd| cwd.join(candidate_root))
    }
}

/// Normalize one CLI config path so relative paths are anchored to the current working directory immediately.
/// 规范化一份 CLI 配置文件路径，使相对路径立即锚定到当前工作目录。
pub(super) fn normalize_cli_config_path(config_path: &str) -> Option<std::path::PathBuf> {
    let candidate_path = std::path::PathBuf::from(config_path);
    if candidate_path.is_absolute() {
        Some(candidate_path)
    } else {
        std::env::current_dir()
            .ok()
            .map(|cwd| cwd.join(candidate_path))
    }
}

/// Find configs/config.yaml in the parent output directory of the running executable.
/// 在运行中可执行文件的上级输出目录中查找 configs/config.yaml。
/// The repository template lives in runtime/configs/config.yaml and is copied here during build.
/// 仓库模板文件位于 runtime/configs/config.yaml，构建后会复制到这里。
pub(super) fn find_exe_parent_config() -> Option<String> {
    let exe_path = std::env::current_exe().ok()?;
    let exe_dir = exe_path.parent()?;
    let parent_dir = exe_dir.parent()?;
    let config_path = parent_dir.join("configs").join("config.yaml");
    if config_path.exists() {
        Some(config_path.to_string_lossy().to_string())
    } else {
        None
    }
}
