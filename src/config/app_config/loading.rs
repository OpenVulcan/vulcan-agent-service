//! Application configuration loading flow.
//! 应用配置加载流程。

use std::fs;

use super::paths::{
    find_exe_parent_config, find_runtime_root_config, normalize_cli_config_path,
    normalize_cli_runtime_root_arg, parse_cli_path_flag_from_args, reject_legacy_config_flag,
};
use super::types::Config;

impl Config {
    /// Load configuration from the given YAML file path.
    /// 从指定 YAML 文件路径加载配置。
    pub fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let normalized_path = normalize_cli_config_path(path).unwrap_or_else(|| path.into());
        let content = fs::read_to_string(&normalized_path)?;
        let mut config: Config = serde_yaml::from_str(&content)?;
        config.loaded_config_path = Some(normalized_path.to_string_lossy().to_string());
        Ok(config)
    }

    /// Load configuration strictly from the runtime-root layout or the built-in executable-side runtime layout.
    /// 严格从 runtime_root 目录布局或内置的可执行文件同级运行目录布局加载配置。
    /// The repository template lives at `runtime/configs/config.yaml` and is synced during build.
    /// 仓库内默认模板文件位于 `runtime/configs/config.yaml`，构建时会同步到输出目录。
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let args: Vec<String> = std::env::args().collect();
        reject_legacy_config_flag(&args)?;
        let runtime_root_arg =
            parse_cli_path_flag_from_args(&args, &["-runtime-root", "--runtime-root"])?;
        let config_path = if let Some(runtime_root) = runtime_root_arg.as_deref() {
            find_runtime_root_config(runtime_root)
        } else {
            find_exe_parent_config()
        };

        match config_path {
            Some(path) => {
                let mut config = Self::from_file(&path)?;
                if let Some(runtime_root) = runtime_root_arg
                    .as_deref()
                    .and_then(normalize_cli_runtime_root_arg)
                {
                    config.runtime_root = Some(runtime_root.to_string_lossy().to_string());
                }
                eprintln!("[Config] Loaded from: {}", path);
                Ok(config)
            }
            None => {
                eprintln!("[Config] Error: No config file found.");
                eprintln!("[Config] Searched:");
                if runtime_root_arg.is_some() {
                    eprintln!(
                        "[Config]   - -runtime-root/--runtime-root + <runtime_root>/configs/config.yaml"
                    );
                } else {
                    eprintln!("[Config]   - <exe_parent>/configs/config.yaml");
                }
                eprintln!("[Config] Template source in repository: runtime/configs/config.yaml");
                eprintln!(
                    "[Config] Provide config via --runtime-root and place config at <runtime_root>/configs/config.yaml, or place the built config file at <exe_parent>/configs/config.yaml."
                );
                std::process::exit(1);
            }
        }
    }
}
