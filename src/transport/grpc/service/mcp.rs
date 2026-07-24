use super::*;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Default gRPC connect heartbeat interval in milliseconds when the proto field is omitted.
/// proto 字段省略时使用的默认 gRPC connect 心跳间隔毫秒数。
const DEFAULT_HEARTBEAT_INTERVAL_MS: u64 = 30_000;

/// Default gRPC connect maximum lifetime in seconds when the proto field is omitted.
/// proto 字段省略时使用的默认 gRPC connect 最大生命周期秒数。
const DEFAULT_CONNECTION_TIMEOUT_SEC: u64 = 86_400;

#[tonic::async_trait]
impl McpService for McpServiceImpl {
    async fn healthz(&self, _request: Request<()>) -> Result<Response<HealthzResponse>, Status> {
        Ok(Response::new(HealthzResponse {
            status: "ok".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: PROTOCOL_VERSION_LATEST.to_string(),
            uptime_sec: duration_secs_to_i64(self.start_time.elapsed()),
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
        let heartbeat_interval = heartbeat_interval_duration(req.heartbeat_interval_ms)?;
        let connection_timeout = connection_timeout_duration(req.connection_timeout_sec)?;

        let (session_id, mut rx) = self.manager.register(&req.client_name).await;
        let manager = self.manager.clone();
        let start_time = self.start_time;

        // Send welcome event
        let welcome = ConnectEvent {
            event: Some(ConnectEventType::Welcome(WelcomeEvent {
                server_version: env!("CARGO_PKG_VERSION").to_string(),
                session_id: session_id.clone(),
                protocol_version: PROTOCOL_VERSION_LATEST.to_string(),
            })),
        };

        let mut heartbeat_stream =
            tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(heartbeat_interval));

        let output = async_stream::stream! {
            // Send welcome first
            yield Ok(welcome);
            let connection_timeout_sleep = tokio::time::sleep(connection_timeout);
            tokio::pin!(connection_timeout_sleep);

            loop {
                tokio::select! {
                    // Stop the stream at the negotiated maximum connection lifetime.
                    // 在协商出的最大连接生命周期到达时停止流。
                    _ = &mut connection_timeout_sleep => {
                        eprintln!("[gRPC] Connection timeout reached for {}", session_id);
                        break;
                    }
                    // Heartbeat tick
                    _ = heartbeat_stream.next() => {
                        match build_heartbeat_event(start_time, SystemTime::now()) {
                            Ok(event) => yield Ok(event),
                            Err(status) => {
                                yield Err(status);
                                break;
                            }
                        }
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

/// Build one gRPC connect heartbeat event from an explicit clock sample.
/// 使用明确的时钟采样构造一个 gRPC connect 心跳事件。
///
/// Parameters: `start_time` is the service start instant, and `now` is the wall-clock time used for the event timestamp.
/// 参数：`start_time` 是服务启动时刻，`now` 是用于事件时间戳的墙钟时间。
///
/// Returns: a heartbeat connect event, or a gRPC status when the wall clock cannot be represented.
/// 返回：心跳连接事件；当墙钟无法表示时返回 gRPC 状态。
fn build_heartbeat_event(start_time: Instant, now: SystemTime) -> Result<ConnectEvent, Status> {
    Ok(ConnectEvent {
        event: Some(ConnectEventType::Heartbeat(HeartbeatEvent {
            timestamp_ms: system_time_unix_millis(now)?,
            uptime_sec: duration_secs_to_i64(start_time.elapsed()),
        })),
    })
}

/// Convert one system time value into Unix epoch milliseconds for protobuf output.
/// 将一个系统时间值转换为 protobuf 输出使用的 Unix epoch 毫秒。
///
/// Parameters: `now` is the wall-clock timestamp to encode.
/// 参数：`now` 是需要编码的墙钟时间戳。
///
/// Returns: Unix epoch milliseconds, or a gRPC status when the timestamp is invalid for the protocol field.
/// 返回：Unix epoch 毫秒；当时间戳对协议字段无效时返回 gRPC 状态。
fn system_time_unix_millis(now: SystemTime) -> Result<i64, Status> {
    let duration = now.duration_since(UNIX_EPOCH).map_err(|error| {
        Status::internal(format!("System clock is before Unix epoch: {}", error))
    })?;
    i64::try_from(duration.as_millis())
        .map_err(|_| Status::internal("System clock timestamp exceeds i64 millisecond range"))
}

/// Convert a duration to signed seconds with explicit saturation at the protobuf field limit.
/// 将持续时间转换为有符号秒数，并在 protobuf 字段上限处显式饱和。
///
/// Parameters: `duration` is the elapsed service uptime.
/// 参数：`duration` 是已经过去的服务运行时间。
///
/// Returns: elapsed seconds capped at `i64::MAX`.
/// 返回：运行秒数，最大不超过 `i64::MAX`。
fn duration_secs_to_i64(duration: Duration) -> i64 {
    i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
}

/// Resolve the gRPC connect heartbeat interval from the raw proto field.
/// 从原始 proto 字段解析 gRPC connect 心跳间隔。
///
/// Parameters: `interval_ms` is the raw `heartbeat_interval_ms` value from `ConnectRequest`.
/// 参数：`interval_ms` 是 `ConnectRequest` 中原始的 `heartbeat_interval_ms` 值。
///
/// Returns: the heartbeat interval duration, or a gRPC status for invalid negative values.
/// 返回：心跳间隔持续时间；当值为无效负数时返回 gRPC 状态。
fn heartbeat_interval_duration(interval_ms: i32) -> Result<Duration, Status> {
    if interval_ms == 0 {
        return Ok(Duration::from_millis(DEFAULT_HEARTBEAT_INTERVAL_MS));
    }
    if interval_ms < 0 {
        return Err(Status::invalid_argument(
            "heartbeat_interval_ms must be positive or 0 for the default",
        ));
    }
    Ok(Duration::from_millis(interval_ms as u64))
}

/// Resolve the gRPC connect maximum lifetime from the raw proto field.
/// 从原始 proto 字段解析 gRPC connect 最大生命周期。
///
/// Parameters: `timeout_sec` is the raw `connection_timeout_sec` value from `ConnectRequest`.
/// 参数：`timeout_sec` 是 `ConnectRequest` 中原始的 `connection_timeout_sec` 值。
///
/// Returns: the maximum connection lifetime, or a gRPC status for invalid negative values.
/// 返回：最大连接生命周期；当值为无效负数时返回 gRPC 状态。
fn connection_timeout_duration(timeout_sec: i64) -> Result<Duration, Status> {
    if timeout_sec == 0 {
        return Ok(Duration::from_secs(DEFAULT_CONNECTION_TIMEOUT_SEC));
    }
    if timeout_sec < 0 {
        return Err(Status::invalid_argument(
            "connection_timeout_sec must be positive or 0 for the default",
        ));
    }
    Ok(Duration::from_secs(timeout_sec as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Heartbeat timestamps should reject wall-clock values before Unix epoch.
    /// 心跳时间戳应拒绝 Unix epoch 之前的墙钟时间。
    #[test]
    fn system_time_unix_millis_rejects_pre_epoch_time() {
        let pre_epoch = UNIX_EPOCH
            .checked_sub(Duration::from_millis(1))
            .expect("test should build a pre-epoch system time");

        let error = system_time_unix_millis(pre_epoch)
            .expect_err("pre-epoch timestamps should be rejected");

        assert_eq!(error.code(), tonic::Code::Internal);
        assert!(error.message().contains("before Unix epoch"));
    }

    /// Heartbeat events should carry the real Unix timestamp and elapsed service uptime.
    /// 心跳事件应携带真实 Unix 时间戳与已运行服务时长。
    #[test]
    fn build_heartbeat_event_uses_timestamp_and_uptime() {
        let start_time = Instant::now()
            .checked_sub(Duration::from_secs(5))
            .expect("test should build an earlier service start instant");
        let now = UNIX_EPOCH + Duration::from_millis(1_234);

        let event = build_heartbeat_event(start_time, now)
            .expect("valid heartbeat clock sample should build event");
        let heartbeat = match event.event {
            Some(ConnectEventType::Heartbeat(heartbeat)) => heartbeat,
            _ => panic!("heartbeat builder should produce a heartbeat event"),
        };

        assert_eq!(heartbeat.timestamp_ms, 1_234);
        assert!(heartbeat.uptime_sec >= 5);
    }

    /// Duration conversion should saturate instead of wrapping when seconds exceed the protobuf field range.
    /// 持续时间转换应在秒数超出 protobuf 字段范围时饱和，而不是回绕。
    #[test]
    fn duration_secs_to_i64_saturates_large_values() {
        let duration = Duration::from_secs(u64::MAX);

        assert_eq!(duration_secs_to_i64(duration), i64::MAX);
    }

    /// Healthz should expose service uptime instead of computing and discarding it.
    /// Healthz 应暴露服务运行时长，而不是计算后丢弃。
    #[tokio::test]
    async fn healthz_reports_elapsed_uptime() {
        let runtime = HostRuntime::new();
        let service = McpServiceImpl {
            runtime: runtime.clone(),
            dispatcher: McpDispatcher::new(runtime),
            manager: ConnectionManager::new(),
            start_time: Instant::now()
                .checked_sub(Duration::from_secs(7))
                .expect("test should build an earlier service start instant"),
        };

        let response = McpService::healthz(&service, Request::new(()))
            .await
            .expect("healthz should succeed")
            .into_inner();

        assert_eq!(response.status, "ok");
        assert_eq!(response.protocol_version, PROTOCOL_VERSION_LATEST);
        assert!(response.uptime_sec >= 7);
    }

    /// Heartbeat interval parsing should use the documented default when the proto field is omitted.
    /// 心跳间隔解析应在 proto 字段省略时使用文档约定的默认值。
    #[test]
    fn heartbeat_interval_duration_defaults_zero_to_thirty_seconds() {
        let interval = heartbeat_interval_duration(0)
            .expect("zero heartbeat interval should resolve to default");

        assert_eq!(
            interval,
            Duration::from_millis(DEFAULT_HEARTBEAT_INTERVAL_MS)
        );
    }

    /// Heartbeat interval parsing should preserve positive caller-provided intervals.
    /// 心跳间隔解析应保留调用方提供的正数间隔。
    #[test]
    fn heartbeat_interval_duration_accepts_positive_milliseconds() {
        let interval = heartbeat_interval_duration(250)
            .expect("positive heartbeat interval should be accepted");

        assert_eq!(interval, Duration::from_millis(250));
    }

    /// Heartbeat interval parsing should reject negative intervals instead of silently defaulting them.
    /// 心跳间隔解析应拒绝负数间隔，而不是静默使用默认值。
    #[test]
    fn heartbeat_interval_duration_rejects_negative_milliseconds() {
        let error = heartbeat_interval_duration(-1)
            .expect_err("negative heartbeat interval should be rejected");

        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        assert!(error.message().contains("heartbeat_interval_ms"));
    }

    /// Connection timeout parsing should use the documented one-day default when the proto field is omitted.
    /// 连接超时解析应在 proto 字段省略时使用文档约定的一天默认值。
    #[test]
    fn connection_timeout_duration_defaults_zero_to_one_day() {
        let timeout = connection_timeout_duration(0)
            .expect("zero connection timeout should resolve to default");

        assert_eq!(timeout, Duration::from_secs(DEFAULT_CONNECTION_TIMEOUT_SEC));
    }

    /// Connection timeout parsing should preserve positive caller-provided lifetimes.
    /// 连接超时解析应保留调用方提供的正数生命周期。
    #[test]
    fn connection_timeout_duration_accepts_positive_seconds() {
        let timeout = connection_timeout_duration(42)
            .expect("positive connection timeout should be accepted");

        assert_eq!(timeout, Duration::from_secs(42));
    }

    /// Connection timeout parsing should reject negative lifetimes instead of silently defaulting them.
    /// 连接超时解析应拒绝负数生命周期，而不是静默使用默认值。
    #[test]
    fn connection_timeout_duration_rejects_negative_seconds() {
        let error = connection_timeout_duration(-1)
            .expect_err("negative connection timeout should be rejected");

        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        assert!(error.message().contains("connection_timeout_sec"));
    }
}
