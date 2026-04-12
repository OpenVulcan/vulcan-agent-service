#[allow(dead_code)]
mod grpc_client;
mod http_server;
#[allow(dead_code)]
mod protocol;
mod server;
#[allow(dead_code)]
mod session;
mod config;

pub mod pb_lancedb {
    tonic::include_proto!("vldb.lancedb.v1");
}

pub mod pb_sqlite {
    tonic::include_proto!("vldb.sqlite.v1");
}

pub mod pb_vmm {
    tonic::include_proto!("vmm.v1");
}

use config::Config;
use server::McpServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = Config::load()?;

    let mut server = McpServer::new();

    // Connect gRPC clients if configured
    if let Some(endpoint) = &cfg.lancedb {
        server = server.with_lancedb(endpoint).await?;
    }
    if let Some(endpoint) = &cfg.sqlite {
        server = server.with_sqlite(endpoint).await?;
        // Auto-enable scratchpad when sqlite is available
        server = server.with_scratchpad_from_sqlite().await?;
    }
    if let Some(endpoint) = &cfg.vmm {
        server = server.with_vmm(endpoint).await?;
    }

    let addr = cfg.http.unwrap_or_else(|| "127.0.0.1:19201".to_string());
    http_server::run_http(server, &addr).await?;

    Ok(())
}
