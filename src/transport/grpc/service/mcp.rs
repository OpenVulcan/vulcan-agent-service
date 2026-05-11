use super::*;

#[tonic::async_trait]
impl McpService for McpServiceImpl {
    async fn healthz(&self, _request: Request<()>) -> Result<Response<HealthzResponse>, Status> {
        let _uptime = self.start_time.elapsed().as_secs();
        Ok(Response::new(HealthzResponse {
            status: "ok".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: PROTOCOL_VERSION_LATEST.to_string(),
        }))
    }

    async fn call(
        &self,
        request: Request<McpCallRequest>,
    ) -> Result<Response<McpCallResponse>, Status> {
        let req = request.into_inner();
        eprintln!(
            "[gRPC] Call: method={} project_id={} user_id={} client_name={}",
            req.method, req.project_id, req.user_id, req.client_name
        );

        let request_context = build_mcp_call_request_context(&req);
        let (result, is_error, message) = self
            .dispatch_method(&req.method, &req.arguments, request_context)
            .await;

        Ok(Response::new(McpCallResponse {
            result,
            is_error,
            message,
        }))
    }

    type ConnectStream =
        std::pin::Pin<Box<dyn futures::Stream<Item = Result<ConnectEvent, Status>> + Send>>;

    async fn connect(
        &self,
        request: Request<ConnectRequest>,
    ) -> Result<Response<Self::ConnectStream>, Status> {
        let req = request.into_inner();
        let heartbeat_ms = if req.heartbeat_interval_ms > 0 {
            req.heartbeat_interval_ms as u64
        } else {
            30000
        };

        let (session_id, mut rx) = self.manager.register(&req.client_name).await;
        let manager = self.manager.clone();

        // Send welcome event
        let welcome = ConnectEvent {
            event: Some(ConnectEventType::Welcome(WelcomeEvent {
                server_version: env!("CARGO_PKG_VERSION").to_string(),
                session_id: session_id.clone(),
                protocol_version: PROTOCOL_VERSION_LATEST.to_string(),
            })),
        };

        let heartbeat_interval = std::time::Duration::from_millis(heartbeat_ms);
        let mut heartbeat_stream =
            tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(heartbeat_interval));

        let output = async_stream::stream! {
            // Send welcome first
            yield Ok(welcome);

            loop {
                tokio::select! {
                    // Heartbeat tick
                    _ = heartbeat_stream.next() => {
                        yield Ok(ConnectEvent {
                            event: Some(ConnectEventType::Heartbeat(HeartbeatEvent {
                                timestamp_ms: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_millis() as i64,
                                uptime_sec: 0,
                            })),
                        });
                    }
                    // Incoming call event from client (via separate channel)
                    event = rx.recv() => {
                        match event {
                            Some(evt) => yield Ok(evt),
                            None => {
                                eprintln!("[gRPC] Stream receiver dropped for {}", session_id);
                                break;
                            }
                        }
                    }
                }
            }

            // Cleanup on disconnect
            manager.unregister(&session_id).await;
        };

        Ok(Response::new(Box::pin(output) as Self::ConnectStream))
    }
}
