mod backends;
mod bootstrap;
mod config;
mod host_core;
#[path = "luaskills/mod.rs"]
mod luaskills_adapter;
mod model_provider;
mod support;
mod transport;

pub mod pb_vmm {
    tonic::include_proto!("vmm.v1");
}

pub mod pb_mcp {
    tonic::include_proto!("vulcan.mcp.v1");
}

/// Run the Vulcan host binary through the bootstrap entrypoint.
/// 通过 bootstrap 入口运行 Vulcan host 二进制。
fn main() -> Result<(), Box<dyn std::error::Error>> {
    bootstrap::run()
}
