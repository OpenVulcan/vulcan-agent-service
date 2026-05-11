use super::*;

// gRPC server runner
// ============================================================

pub async fn run_grpc(server: HostRuntime, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);
    run_grpc_with_shutdown(server, addr, shutdown_rx).await
}

/// Run the gRPC transport until the supplied shutdown receiver is triggered.
/// 运行 gRPC 传输层，直到提供的关闭接收器被触发。
pub async fn run_grpc_with_shutdown(
    server: HostRuntime,
    addr: &str,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<(), Box<dyn std::error::Error>> {
    let addr: SocketAddr = addr.parse()?;
    let manager = ConnectionManager::new();
    let service = McpServiceImpl::new(server, manager);

    eprintln!("[gRPC] Starting gRPC server on http://{} ...", addr);
    eprintln!("[gRPC]   Healthz    Healthz");
    eprintln!("[gRPC]   Call       Unary tool/method invocation");
    eprintln!("[gRPC]   Connect    Long-lived streaming connection with heartbeat");
    eprintln!("[gRPC]   LuaSkills Stable LuaSkills tool and management API");
    eprintln!("[gRPC]   VMM        VMM service relay API");

    Server::builder()
        .add_service(McpServiceServer::new(service.clone()))
        .add_service(LuaSkillsServiceServer::new(service.clone()))
        .add_service(HostAdapterServiceServer::new(service.clone()))
        .add_service(VmmServiceServer::new(service))
        .serve_with_shutdown(addr, async move {
            let _ = shutdown_rx.changed().await;
        })
        .await?;

    Ok(())
}
