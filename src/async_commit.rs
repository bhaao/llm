//! 异步提交服务 - Channel + 批处理上链
//!
//! **架构定位**：
//! - 真正的异步上链，不阻塞推理主流程
//! - 批处理提升吞吐量
//! - 超时自动提交
//!
//! **核心特性**：
//! - 使用 tokio channel 进行异步通信
//! - 按批次大小或超时自动触发提交
//! - 支持背压（backpressure）控制

use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{interval, timeout};
use tokio::select;
use serde::{Serialize, Deserialize};
use log::{info, debug};

use crate::block::KvCacheProof;
use crate::metadata::BlockMetadata;

/// 提交请求
#[derive(Debug, Serialize, Deserialize)]
pub struct CommitRequest {
    /// 请求 ID
    pub request_id: String,
    /// 节点 ID
    pub node_id: String,
    /// 推理输出
    pub output: String,
    /// KV Cache 存证
    pub kv_proof: KvCacheProof,
    /// 元数据
    pub metadata: BlockMetadata,
    /// 预期 token 数
    pub expected_tokens: Option<u64>,
    /// 响应通道
    #[serde(skip)]
    pub response_tx: Option<oneshot::Sender<CommitResult>>,
}

/// 提交结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitResult {
    /// 是否成功
    pub success: bool,
    /// 区块高度
    pub block_height: Option<u64>,
    /// 区块哈希
    pub block_hash: Option<String>,
    /// 错误信息
    pub error: Option<String>,
    /// 处理延迟（毫秒）
    pub latency_ms: u64,
}

impl CommitResult {
    /// 创建成功结果
    pub fn success(block_height: u64, block_hash: String, latency_ms: u64) -> Self {
        CommitResult {
            success: true,
            block_height: Some(block_height),
            block_hash: Some(block_hash),
            error: None,
            latency_ms,
        }
    }

    /// 创建失败结果
    pub fn failure(error: String) -> Self {
        CommitResult {
            success: false,
            block_height: None,
            block_hash: None,
            error: Some(error),
            latency_ms: 0,
        }
    }
}

/// 异步提交服务配置
#[derive(Debug, Clone)]
pub struct AsyncCommitConfig {
    /// 批处理大小
    pub batch_size: usize,
    /// 批处理超时（毫秒）
    pub batch_timeout_ms: u64,
    /// 通道缓冲区大小
    pub channel_buffer_size: usize,
    /// 提交超时（毫秒）
    pub commit_timeout_ms: u64,
}

impl Default for AsyncCommitConfig {
    fn default() -> Self {
        AsyncCommitConfig {
            batch_size: 10,
            batch_timeout_ms: 1000,
            channel_buffer_size: 100,
            commit_timeout_ms: 5000,
        }
    }
}

/// 异步提交服务
pub struct AsyncCommitService {
    /// 发送端
    tx: mpsc::Sender<CommitRequest>,
    /// 接收端（内部使用）
    rx: Option<mpsc::Receiver<CommitRequest>>,
    /// 配置
    config: AsyncCommitConfig,
    /// 运行状态
    running: bool,
}

impl AsyncCommitService {
    /// 创建新的异步提交服务
    pub fn new(config: AsyncCommitConfig) -> Self {
        let (tx, rx) = mpsc::channel(config.channel_buffer_size);

        AsyncCommitService {
            tx,
            rx: Some(rx),
            config,
            running: false,
        }
    }

    /// 获取发送端（用于外部提交请求）
    pub fn sender(&self) -> mpsc::Sender<CommitRequest> {
        self.tx.clone()
    }

    /// 启动服务（在后台运行）
    pub async fn run(&mut self) -> Result<(), String> {
        if self.running {
            return Err("Service already running".to_string());
        }

        let rx = self.rx.take().ok_or("Receiver already taken")?;
        self.running = true;

        self.run_loop(rx).await;

        Ok(())
    }

    /// 运行主循环
    async fn run_loop(&mut self, mut rx: mpsc::Receiver<CommitRequest>) {
        let mut batch = Vec::new();
        let batch_timeout_ms = self.config.batch_timeout_ms;
        let batch_size = self.config.batch_size;
        let mut batch_timer = interval(Duration::from_millis(batch_timeout_ms));
        batch_timer.tick().await; // 立即跳过第一次 tick

        info!(
            "AsyncCommitService started: batch_size={}, timeout={}ms",
            batch_size,
            batch_timeout_ms
        );

        loop {
            select! {
                request = rx.recv() => {
                    match request {
                        Some(req) => {
                            batch.push(req);
                            debug!("Received commit request, batch size: {}", batch.len());

                            // 达到批处理大小，立即提交
                            if batch.len() >= batch_size {
                                self.commit_batch(batch).await;
                                batch = Vec::new();
                                batch_timer.reset();
                            }
                        }
                        None => {
                            // 通道关闭，处理剩余请求
                            if !batch.is_empty() {
                                self.commit_batch(batch).await;
                            }
                            info!("AsyncCommitService stopped: channel closed");
                            break;
                        }
                    }
                }
                _ = batch_timer.tick() => {
                    // 超时，提交当前批次
                    if !batch.is_empty() {
                        debug!("Batch timeout, committing {} requests", batch.len());
                        self.commit_batch(batch).await;
                        batch = Vec::new();
                    }
                }
            }
        }
    }

    /// 提交批次
    async fn commit_batch(&self, batch: Vec<CommitRequest>) {
        if batch.is_empty() {
            return;
        }

        info!("Committing batch of {} requests", batch.len());

        // 实际实现中，这里会：
        // 1. 批量构建区块
        // 2. 调用 PBFT 共识
        // 3. 并行写入记忆链
        // 4. 返回结果给客户端

        for mut request in batch {
            let start_time = std::time::Instant::now();

            // 模拟提交处理
            let result = self.process_request(&request).await;

            let latency = start_time.elapsed().as_millis() as u64;

            // 发送结果
            if let Some(tx) = request.response_tx.take() {
                let mut commit_result = result;
                commit_result.latency_ms = latency;

                let _ = tx.send(commit_result);
            }
        }
    }

    /// 处理单个请求
    async fn process_request(&self, _request: &CommitRequest) -> CommitResult {
        // 模拟处理延迟
        tokio::time::sleep(Duration::from_millis(10)).await;

        // 实际实现中，这里会调用区块链的 commit 方法
        // 这里仅返回模拟结果
        CommitResult::success(
            100, // 模拟区块高度
            "simulated_block_hash".to_string(),
            0,
        )
    }

    /// 提交请求（带超时）
    pub async fn submit_with_timeout(
        &self,
        request: CommitRequest,
    ) -> Result<CommitResult, String> {
        let (tx, rx) = oneshot::channel();

        let mut request = request;
        request.response_tx = Some(tx);

        // 发送请求
        self.tx.send(request)
            .await
            .map_err(|e| format!("Failed to send request: {}", e))?;

        // 等待结果（带超时）
        match timeout(
            Duration::from_millis(self.config.commit_timeout_ms),
            rx
        ).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(e)) => Err(format!("Channel error: {}", e)),
            Err(_) => Err(format!(
                "Commit timeout after {}ms",
                self.config.commit_timeout_ms
            )),
        }
    }

    /// 停止服务
    pub fn stop(&mut self) {
        self.running = false;
    }

    /// 检查是否正在运行
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// 获取统计信息
    pub fn stats(&self) -> AsyncCommitStats {
        AsyncCommitStats {
            batch_size: self.config.batch_size,
            batch_timeout_ms: self.config.batch_timeout_ms,
            buffer_size: self.config.channel_buffer_size,
            running: self.running,
        }
    }
}

/// 异步提交统计信息
#[derive(Debug, Clone, Default)]
pub struct AsyncCommitStats {
    /// 批处理大小
    pub batch_size: usize,
    /// 批处理超时（毫秒）
    pub batch_timeout_ms: u64,
    /// 缓冲区大小
    pub buffer_size: usize,
    /// 是否正在运行
    pub running: bool,
}

/// 提交服务句柄 - 用于外部访问
#[derive(Clone)]
pub struct CommitServiceHandle {
    tx: mpsc::Sender<CommitRequest>,
    commit_timeout_ms: u64,
}

impl CommitServiceHandle {
    /// 创建新的句柄
    pub fn new(tx: mpsc::Sender<CommitRequest>, commit_timeout_ms: u64) -> Self {
        CommitServiceHandle {
            tx,
            commit_timeout_ms,
        }
    }

    /// 提交请求（不等待结果）
    pub async fn submit(&self, request: CommitRequest) -> Result<(), String> {
        self.tx.send(request)
            .await
            .map_err(|e| format!("Failed to send request: {}", e))
    }

    /// 提交请求并等待结果
    pub async fn submit_and_wait(&self, request: CommitRequest) -> Result<CommitResult, String> {
        let (tx, rx) = oneshot::channel();

        let mut request = request;
        request.response_tx = Some(tx);

        self.tx.send(request)
            .await
            .map_err(|e| format!("Failed to send request: {}", e))?;

        match timeout(
            Duration::from_millis(self.commit_timeout_ms),
            rx
        ).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(e)) => Err(format!("Channel error: {}", e)),
            Err(_) => Err(format!(
                "Commit timeout after {}ms",
                self.commit_timeout_ms
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_async_commit_service_creation() {
        let config = AsyncCommitConfig::default();
        let service = AsyncCommitService::new(config);

        assert!(!service.is_running());
        assert_eq!(service.stats().batch_size, 10);
    }

    #[tokio::test]
    async fn test_batch_by_size() {
        let config = AsyncCommitConfig {
            batch_size: 3,
            batch_timeout_ms: 10000, // 很长的超时，确保按大小触发
            channel_buffer_size: 10,
            commit_timeout_ms: 5000,
        };

        let mut service = AsyncCommitService::new(config);
        let sender = service.sender();

        // 启动服务
        let handle = tokio::spawn(async move {
            service.run().await
        });

        // 发送 3 个请求（达到批处理大小）
        for i in 0..3 {
            let request = CommitRequest {
                request_id: format!("req_{}", i),
                node_id: "node_1".to_string(),
                output: format!("output_{}", i),
                kv_proof: KvCacheProof::new(
                    format!("kv_{}", i),
                    format!("hash_{}", i),
                    "node_1".to_string(),
                    100,
                ),
                metadata: BlockMetadata::default(),
                expected_tokens: None,
                response_tx: None,
            };

            sender.send(request).await.unwrap();
        }

        // 等待处理
        tokio::time::sleep(Duration::from_millis(100)).await;

        // 停止服务
        drop(sender);
        let _ = handle.await;
    }

    #[tokio::test]
    async fn test_batch_by_timeout() {
        let config = AsyncCommitConfig {
            batch_size: 10, // 很大的大小，确保按超时触发
            batch_timeout_ms: 100,
            channel_buffer_size: 10,
            commit_timeout_ms: 5000,
        };

        let mut service = AsyncCommitService::new(config);
        let sender = service.sender();

        // 启动服务
        let handle = tokio::spawn(async move {
            service.run().await
        });

        // 发送 1 个请求（不足批处理大小，等待超时）
        let request = CommitRequest {
            request_id: "req_1".to_string(),
            node_id: "node_1".to_string(),
            output: "output_1".to_string(),
            kv_proof: KvCacheProof::new(
                "kv_1".to_string(),
                "hash_1".to_string(),
                "node_1".to_string(),
                100,
            ),
            metadata: BlockMetadata::default(),
            expected_tokens: None,
            response_tx: None,
        };

        sender.send(request).await.unwrap();

        // 等待超时触发
        tokio::time::sleep(Duration::from_millis(200)).await;

        // 停止服务
        drop(sender);
        let _ = handle.await;
    }

    #[tokio::test]
    async fn test_submit_with_timeout() {
        let config = AsyncCommitConfig::default();
        let service = AsyncCommitService::new(config);

        let request = CommitRequest {
            request_id: "req_1".to_string(),
            node_id: "node_1".to_string(),
            output: "output_1".to_string(),
            kv_proof: KvCacheProof::new(
                "kv_1".to_string(),
                "hash_1".to_string(),
                "node_1".to_string(),
                100,
            ),
            metadata: BlockMetadata::default(),
            expected_tokens: None,
            response_tx: None,
        };

        // 服务未启动，应该失败
        let result = service.submit_with_timeout(request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_commit_result_creation() {
        let success_result = CommitResult::success(100, "abc123".to_string(), 50);
        assert!(success_result.success);
        assert_eq!(success_result.block_height, Some(100));
        assert_eq!(success_result.block_hash, Some("abc123".to_string()));
        assert!(success_result.error.is_none());

        let failure_result = CommitResult::failure("Test error".to_string());
        assert!(!failure_result.success);
        assert!(failure_result.block_height.is_none());
        assert_eq!(failure_result.error, Some("Test error".to_string()));
    }
}
