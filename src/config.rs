use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

// ============================================================
// Configuration (loaded from YAML)
// ============================================================

#[derive(Deserialize, Debug, Default)]
pub struct Config {
    /// HTTP transport address, e.g. "0.0.0.0:3000"
    /// If absent, STDIO transport is used.
    pub http: Option<String>,

    /// LanceDb gRPC service endpoint, e.g. "http://localhost:50051"
    pub lancedb: Option<String>,

    /// Sqlite gRPC service endpoint, e.g. "http://localhost:50052"
    pub sqlite: Option<String>,

    /// VMM (VulcanMemoryMesh) gRPC service endpoint, e.g. "http://localhost:50053"
    pub vmm: Option<String>,
}

impl Config {
    /// Load configuration from the given path.
    pub fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    /// Load configuration with priority:
    /// 1. `-config` / `--config` CLI argument
    /// 2. `<exe_parent>/configs/config.yaml`
    /// 3. `configs/config.yaml` in current working directory
    /// If none found, exits with error (STDIO mode is not allowed).
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = find_config_arg().or_else(find_exe_parent_config).or_else(find_cwd_config);

        match config_path {
            Some(path) => {
                let config = Self::from_file(&path)?;
                eprintln!("[Config] Loaded from: {}", path);
                Ok(config)
            }
            None => {
                eprintln!("[Config] Error: No config file found.");
                eprintln!("[Config] Searched:");
                eprintln!("[Config]   - <exe_parent>/configs/config.yaml");
                eprintln!("[Config]   - ./configs/config.yaml");
                eprintln!("[Config] Provide config via -config flag or place it in the expected location.");
                std::process::exit(1);
            }
        }
    }
}

/// Look for -config or --config in argv.
fn find_config_arg() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    for i in 0..args.len() {
        if args[i] == "-config" || args[i] == "--config" {
            if i + 1 < args.len() {
                return Some(args[i + 1].clone());
            }
        }
    }
    None
}

/// Find configs/config.yaml in the parent directory of the running executable.
fn find_exe_parent_config() -> Option<String> {
    let exe_path = std::env::current_exe().ok()?;
    let exe_dir = exe_path.parent()?;
    // Look for configs/config.yaml relative to exe directory
    let config_path = exe_dir.join("configs").join("config.yaml");
    if config_path.exists() {
        Some(config_path.to_string_lossy().to_string())
    } else {
        None
    }
}

/// Find configs/config.yaml in current working directory.
fn find_cwd_config() -> Option<String> {
    let config_path = PathBuf::from("configs").join("config.yaml");
    if config_path.exists() {
        Some(config_path.to_string_lossy().to_string())
    } else {
        None
    }
}
