use tokio::sync::watch;

/// Wait until a shared transport shutdown receiver observes `true` or loses every sender.
/// 等待共享传输关闭接收器观察到 `true`，或所有发送端均已断开。
///
/// Parameters: `shutdown_rx` is the watch receiver carrying the latest shutdown-requested state.
/// 参数：`shutdown_rx` 是携带最新关闭请求状态的 watch 接收器。
///
/// Returns: this async function resolves when shutdown is requested or the channel can no longer receive updates.
/// 返回：当请求关闭或通道无法继续接收更新时，该异步函数结束。
pub(crate) async fn wait_for_shutdown_requested(shutdown_rx: &mut watch::Receiver<bool>) {
    loop {
        if *shutdown_rx.borrow_and_update() {
            return;
        }
        if shutdown_rx.changed().await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    /// Shutdown waiting should ignore `false` updates and complete only after an explicit `true`.
    /// 关闭等待应忽略 `false` 更新，并且只在显式 `true` 后完成。
    #[tokio::test]
    async fn wait_for_shutdown_requested_ignores_false_updates() {
        // Create one shutdown channel with the normal non-shutdown initial state.
        // 创建一条以正常未关闭状态为初始值的关闭通道。
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        // Spawn the waiter so the test can prove a false update does not resolve it.
        // 启动等待任务，以便测试证明 false 更新不会结束等待。
        let mut wait_task =
            tokio::spawn(async move { wait_for_shutdown_requested(&mut shutdown_rx).await });

        shutdown_tx
            .send(false)
            .expect("false shutdown update should be sendable");
        timeout(Duration::from_millis(20), &mut wait_task)
            .await
            .expect_err("false shutdown update should not complete the waiter");

        shutdown_tx
            .send(true)
            .expect("true shutdown update should be sendable");
        timeout(Duration::from_secs(1), wait_task)
            .await
            .expect("true shutdown update should complete the waiter")
            .expect("wait task should not panic");
    }

    /// Shutdown waiting should complete when all senders are dropped because no later true state can arrive.
    /// 当所有发送端断开且不可能再收到后续 true 状态时，关闭等待应完成。
    #[tokio::test]
    async fn wait_for_shutdown_requested_finishes_when_sender_closes() {
        // Create one shutdown channel that starts without a shutdown request.
        // 创建一条初始未请求关闭的关闭通道。
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        // Drop the sender to model transport ownership ending before an explicit shutdown value is sent.
        // 丢弃发送端，用于模拟传输所有权在显式关闭值发送前结束。
        drop(shutdown_tx);

        timeout(
            Duration::from_secs(1),
            wait_for_shutdown_requested(&mut shutdown_rx),
        )
        .await
        .expect("closed shutdown channel should complete the waiter");
    }
}
