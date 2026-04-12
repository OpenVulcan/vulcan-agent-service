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

    /// Load configuration from the default location.
    /// Checks -config CLI arg first, then config.yaml next to the executable.
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        // Check if -config or --config was passed on command line
        let config_path = find_config_arg().or_else(find_default_config);

        match config_path {
            Some(path) => {
                let config = Self::from_file(&path)?;
                eprintln!("[Config] Loaded from: {}", path);
                Ok(config)
            }
            None => {
                eprintln!("[Config] No config file found, using defaults (STDIO transport)");
                Ok(Config::default())
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

/// Find config.yaml next to the running executable.
fn find_default_config() -> Option<String> {
    let exe_path = std::env::current_exe().ok()?;
    let exe_dir = exe_path.parent()?;
    let config_path = exe_dir.join("config.yaml");
    if config_path.exists() {
        Some(config_path.to_string_lossy().to_string())
    } else {
        // Also try current working directory
        let cwd_config = PathBuf::from("config.yaml");
        if cwd_config.exists() {
            Some(cwd_config.to_string_lossy().to_string())
        } else {
            None
        }
    }
}
