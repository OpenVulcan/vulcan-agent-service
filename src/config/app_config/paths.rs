//! CLI and runtime-root path helpers for application config discovery.
//! 应用配置发现使用的 CLI 与运行根路径辅助函数。

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Result type used by config path discovery helpers.
/// 配置路径发现辅助函数使用的结果类型。
type ConfigPathResult<T> = Result<T, Box<dyn std::error::Error>>;

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
/// Parameters: `runtime_root` is the CLI-provided runtime root path.
/// 参数：`runtime_root` 是 CLI 传入的运行根路径。
/// Returns the config path when it exists, `None` when absent, or a path-resolution error.
/// 返回存在的配置路径、缺失时的 `None`，或路径解析错误。
pub(super) fn find_runtime_root_config(runtime_root: &str) -> ConfigPathResult<Option<String>> {
    let config_path = normalize_cli_runtime_root_arg(runtime_root)?
        .join("configs")
        .join("config.yaml");
    optional_config_file_path(&config_path, "runtime-root config path")
}

/// Normalize one CLI runtime-root argument so relative paths are anchored to the current working directory immediately.
/// 规范化一份 CLI runtime-root 参数，使相对路径立即锚定到当前工作目录。
/// Parameters: `runtime_root` is the raw CLI runtime-root argument.
/// 参数：`runtime_root` 是原始 CLI 运行根参数。
/// Returns an absolute path anchored to the current directory when needed.
/// 返回必要时锚定到当前目录的绝对路径。
pub(super) fn normalize_cli_runtime_root_arg(runtime_root: &str) -> ConfigPathResult<PathBuf> {
    normalize_cli_path_arg(runtime_root, "runtime-root")
}

/// Normalize one CLI config path so relative paths are anchored to the current working directory immediately.
/// 规范化一份 CLI 配置文件路径，使相对路径立即锚定到当前工作目录。
/// Parameters: `config_path` is the raw CLI config path argument.
/// 参数：`config_path` 是原始 CLI 配置文件路径参数。
/// Returns an absolute path anchored to the current directory when needed.
/// 返回必要时锚定到当前目录的绝对路径。
pub(super) fn normalize_cli_config_path(config_path: &str) -> ConfigPathResult<PathBuf> {
    normalize_cli_path_arg(config_path, "config path")
}

/// Normalize one CLI path argument against the current working directory.
/// 基于当前工作目录规范化一个 CLI 路径参数。
/// Parameters: `raw_path` is the raw CLI path argument.
/// 参数：`raw_path` 是原始 CLI 路径参数。
/// Parameters: `label` names the caller-specific path for error messages.
/// 参数：`label` 用于在错误消息中标识调用方路径。
/// Returns an absolute path anchored to the current directory when needed.
/// 返回必要时锚定到当前目录的绝对路径。
fn normalize_cli_path_arg(raw_path: &str, label: &str) -> ConfigPathResult<PathBuf> {
    let candidate_path = PathBuf::from(raw_path);
    if candidate_path.is_absolute() {
        Ok(candidate_path)
    } else {
        let cwd = std::env::current_dir()
            .map_err(|error| format!("failed to resolve current directory for {label}: {error}"))?;
        Ok(cwd.join(candidate_path))
    }
}

/// Find configs/config.yaml in the parent output directory of the running executable.
/// 在运行中可执行文件的上级输出目录中查找 configs/config.yaml。
/// The repository template lives in runtime/configs/config.yaml and is copied here during build.
/// 仓库模板文件位于 runtime/configs/config.yaml，构建后会复制到这里。
/// Returns the config path when it exists, `None` when absent, or an executable-path error.
/// 返回存在的配置路径、缺失时的 `None`，或可执行文件路径错误。
pub(super) fn find_exe_parent_config() -> ConfigPathResult<Option<String>> {
    let exe_path = std::env::current_exe()
        .map_err(|error| format!("failed to resolve current executable path: {error}"))?;
    find_exe_parent_config_from_exe_path(&exe_path)
}

/// Find configs/config.yaml from one already-resolved executable path.
/// 基于一份已经解析出的可执行文件路径查找 configs/config.yaml。
/// Parameters: `exe_path` is the resolved executable path.
/// 参数：`exe_path` 是已经解析出的可执行文件路径。
/// Returns the config path when the expected parent layout contains it, `None` when absent, or an inspection error.
/// 当预期上级目录布局中存在配置文件时返回配置路径，缺失时返回 `None`，或返回检查错误。
pub(super) fn find_exe_parent_config_from_exe_path(
    exe_path: &Path,
) -> ConfigPathResult<Option<String>> {
    let Some(exe_dir) = exe_path.parent() else {
        return Ok(None);
    };
    let Some(parent_dir) = exe_dir.parent() else {
        return Ok(None);
    };
    let config_path = parent_dir.join("configs").join("config.yaml");
    optional_config_file_path(&config_path, "executable-parent config path")
}

/// Inspect one optional application config path without hiding metadata or shape errors.
/// 检查一个可选应用配置路径，且不隐藏元数据或形态错误。
/// Parameters: `config_path` is the candidate config file path.
/// 参数：`config_path` 是候选配置文件路径。
/// Parameters: `path_label` names the discovery source in diagnostics.
/// 参数：`path_label` 用于在诊断中标识发现来源。
/// Returns the config path when it is a file, `None` when absent, or an inspection/shape error.
/// 当路径为文件时返回配置路径，缺失时返回 `None`，或返回检查/形态错误。
fn optional_config_file_path(
    config_path: &Path,
    path_label: &str,
) -> ConfigPathResult<Option<String>> {
    match std::fs::metadata(config_path) {
        Ok(metadata) => {
            if !metadata.is_file() {
                return Err(
                    format!("{path_label} is not a file: {}", config_path.display()).into(),
                );
            }
            Ok(Some(config_path.to_string_lossy().to_string()))
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "failed to inspect {path_label} {}: {}",
            config_path.display(),
            error
        )
        .into()),
    }
}
