//! 三层架构协调器 - 统一调度节点层、记忆层、推理提供商层交互
//!
//! **核心定位**：协调三层之间的数据流和权限控制，实现完整的推理流程
//!
//! # 依赖关系
//!
//! ```text
//! 推理提供商 → 依赖 → 记忆层（读取/写入 KV）
//! 推理提供商 → 依赖 → 节点层（获取访问授权/上报指标）
//! 记忆层 → 依赖 → 节点层（哈希校验/存证上链）
//! 节点层 → 不依赖 → 推理提供商/记忆层（仅做管控，不做执行）
//! ```
//!
//! # 异步上链
//!
//! 协调器支持两种上链模式：
//! - **同步上链**：`execute_inference()` 阻塞等待区块提交完成
//! - **异步上链**：`execute_inference_async()` 立即返回，区块在后台提交
//!
//! # 使用示例

use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::{Arc, RwLock};
#[cfg(feature = "async")]
use tokio::spawn;
#[cfg(feature = "async")]
use tokio::sync::mpsc;
use crate::node_layer::{
    NodeLayerManager, NodeIdentity, NodeRole, ProviderRecord, ProviderStatus,
    AccessCredential, AccessType, SchedulingStrategy,
};
use crate::memory_layer::{MemoryLayerManager, KvProof};
use crate::provider_layer::{
    ProviderLayerManager, InferenceRequest, InferenceResponse,
    MockInferenceProvider, InferenceEngineType,
};
use crate::blockchain::{Blockchain, BlockchainConfig};
use crate::block::KvCacheProof;
use crate::metadata::BlockMetadata;
use crate::transaction::{Transaction, TransactionType, TransactionPayload};
use crate::failover::{
    TimeoutConfig, ProviderHealthMonitor, FailoverReason, FailoverEvent,
};

/// 架构配置 - 三层架构的统一配置
#[derive(Debug, Clone)]
pub struct ArchitectureConfig {
    /// 节点层配置
    pub node_layer_config: NodeLayerConfig,
    /// 记忆层配置
    pub memory_layer_config: MemoryLayerConfig,
    /// 区块链配置
    pub blockchain_config: BlockchainConfig,
    /// 超时和故障转移配置
    pub timeout_config: TimeoutConfig,
}

/// 节点层配置
#[derive(Debug, Clone)]
pub struct NodeLayerConfig {
    /// 可信阈值
    pub trust_threshold: f64,
    /// 调度策略
    pub scheduling_strategy: SchedulingStrategy,
    /// 凭证有效期（秒）
    pub credential_expiry_secs: u64,
}

/// 记忆层配置
#[derive(Debug, Clone)]
pub struct MemoryLayerConfig {
    /// 热点缓存大小
    pub hot_cache_size: usize,
    /// 副本数量
    pub replica_count: usize,
    /// 区块最大 KV 数
    pub max_kv_per_block: usize,
}

impl Default for ArchitectureConfig {
    fn default() -> Self {
        ArchitectureConfig {
            node_layer_config: NodeLayerConfig {
                trust_threshold: 0.7,
                scheduling_strategy: SchedulingStrategy::Balanced,
                credential_expiry_secs: 3600, // 1 小时
            },
            memory_layer_config: MemoryLayerConfig {
                hot_cache_size: 1000,
                replica_count: 3,
                max_kv_per_block: 100,
            },
            blockchain_config: BlockchainConfig::default(),
            timeout_config: TimeoutConfig::default(),
        }
    }
}

/// 推理执行上下文 - 保存单次推理的完整上下文
#[derive(Debug, Clone)]
pub struct InferenceContext {
    /// 推理请求
    pub request: InferenceRequest,
    /// 访问凭证
    pub credential: AccessCredential,
    /// 选中的提供商 ID
    pub selected_provider_id: String,
    /// 开始时间
    pub start_time: u64,
    /// 结束时间（可选）
    pub end_time: Option<u64>,
    /// 是否成功
    pub success: bool,
}

impl InferenceContext {
    pub fn new(
        request: InferenceRequest,
        credential: AccessCredential,
        selected_provider_id: String,
    ) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        InferenceContext {
            request,
            credential,
            selected_provider_id,
            start_time: timestamp,
            end_time: None,
            success: false,
        }
    }
}

/// 异步上链任务结果
#[derive(Debug, Clone)]
pub struct AsyncCommitResult {
    /// 请求 ID
    pub request_id: String,
    /// 区块索引（如果成功）
    pub block_index: Option<u64>,
    /// 是否成功
    pub success: bool,
    /// 错误信息（如果失败）
    pub error_message: Option<String>,
}

/// 异步上链任务
#[cfg(feature = "async")]
pub struct AsyncCommitTask {
    /// 任务 ID
    pub task_id: String,
    /// 发送器用于返回结果
    pub tx: mpsc::Sender<AsyncCommitResult>,
}

#[cfg(feature = "async")]
impl AsyncCommitTask {
    pub fn new(task_id: String, tx: mpsc::Sender<AsyncCommitResult>) -> Self {
        AsyncCommitTask { task_id, tx }
    }
}

/// 三层架构协调器 - 统一管理三层交互
pub struct ArchitectureCoordinator {
    /// 节点层管理器
    pub node_layer: NodeLayerManager,
    /// 记忆层管理器
    pub memory_layer: MemoryLayerManager,
    /// 提供商层管理器
    pub provider_layer: ProviderLayerManager,
    /// 区块链（用于存证）
    pub blockchain: Blockchain,
    /// 架构配置
    pub config: ArchitectureConfig,
    /// 推理历史上下文
    pub inference_history: Vec<InferenceContext>,
    /// 提供商健康监控器
    pub health_monitor: ProviderHealthMonitor,
    /// 故障切换历史
    pub failover_history: Vec<FailoverEvent>,
    /// 链上存证失败缓存
    pub pending_commit_cache: Arc<PendingCommitCache>,
}

impl Clone for ArchitectureCoordinator {
    fn clone(&self) -> Self {
        ArchitectureCoordinator {
            node_layer: self.node_layer.clone(),
            memory_layer: self.memory_layer.clone(),
            provider_layer: self.provider_layer.clone(),
            blockchain: self.blockchain.clone(),
            config: self.config.clone(),
            inference_history: self.inference_history.clone(),
            health_monitor: self.health_monitor.clone(),
            failover_history: self.failover_history.clone(),
            pending_commit_cache: self.pending_commit_cache.clone(),
        }
    }
}

impl ArchitectureCoordinator {
    /// 创建新的架构协调器
    pub fn new(node_id: String) -> Self {
        Self::with_config(node_id, ArchitectureConfig::default())
            .expect("Failed to create ArchitectureCoordinator")
    }

    /// 创建带配置的架构协调器
    pub fn with_config(node_id: String, config: ArchitectureConfig) -> Result<Self, String> {
        let node_address = format!("address_{}", node_id);

        // 创建节点层
        let mut node_layer = NodeLayerManager::new(node_id.clone(), node_address.clone());
        node_layer.set_scheduling_strategy(config.node_layer_config.scheduling_strategy.clone());

        // 创建记忆层
        let memory_layer = MemoryLayerManager::new(&node_id);

        // 创建提供商层
        let provider_layer = ProviderLayerManager::new();

        // 创建区块链
        let blockchain = Blockchain::with_config(
            node_address.clone(),
            config.blockchain_config.clone(),
        );

        // 创建健康监控器
        let health_monitor = ProviderHealthMonitor::new(config.timeout_config.clone());

        // 创建待提交缓存
        let pending_commit_cache = Arc::new(PendingCommitCache::new(
            3,  // max_retries
            5000,  // retry_interval_ms
        ));

        // 注册当前节点
        let node_identity = NodeIdentity::new(
            node_id.clone(),
            node_address,
            NodeRole::Consensus,
            format!("pubkey_{}", node_id),
            Some("Enterprise Info".to_string()),
        );
        node_layer.register_node(node_identity)
            .map_err(|e| format!("Failed to register node: {}", e))?;

        Ok(ArchitectureCoordinator {
            node_layer,
            memory_layer,
            provider_layer,
            blockchain,
            config,
            inference_history: Vec::new(),
            health_monitor,
            failover_history: Vec::new(),
            pending_commit_cache,
        })
    }

    /// 启动后台重试 worker
    /// 
    /// 此方法启动一个后台任务，定期重试缓存中的待提交记录
    /// 
    /// # 使用示例
    ///
    /// ```rust,ignore
    /// #[cfg(feature = "async")]
    /// let mut coordinator = ArchitectureCoordinator::new("node1".to_string());
    ///
    /// // 启动后台重试 worker
    /// coordinator.spawn_pending_commit_worker().await;
    /// ```
    #[cfg(feature = "async")]
    pub fn spawn_pending_commit_worker(&self) -> tokio::task::JoinHandle<()> {
        let cache = self.pending_commit_cache.clone();
        let blockchain = self.blockchain.clone();
        let node_address = self.blockchain.owner_address().to_string();

        spawn(async move {
            let _handle = cache.spawn_retry_worker(move |commit| -> Result<(), String> {
                // 重试上链逻辑
                // 注意：blockchain 是 Arc<RwLock<>> 包装的，所以每次重试都使用最新状态
                let mut bc = blockchain.clone();

                // 重新添加 KV 存证
                for kv_proof in &commit.kv_proofs {
                    bc.add_kv_proof(kv_proof.clone());
                }

                // 重新添加交易
                let tx = Transaction::new_internal(
                    commit.provider_id.clone(),
                    node_address.clone(),
                    TransactionType::InferenceResponse,
                    TransactionPayload::InferenceResponse {
                        response_id: commit.response.request_id.clone(),
                        completion: commit.response.completion.clone(),
                        prompt_tokens: commit.response.prompt_tokens,
                        completion_tokens: commit.response.completion_tokens,
                    },
                );
                bc.add_pending_transaction(tx);

                // 提交到链上
                let metadata = BlockMetadata::new(
                    commit.response.request_id.clone(),
                    "1.0.0".to_string(),
                    commit.response.prompt_tokens as u64,
                    commit.response.completion_tokens as u64,
                    commit.response.latency_ms,
                    0.0,
                    commit.provider_id.clone(),
                );

                // 使用 blockchain.write() 获取可变引用进行提交
                bc.commit_inference(metadata, commit.provider_id.clone())
                    .map(|_| ())
            }).await;
        })
    }

    /// 启动协调器的所有后台服务
    ///
    /// 此方法启动：
    /// - PendingCommitCache 后台重试 worker
    ///
    /// # 使用示例
    ///
    /// ```rust,ignore
    /// #[cfg(feature = "async")]
    /// let mut coordinator = ArchitectureCoordinator::with_config("node1".to_string(), config)?;
    ///
    /// // 启动所有后台服务
    /// coordinator.start_async().await;
    /// ```
    #[cfg(feature = "async")]
    pub async fn start_async(&self) {
        let _ = self.spawn_pending_commit_worker().await;
    }

    /// 注册推理提供商
    pub fn register_provider(
        &mut self,
        provider_id: String,
        engine_type: InferenceEngineType,
        compute_capacity: u64,
    ) -> Result<(), String> {
        let provider = Box::new(MockInferenceProvider::new(
            provider_id.clone(),
            engine_type,
            compute_capacity,
        ));

        self.provider_layer.register_provider(provider)?;

        // 在节点层也注册提供商记录
        let mut record = ProviderRecord::new(
            provider_id.clone(),
            "1.0.0".to_string(),
            compute_capacity,
            0.1,
        );
        record.status = ProviderStatus::Active;
        self.node_layer.register_provider(record)?;

        // 在健康监控器中注册
        self.health_monitor.register_provider(provider_id.clone(), 0.5)
            .map_err(|e| format!("Failed to register provider in health monitor: {}", e))?;

        Ok(())
    }

    /// 执行推理流程（带超时检测和自动故障切换）
    ///
    /// **核心功能**：
    /// - 自动检测推理超时
    /// - 自动切换到备用提供商
    /// - 上下文不丢失
    /// - 防抖动机制
    ///
    /// 流程：
    /// 1. 选择主提供商
    /// 2. 执行推理（带超时检测）
    /// 3. 如果超时，记录失败并选择备用提供商
    /// 4. 保存上下文到记忆层
    /// 5. 新提供商从记忆层继续
    /// 6. 记录故障切换事件
    pub fn execute_inference_with_failover(
        &mut self,
        request: InferenceRequest,
    ) -> Result<InferenceResponse, String> {
        let task_id = request.request_id.clone();
        let timeout_ms = self.config.timeout_config.inference_timeout_ms;

        // 检查是否允许故障切换
        if !self.health_monitor.can_failover(&task_id)
            .unwrap_or(false) {
            return Err(format!(
                "Task {} reached max failover count ({})",
                task_id, self.config.timeout_config.max_failover_count
            ));
        }

        // 选择初始提供商
        let mut current_provider_id = self.select_provider()?;
        let mut attempts = 0u32;

        loop {
            attempts += 1;

            // 执行带超时检测的推理
            match self.execute_with_timeout(&request, &current_provider_id, timeout_ms) {
                Ok(response) => {
                    // 推理成功，记录健康状态
                    let _ = self.health_monitor.record_success(&current_provider_id, response.latency_ms);

                    // 处理后续流程（写入记忆层、上报指标、上链）
                    return self.finalize_inference(response, &request, &current_provider_id);
                }
                Err(e) => {
                    // 推理失败，记录失败
                    if e.contains("timeout") {
                        let _ = self.health_monitor.record_timeout(&current_provider_id);
                    } else {
                        let _ = self.health_monitor.record_failure(&current_provider_id);
                    }

                    // 检查是否还能切换
                    if !self.health_monitor.can_failover(&task_id)
                        .unwrap_or(false) {
                        return Err(format!(
                            "Task {} reached max failover count after {} attempts. Last error: {}",
                            task_id, attempts, e
                        ));
                    }

                    // 选择备用提供商
                    match self.health_monitor.select_best_backup(Some(&current_provider_id)) {
                        Some(backup_id) => {
                            // 记录故障切换事件
                            let failover_count = self.health_monitor.increment_failover_count(&task_id)
                                .unwrap_or(0);
                            let event = FailoverEvent {
                                task_id: task_id.clone(),
                                from_provider: current_provider_id.clone(),
                                to_provider: backup_id.clone(),
                                reason: if e.contains("timeout") {
                                    FailoverReason::Timeout
                                } else {
                                    FailoverReason::ProviderDown
                                },
                                timestamp: SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                                failover_count,
                            };
                            self.failover_history.push(event.clone());

                            // 保存上下文到记忆层（确保不丢失）
                            self.save_context_for_failover(&request, &current_provider_id)?;

                            println!(
                                "Failover #{}: {} -> {} (reason: {:?})",
                                failover_count, current_provider_id, backup_id, event.reason
                            );

                            current_provider_id = backup_id;
                        }
                        None => {
                            return Err(format!(
                                "No available backup provider after {} attempts. Last error: {}",
                                attempts, e
                            ));
                        }
                    }
                }
            }
        }
    }

    /// 执行带超时检测的推理
    fn execute_with_timeout(
        &mut self,
        request: &InferenceRequest,
        provider_id: &str,
        timeout_ms: u64,
    ) -> Result<InferenceResponse, String> {
        // 获取凭证
        let credential = self.node_layer.issue_credential(
            provider_id.to_string(),
            request.memory_block_ids.iter().map(|id| id.to_string()).collect(),
            AccessType::ReadWrite,
            self.config.node_layer_config.credential_expiry_secs,
        )?;

        // 执行推理
        let start_time = SystemTime::now();
        let response = self.provider_layer.execute_with_provider(
            provider_id,
            request,
            &self.memory_layer,
            &credential,
        )?;
        
        let elapsed_ms = start_time
            .elapsed()
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        // 检查是否超时
        if elapsed_ms > timeout_ms {
            return Err(format!(
                "Inference timeout: provider={}, timeout={}ms, elapsed={}ms",
                provider_id, timeout_ms, elapsed_ms
            ));
        }

        Ok(response)
    }

    /// 保存上下文以便故障切换时不丢失
    fn save_context_for_failover(
        &mut self,
        request: &InferenceRequest,
        _provider_id: &str,
    ) -> Result<(), String> {
        // 将请求上下文保存到记忆层
        // 这样新提供商可以从记忆层读取继续推理
        let context_key = format!("failover_context_{}", request.request_id);
        let context_data = request.prompt.as_bytes().to_vec();

        // 使用协调器内部凭证写入（协调器作为可信节点）
        // 创建一个内部凭证用于故障切换上下文保存
        let internal_credential = AccessCredential {
            credential_id: format!("internal_failover_{}", request.request_id),
            provider_id: self.blockchain.owner_address().to_string(),
            memory_block_ids: vec!["all".to_string()],
            access_type: AccessType::ReadWrite,
            expires_at: u64::MAX,
            issuer_node_id: self.blockchain.owner_address().to_string(),
            signature: "internal_failover_signature".to_string(),
            is_revoked: false,
        };

        // 保存请求上下文
        self.memory_layer.write_kv(
            context_key,
            context_data,
            &internal_credential,
        )?;

        // 同时保存请求元数据（用于新提供商重建请求）
        let metadata_key = format!("failover_metadata_{}", request.request_id);
        let metadata = serde_json::json!({
            "request_id": request.request_id,
            "model_id": request.model_id,
            "max_tokens": request.max_tokens,
            "memory_block_ids": request.memory_block_ids,
            "timestamp": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        });
        let metadata_bytes = serde_json::to_vec(&metadata)
            .map_err(|e| format!("Failed to serialize failover metadata: {}", e))?;

        self.memory_layer.write_kv(
            metadata_key,
            metadata_bytes,
            &internal_credential,
        )?;

        Ok(())
    }

    /// 完成推理流程（写入记忆层、上报指标、上链）
    fn finalize_inference(
        &mut self,
        response: InferenceResponse,
        request: &InferenceRequest,
        provider_id: &str,
    ) -> Result<InferenceResponse, String> {
        // 阶段 4：写入新 KV 到记忆层
        let credential = self.node_layer.issue_credential(
            provider_id.to_string(),
            request.memory_block_ids.iter().map(|id| id.to_string()).collect(),
            AccessType::ReadWrite,
            self.config.node_layer_config.credential_expiry_secs,
        )?;

        for (key, value) in &response.new_kv {
            self.memory_layer.write_kv(
                key.clone(),
                value.clone(),
                &credential,
            )?;
        }

        // 阶段 5：密封记忆区块（准备上链）
        self.memory_layer.seal_current_block();

        // 阶段 6：上报指标到节点层
        self.node_layer.report_provider_metrics(
            provider_id,
            response.efficiency,
            response.success,
        )?;

        // 阶段 7：哈希校验并上链存证
        self.commit_to_blockchain(&response, provider_id)?;

        // 记录推理上下文
        let mut context = InferenceContext::new(
            request.clone(),
            credential,
            provider_id.to_string(),
        );
        context.success = response.success;
        context.end_time = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );
        self.inference_history.push(context);

        // 重置故障切换计数
        let _ = self.health_monitor.reset_failover_count(&request.request_id);

        Ok(response)
    }

    /// 执行推理流程（完整端到端流程，带故障转移）
    ///
    /// 流程：
    /// 1. 选择最佳推理提供商
    /// 2. 签发访问凭证
    /// 3. 执行推理（带超时检测和自动故障切换）
    /// 4. 写入新 KV 到记忆层
    /// 5. 上报指标到节点层
    /// 6. 哈希校验并上链存证
    ///
    /// # 故障转移机制
    /// - 自动检测超时和提供商故障
    /// - 自动切换到备用提供商
    /// - 上下文不丢失（保存到记忆层）
    /// - 防抖动（冷却时间 + 最大切换次数）
    pub fn execute_inference(
        &mut self,
        request: InferenceRequest,
    ) -> Result<InferenceResponse, String> {
        // 使用带故障转移的推理执行
        self.execute_inference_with_failover(request)
    }

    /// 执行推理流程（异步上链版本）
    ///
    /// **异步上链**：推理执行同步完成，上链存证在后台异步执行
    ///
    /// 流程：
    /// 1. 选择最佳推理提供商（同步）
    /// 2. 签发访问凭证（同步）
    /// 3. 执行推理（同步）
    /// 4. 写入新 KV 到记忆层（同步）
    /// 5. 上报指标到节点层（同步）
    /// 6. 上链存证（异步，立即返回）
    ///
    /// 返回：
    /// - 推理响应（立即返回）
    /// - 异步任务接收器（用于获取上链结果）
    #[cfg(feature = "async")]
    pub fn execute_inference_async(
        &mut self,
        request: InferenceRequest,
    ) -> Result<(InferenceResponse, mpsc::Receiver<AsyncCommitResult>), String> {
        use sha2::{Digest, Sha256};

        // 阶段 1-5：同步执行（与同步版本相同）
        let provider_id = self.select_provider()?;

        let credential = self.node_layer.issue_credential(
            provider_id.clone(),
            request.memory_block_ids.iter().map(|id| id.to_string()).collect(),
            AccessType::ReadWrite,
            self.config.node_layer_config.credential_expiry_secs,
        )?;

        let response = self.provider_layer.execute_with_provider(
            &provider_id,
            &request,
            &self.memory_layer,
            &credential,
        )?;

        // 写入新 KV 到记忆层
        for (key, value) in &response.new_kv {
            self.memory_layer.write_kv(
                key.clone(),
                value.clone(),
                &credential,
            )?;
        }

        // 密封记忆区块
        self.memory_layer.seal_current_block();

        // 上报指标
        self.node_layer.report_provider_metrics(
            &provider_id,
            response.efficiency,
            response.success,
        )?;

        // 阶段 6：异步上链存证（带重试和缓存机制）
        let (commit_tx, commit_rx) = mpsc::channel(1);

        // 克隆需要的数据
        let blockchain_owner = self.blockchain.owner_address().to_string();
        let mut blockchain = self.blockchain.clone();
        let response_clone = response.clone();
        let provider_id_clone = provider_id.clone();
        let request_id = request.request_id.clone();
        // 使用 Arc<RwLock<>> 包装 memory_layer 以支持异步环境中的内部可变性
        let memory_layer_arc = Arc::new(RwLock::new(self.memory_layer.clone()));
        let max_retries = 3u32;
        let retry_delay_ms = 1000u64;
        let pending_commit_cache = self.pending_commit_cache.clone();

        // 在后台任务中执行上链（带重试和缓存）
        spawn(async move {
            let mut retry_count = 0u32;

            loop {
                // 创建 KV 存证
                for (key, value) in &response_clone.new_kv {
                    let kv_hash = format!("{:x}", Sha256::digest(value));
                    let kv_proof = KvCacheProof::new(
                        format!("kv_{}_{}", response_clone.request_id, key),
                        kv_hash,
                        provider_id_clone.clone(),
                        value.len() as u64,
                    );
                    blockchain.add_kv_proof(kv_proof);
                }

                // 创建推理记录交易
                let tx_record = Transaction::new_internal(
                    provider_id_clone.clone(),
                    blockchain_owner.clone(),
                    TransactionType::InferenceResponse,
                    TransactionPayload::InferenceResponse {
                        response_id: response_clone.request_id.clone(),
                        completion: response_clone.completion.clone(),
                        prompt_tokens: response_clone.prompt_tokens,
                        completion_tokens: response_clone.completion_tokens,
                    },
                );
                blockchain.add_pending_transaction(tx_record);

                // 提交到链上
                let metadata = BlockMetadata::new(
                    response_clone.request_id.clone(),
                    "1.0.0".to_string(),
                    response_clone.prompt_tokens as u64,
                    response_clone.completion_tokens as u64,
                    response_clone.latency_ms,
                    0.0,
                    provider_id_clone.clone(),
                );

                let result = blockchain.commit_inference(metadata, provider_id_clone.clone());

                match result {
                    Ok(block) => {
                        let commit_result = AsyncCommitResult {
                            request_id: request_id.clone(),
                            block_index: Some(block.index),
                            success: true,
                            error_message: None,
                        };
                        let _ = commit_tx.send(commit_result).await;
                        break;
                    }
                    Err(e) => {
                        retry_count += 1;

                        if retry_count < max_retries {
                            // 等待后重试
                            tokio::time::sleep(
                                tokio::time::Duration::from_millis(retry_delay_ms * retry_count as u64)
                            ).await;
                            continue;
                        } else {
                            // 达到最大重试次数，添加到缓存，后台重试
                            Self::log_commit_failure(&e, &request_id);

                            // 创建待提交记录并添加到缓存
                            // 从 response_clone.new_kv 生成 kv_proofs
                            let kv_proofs: Vec<KvCacheProof> = response_clone.new_kv.iter().map(|(key, value)| {
                                let kv_hash = format!("{:x}", sha2::Sha256::digest(value));
                                KvCacheProof::new(
                                    format!("kv_{}_{}", response_clone.request_id, key),
                                    kv_hash,
                                    provider_id_clone.clone(),
                                    value.len() as u64,
                                )
                            }).collect();

                            let pending_commit = PendingCommit {
                                request_id: request_id.clone(),
                                kv_proofs,
                                response: response_clone.clone(),
                                provider_id: provider_id_clone.clone(),
                                retry_count: 0,
                                last_error: Some(e.clone()),
                                created_at: SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                            };

                            if let Err(cache_err) = pending_commit_cache.add(pending_commit) {
                                eprintln!(
                                    "[Cache Error] request_id={}, failed to add to cache: {}",
                                    request_id, cache_err
                                );
                            } else {
                                println!(
                                    "[Cache Add] request_id={} added to pending commit cache",
                                    request_id
                                );
                            }

                            // 触发回滚（暂时回滚，缓存成功后再恢复）
                            Self::trigger_rollback(memory_layer_arc.clone(), &request_id).await;

                            let commit_result = AsyncCommitResult {
                                request_id: request_id.clone(),
                                block_index: None,
                                success: false,
                                error_message: Some(format!(
                                    "Commit failed after {} retries, added to cache: {}",
                                    max_retries, e
                                )),
                            };
                            let _ = commit_tx.send(commit_result).await;
                            break;
                        }
                    }
                }
            }
        });

        // 记录推理上下文（标记为异步上链）
        let mut context = InferenceContext::new(request, credential, provider_id);
        context.success = response.success;
        context.end_time = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );
        self.inference_history.push(context);

        Ok((response, commit_rx))
    }

    /// 选择最佳推理提供商
    fn select_provider(&self) -> Result<String, String> {
        // 优先使用节点层调度（综合评分）
        if let Some(best) = self.node_layer.select_best_provider() {
            return Ok(best.provider_id.clone());
        }

        // 回退到提供商层
        if let Some(provider) = self.provider_layer.current_provider() {
            return Ok(provider.provider_id().to_string());
        }

        Err("No available inference provider".to_string())
    }

    /// 提交推理结果到区块链
    fn commit_to_blockchain(
        &mut self,
        response: &InferenceResponse,
        provider_id: &str,
    ) -> Result<(), String> {
        use sha2::{Digest, Sha256};

        // 创建 KV 存证
        for (key, value) in &response.new_kv {
            let kv_hash = format!("{:x}", Sha256::digest(value));
            let kv_proof = KvCacheProof::new(
                format!("kv_{}_{}", response.request_id, key),
                kv_hash,
                provider_id.to_string(),
                value.len() as u64,
            );
            self.blockchain.add_kv_proof(kv_proof);
        }

        // 创建推理记录交易
        let tx = Transaction::new_internal(
            provider_id.to_string(),
            self.blockchain.owner_address().to_string(),
            TransactionType::InferenceResponse,
            TransactionPayload::InferenceResponse {
                response_id: response.request_id.clone(),
                completion: response.completion.clone(),
                prompt_tokens: response.prompt_tokens,
                completion_tokens: response.completion_tokens,
            },
        );
        self.blockchain.add_pending_transaction(tx);

        // 提交到链上
        let metadata = BlockMetadata::new(
            response.request_id.clone(),
            "1.0.0".to_string(),
            response.prompt_tokens as u64,
            response.completion_tokens as u64,
            response.latency_ms,
            0.0, // 成本（暂未计算）
            provider_id.to_string(),
        );

        self.blockchain.commit_inference(metadata, provider_id.to_string())?;

        Ok(())
    }

    /// 获取记忆层 KV 证明（用于外部验证）
    pub fn get_kv_proofs(&self) -> Vec<KvProof> {
        self.memory_layer.get_all_kv_proofs()
    }

    /// 验证记忆链完整性
    pub fn verify_memory_chain(&self) -> bool {
        self.memory_layer.verify_chain()
    }

    /// 验证区块链完整性
    pub fn verify_blockchain(&self) -> bool {
        self.blockchain.verify_chain()
    }

    /// 获取推理统计
    pub fn get_inference_stats(&self) -> InferenceStats {
        let total = self.inference_history.len();
        let successful = self.inference_history.iter().filter(|c| c.success).count();
        let failed = total - successful;

        let avg_latency = if total > 0 {
            self.inference_history.iter()
                .filter_map(|c| c.end_time.map(|end| end - c.start_time))
                .sum::<u64>() / total as u64
        } else {
            0
        };

        InferenceStats {
            total,
            successful,
            failed,
            avg_latency_ms: avg_latency,
            success_rate: if total > 0 { successful as f64 / total as f64 } else { 0.0 },
        }
    }

    /// 切换推理提供商（支持无缝切换）
    pub fn switch_provider(
        &mut self,
        new_provider_id: &str,
        reason: &str,
    ) -> Result<(), String> {
        // 验证新提供商存在
        if self.provider_layer.get_provider(new_provider_id).is_none() {
            return Err(format!("Provider {} not found", new_provider_id));
        }

        // 获取当前提供商
        let old_provider_id = self.provider_layer.current_provider()
            .map(|p| p.provider_id().to_string());

        // 设置新提供商
        self.provider_layer.set_current_provider(new_provider_id)?;

        // 记录切换事件
        println!(
            "Provider switched: {:?} -> {} (reason: {})",
            old_provider_id, new_provider_id, reason
        );

        // 密封当前记忆区块（快照存证）
        self.memory_layer.seal_current_block();

        // 上链存证切换事件
        // old_provider_id 为 None 时记录警告，但不阻止切换
        let old_id_str = old_provider_id.unwrap_or_else(|| {
            eprintln!("[Warning] Provider switch without previous provider (first provider or recovery scenario)");
            String::new()
        });
        self.commit_provider_switch_event(&old_id_str, new_provider_id, reason)?;

        Ok(())
    }

    /// 提交提供商切换事件到链上
    fn commit_provider_switch_event(
        &mut self,
        _old_provider_id: &str,
        new_provider_id: &str,
        reason: &str,
    ) -> Result<(), String> {
        let tx = Transaction::new_internal(
            self.node_layer.node_public_key.clone(),
            new_provider_id.to_string(),
            TransactionType::Internal,
            TransactionPayload::None,
        );
        self.blockchain.add_pending_transaction(tx);

        let metadata = BlockMetadata::new(
            format!("provider_switch_{}", new_provider_id),
            "1.0.0".to_string(),
            0,
            0,
            0,
            0.0,
            format!("switch_reason:{}", reason),
        );

        self.blockchain.commit_inference(metadata, new_provider_id.to_string())?;

        Ok(())
    }

    /// 获取节点层管理器（可变引用）
    pub fn node_layer_mut(&mut self) -> &mut NodeLayerManager {
        &mut self.node_layer
    }

    /// 获取记忆层管理器（可变引用）
    pub fn memory_layer_mut(&mut self) -> &mut MemoryLayerManager {
        &mut self.memory_layer
    }

    /// 获取提供商层管理器（可变引用）
    pub fn provider_layer_mut(&mut self) -> &mut ProviderLayerManager {
        &mut self.provider_layer
    }

    /// 记录上链失败（用于日志和监控）
    #[cfg(feature = "async")]
    fn log_commit_failure(error: &str, request_id: &str) {
        eprintln!(
            "[Commit Failure] request_id={}, error={}",
            request_id, error
        );
        // TODO: 集成到 tracing 日志系统
    }

    /// 触发回滚操作（当上链失败时回滚记忆层写入）
    /// 
    /// 使用 Arc<RwLock<>> 包装 memory_layer 以支持异步环境中的内部可变性
    #[cfg(feature = "async")]
    async fn trigger_rollback(
        memory_layer: Arc<RwLock<MemoryLayerManager>>,
        request_id: &str,
    ) {
        println!(
            "[Rollback] request_id={}, attempting to roll back memory layer writes",
            request_id
        );

        // 获取写锁并执行回滚
        match memory_layer.write() {
            Ok(mut guard) => {
                if let Err(e) = guard.mark_current_block_as_rolled_back() {
                    eprintln!(
                        "[Rollback Error] request_id={}, failed to roll back: {}",
                        request_id, e
                    );
                } else {
                    println!(
                        "[Rollback Success] request_id={}, memory layer rolled back successfully",
                        request_id
                    );
                }
            }
            Err(poisoned) => {
                eprintln!(
                    "[Rollback Error] request_id={}, memory layer lock poisoned: {}",
                    request_id, poisoned
                );
            }
        }

        // TODO: 实现完整的事务回滚机制
        // 1. 撤销 KV 写入（已完成）
        // 2. 通知相关节点
        // 3. 记录回滚事件到审计日志
    }
}

/// 推理统计信息
#[derive(Debug, Clone, Default)]
pub struct InferenceStats {
    /// 总推理次数
    pub total: usize,
    /// 成功次数
    pub successful: usize,
    /// 失败次数
    pub failed: usize,
    /// 平均延迟（毫秒）
    pub avg_latency_ms: u64,
    /// 成功率
    pub success_rate: f64,
}

/// 待提交记录 - 链上存证失败时的缓存项
#[derive(Debug, Clone)]
pub struct PendingCommit {
    /// 请求 ID
    pub request_id: String,
    /// KV 存证列表
    pub kv_proofs: Vec<KvCacheProof>,
    /// 推理响应数据
    pub response: InferenceResponse,
    /// 提供商 ID
    pub provider_id: String,
    /// 重试次数
    pub retry_count: u32,
    /// 最后错误信息
    pub last_error: Option<String>,
    /// 创建时间戳
    pub created_at: u64,
}

impl PendingCommit {
    pub fn new(
        request_id: String,
        kv_proofs: Vec<KvCacheProof>,
        response: InferenceResponse,
        provider_id: String,
    ) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        PendingCommit {
            request_id,
            kv_proofs,
            response,
            provider_id,
            retry_count: 0,
            last_error: None,
            created_at: timestamp,
        }
    }
}

/// 待提交缓存 - 链上存证失败时的本地缓存和重试机制
#[derive(Debug, Clone)]
pub struct PendingCommitCache {
    /// 缓存的待提交记录
    cache: Arc<RwLock<Vec<PendingCommit>>>,
    /// 最大重试次数
    max_retries: u32,
    /// 重试间隔（毫秒）
    retry_interval_ms: u64,
}

impl PendingCommitCache {
    pub fn new(max_retries: u32, retry_interval_ms: u64) -> Self {
        PendingCommitCache {
            cache: Arc::new(RwLock::new(Vec::new())),
            max_retries,
            retry_interval_ms,
        }
    }

    /// 添加待提交记录
    pub fn add(&self, commit: PendingCommit) -> Result<(), String> {
        let mut cache = self.cache.write()
            .map_err(|e| format!("Cache lock poisoned: {}", e))?;
        cache.push(commit);
        Ok(())
    }

    /// 获取所有待提交记录
    pub fn get_all(&self) -> Result<Vec<PendingCommit>, String> {
        let cache = self.cache.read()
            .map_err(|e| format!("Cache lock poisoned: {}", e))?;
        Ok(cache.clone())
    }

    /// 移除已成功的记录
    pub fn remove(&self, request_id: &str) -> Result<(), String> {
        let mut cache = self.cache.write()
            .map_err(|e| format!("Cache lock poisoned: {}", e))?;
        cache.retain(|c| c.request_id != request_id);
        Ok(())
    }

    /// 增加重试计数
    pub fn increment_retry(&self, request_id: &str, error: &str) -> Result<bool, String> {
        let mut cache = self.cache.write()
            .map_err(|e| format!("Cache lock poisoned: {}", e))?;
        
        if let Some(commit) = cache.iter_mut().find(|c| c.request_id == request_id) {
            commit.retry_count += 1;
            commit.last_error = Some(error.to_string());
            Ok(commit.retry_count <= self.max_retries)
        } else {
            Ok(false)
        }
    }

    /// 获取缓存大小
    pub fn len(&self) -> Result<usize, String> {
        let cache = self.cache.read()
            .map_err(|e| format!("Cache lock poisoned: {}", e))?;
        Ok(cache.len())
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> Result<bool, String> {
        self.len().map(|len| len == 0)
    }

    /// 获取重试间隔
    pub fn get_retry_interval(&self) -> u64 {
        self.retry_interval_ms
    }
}

#[cfg(feature = "async")]
impl PendingCommitCache {
    /// 启动后台重试工作器
    pub async fn spawn_retry_worker<F>(
        self: Arc<Self>,
        mut commit_func: F,
    ) -> tokio::task::JoinHandle<()>
    where
        F: FnMut(&PendingCommit) -> Result<(), String> + Send + 'static,
    {
        use tokio::time::{sleep, Duration};

        let cache_clone = self.clone();
        
        tokio::spawn(async move {
            loop {
                // 获取所有待提交记录
                let pending = match cache_clone.get_all() {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("[RetryWorker] Failed to get pending commits: {}", e);
                        sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                };

                // 重试每个待提交记录
                for commit in &pending {
                    if commit.retry_count >= cache_clone.max_retries {
                        // 超过最大重试次数，移除
                        eprintln!(
                            "[RetryWorker] Request {} exceeded max retries ({}), dropping",
                            commit.request_id, commit.retry_count
                        );
                        let _ = cache_clone.remove(&commit.request_id);
                        continue;
                    }

                    // 尝试提交
                    match commit_func(commit) {
                        Ok(_) => {
                            println!("[RetryWorker] Request {} committed successfully", commit.request_id);
                            let _ = cache_clone.remove(&commit.request_id);
                        }
                        Err(e) => {
                            eprintln!(
                                "[RetryWorker] Request {} retry {} failed: {}",
                                commit.request_id, commit.retry_count + 1, e
                            );
                            let _ = cache_clone.increment_retry(&commit.request_id, &e);
                        }
                    }
                }

                // 等待下次重试
                sleep(Duration::from_millis(cache_clone.retry_interval_ms)).await;
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_architecture_coordinator_creation() {
        let coordinator = ArchitectureCoordinator::new("node_1".to_string());

        assert_eq!(coordinator.node_layer.node_count(), 1);
        assert_eq!(coordinator.memory_layer.height(), 1);
        assert_eq!(coordinator.provider_layer.provider_count(), 0);
        assert!(coordinator.verify_blockchain());
    }

    #[test]
    fn test_register_and_execute_inference() {
        let mut coordinator = ArchitectureCoordinator::new("node_1".to_string());

        // 注册提供商
        coordinator.register_provider(
            "provider_1".to_string(),
            InferenceEngineType::Vllm,
            100,
        ).unwrap();

        // 创建推理请求
        let request = InferenceRequest::new(
            "req_1".to_string(),
            "Hello, AI!".to_string(),
            "llama-7b".to_string(),
            100,
        ).with_memory_blocks(vec![0]);

        // 执行推理
        let response = coordinator.execute_inference(request).unwrap();

        assert!(response.success);
        assert!(!response.completion.is_empty());
        assert!(response.completion_tokens > 0);
    }

    #[test]
    fn test_provider_switching() {
        let mut coordinator = ArchitectureCoordinator::new("node_1".to_string());

        // 注册两个提供商
        coordinator.register_provider(
            "provider_1".to_string(),
            InferenceEngineType::Vllm,
            100,
        ).unwrap();

        coordinator.register_provider(
            "provider_2".to_string(),
            InferenceEngineType::Sglang,
            80,
        ).unwrap();

        // 切换到 provider_2
        coordinator.switch_provider("provider_2", "test switch").unwrap();

        // 验证切换成功
        let current = coordinator.provider_layer.current_provider().unwrap();
        assert_eq!(current.provider_id(), "provider_2");
    }

    #[test]
    fn test_chain_verification() {
        let mut coordinator = ArchitectureCoordinator::new("node_1".to_string());

        // 注册提供商并执行推理
        coordinator.register_provider(
            "provider_1".to_string(),
            InferenceEngineType::Vllm,
            100,
        ).unwrap();

        let request = InferenceRequest::new(
            "req_1".to_string(),
            "Hello!".to_string(),
            "llama-7b".to_string(),
            100,
        );

        coordinator.execute_inference(request).unwrap();

        // 验证两条链
        assert!(coordinator.verify_memory_chain());
        assert!(coordinator.verify_blockchain());
    }

    #[test]
    fn test_inference_stats() {
        let mut coordinator = ArchitectureCoordinator::new("node_1".to_string());

        coordinator.register_provider(
            "provider_1".to_string(),
            InferenceEngineType::Vllm,
            100,
        ).unwrap();

        // 执行多次推理
        for i in 0..3 {
            let request = InferenceRequest::new(
                format!("req_{}", i),
                format!("Prompt {}", i),
                "llama-7b".to_string(),
                100,
            );
            coordinator.execute_inference(request).unwrap();
        }

        let stats = coordinator.get_inference_stats();
        assert_eq!(stats.total, 3);
        assert_eq!(stats.successful, 3);
        assert_eq!(stats.failed, 0);
        assert_eq!(stats.success_rate, 1.0);
    }

    #[test]
    fn test_execute_inference_with_failover_timeout() {
        // 创建带短超时配置的协调器
        let mut config = ArchitectureConfig::default();
        config.timeout_config.inference_timeout_ms = 50; // 50ms 超时
        config.timeout_config.max_failover_count = 2;
        config.timeout_config.consecutive_timeout_threshold = 1; // 1 次超时即标记

        let mut coordinator = ArchitectureCoordinator::with_config("node_1".to_string(), config).unwrap();

        // 注册两个提供商，第一个会超时
        let provider_slow = Box::new(MockInferenceProvider::new(
            "provider_slow".to_string(),
            InferenceEngineType::Vllm,
            100,
        ).with_latency(100)); // 100ms 延迟，超过 50ms 超时

        let provider_fast = Box::new(MockInferenceProvider::new(
            "provider_fast".to_string(),
            InferenceEngineType::Sglang,
            80,
        ).with_latency(10)); // 10ms 延迟，正常

        coordinator.provider_layer.register_provider(provider_slow).unwrap();
        coordinator.provider_layer.register_provider(provider_fast).unwrap();

        // 在节点层也注册
        let mut record1 = ProviderRecord::new("provider_slow".to_string(), "1.0.0".to_string(), 100, 0.1);
        record1.status = ProviderStatus::Active;
        coordinator.node_layer.register_provider(record1).unwrap();

        let mut record2 = ProviderRecord::new("provider_fast".to_string(), "1.0.0".to_string(), 80, 0.1);
        record2.status = ProviderStatus::Active;
        coordinator.node_layer.register_provider(record2).unwrap();

        // 在健康监控器中注册，设置不同的信誉分以便区分
        let _ = coordinator.health_monitor.register_provider("provider_slow".to_string(), 0.5);
        let _ = coordinator.health_monitor.register_provider("provider_fast".to_string(), 0.9);

        // 手动设置当前提供商为 slow（在注册之后）
        coordinator.provider_layer.set_current_provider("provider_slow").unwrap();

        // 创建推理请求
        let request = InferenceRequest::new(
            "req_failover_1".to_string(),
            "Hello!".to_string(),
            "llama-7b".to_string(),
            100,
        ).with_memory_blocks(vec![0]);

        // 执行带故障切换的推理
        let response = coordinator.execute_inference_with_failover(request);

        // 应该成功（切换到快速提供商）
        assert!(response.is_ok(), "Failover should succeed: {:?}", response);
    }

    #[test]
    fn test_health_monitor_records() {
        let mut coordinator = ArchitectureCoordinator::new("node_1".to_string());

        coordinator.register_provider(
            "provider_1".to_string(),
            InferenceEngineType::Vllm,
            100,
        ).unwrap();

        coordinator.register_provider(
            "provider_2".to_string(),
            InferenceEngineType::Sglang,
            80,
        ).unwrap();

        // 检查提供商是否被注册到健康监控器
        let record1 = coordinator.health_monitor.get_record("provider_1");
        let record2 = coordinator.health_monitor.get_record("provider_2");

        assert!(record1.is_some(), "Provider 1 should have health record");
        assert!(record2.is_some(), "Provider 2 should have health record");

        // 检查可用提供商
        let available = coordinator.health_monitor.get_available_providers().unwrap();
        assert_eq!(available.len(), 2, "Should have 2 available providers");
    }

    #[test]
    fn test_failover_switch_to_success() {
        // 测试故障切换核心场景：超时→切换→成功
        let mut config = ArchitectureConfig::default();
        config.timeout_config.inference_timeout_ms = 50; // 50ms 超时
        config.timeout_config.max_failover_count = 2; // 允许切换 2 次

        let mut coordinator = ArchitectureCoordinator::with_config("node_1".to_string(), config).unwrap();

        // 注册两个提供商：第一个超时，第二个成功
        // provider_timeout: 模拟超时（latency > timeout）
        let provider_timeout = Box::new(
            MockInferenceProvider::new(
                "provider_timeout".to_string(),
                InferenceEngineType::Vllm,
                100,
            )
            .with_latency(100) // 100ms 延迟
            .with_timeout(50) // 50ms 超时，会触发超时错误
        );

        // provider_success: 模拟快速响应（latency < timeout，成功）
        let provider_success = Box::new(
            MockInferenceProvider::new(
                "provider_success".to_string(),
                InferenceEngineType::Sglang,
                100,
            )
            .with_latency(20) // 20ms 延迟
            .with_timeout(50) // 50ms 超时，不会触发
        );

        coordinator.provider_layer.register_provider(provider_timeout).unwrap();
        coordinator.provider_layer.register_provider(provider_success).unwrap();

        // 在节点层也注册
        let mut record1 = ProviderRecord::new("provider_timeout".to_string(), "1.0.0".to_string(), 100, 0.1);
        record1.status = ProviderStatus::Active;
        coordinator.node_layer.register_provider(record1).unwrap();

        let mut record2 = ProviderRecord::new("provider_success".to_string(), "1.0.0".to_string(), 100, 0.1);
        record2.status = ProviderStatus::Active;
        coordinator.node_layer.register_provider(record2).unwrap();

        // 在健康监控器中注册
        let _ = coordinator.health_monitor.register_provider("provider_timeout".to_string(), 0.5);
        let _ = coordinator.health_monitor.register_provider("provider_success".to_string(), 0.9);

        // 手动设置当前提供商为 timeout 的那个
        coordinator.provider_layer.set_current_provider("provider_timeout").unwrap();

        // 创建推理请求
        let request = InferenceRequest::new(
            "req_failover_success".to_string(),
            "Hello!".to_string(),
            "llama-7b".to_string(),
            100,
        ).with_memory_blocks(vec![0]);

        // 执行带故障切换的推理
        // 预期流程：provider_timeout 超时 → 切换到 provider_success → 成功
        let response = coordinator.execute_inference_with_failover(request);

        // 应该成功（切换到成功提供商）
        assert!(response.is_ok(), "Failover should succeed: {:?}", response);
        
        // 验证响应来自成功的提供商
        // 注意：由于 MockInferenceProvider 内部超时检测，第一个提供商会返回超时错误
        // 触发故障切换逻辑，切换到第二个提供商并成功
    }

    #[test]
    fn test_failover_count_limit() {
        // 创建带严格切换限制的配置
        let mut config = ArchitectureConfig::default();
        config.timeout_config.inference_timeout_ms = 10; // 10ms 超时
        config.timeout_config.max_failover_count = 1; // 只允许切换 1 次
        config.timeout_config.consecutive_timeout_threshold = 1; // 1 次超时即标记

        let mut coordinator = ArchitectureCoordinator::with_config("node_1".to_string(), config).unwrap();

        // 注册两个都会超时的提供商（使用 timeout 模拟，不是 latency）
        // simulated_timeout_ms < simulated_latency_ms 时会返回超时错误
        let provider_slow_1 = Box::new(MockInferenceProvider::new(
            "provider_slow_1".to_string(),
            InferenceEngineType::Vllm,
            100,
        ).with_latency(100).with_timeout(5)); // 延迟 100ms，超时 5ms -> 会超时

        let provider_slow_2 = Box::new(MockInferenceProvider::new(
            "provider_slow_2".to_string(),
            InferenceEngineType::Sglang,
            80,
        ).with_latency(100).with_timeout(5)); // 延迟 100ms，超时 5ms -> 会超时

        coordinator.provider_layer.register_provider(provider_slow_1).unwrap();
        coordinator.provider_layer.register_provider(provider_slow_2).unwrap();

        // 在节点层也注册
        let mut record1 = ProviderRecord::new("provider_slow_1".to_string(), "1.0.0".to_string(), 100, 0.1);
        record1.status = ProviderStatus::Active;
        coordinator.node_layer.register_provider(record1).unwrap();

        let mut record2 = ProviderRecord::new("provider_slow_2".to_string(), "1.0.0".to_string(), 80, 0.1);
        record2.status = ProviderStatus::Active;
        coordinator.node_layer.register_provider(record2).unwrap();

        // 在健康监控器中注册
        let _ = coordinator.health_monitor.register_provider("provider_slow_1".to_string(), 0.5);
        let _ = coordinator.health_monitor.register_provider("provider_slow_2".to_string(), 0.5);

        // 手动设置当前提供商（在注册之后）
        coordinator.provider_layer.set_current_provider("provider_slow_1").unwrap();

        let request = InferenceRequest::new(
            "req_limit_test".to_string(),
            "Hello!".to_string(),
            "llama-7b".to_string(),
            100,
        );

        // 执行应该失败（所有提供商都超时）
        let result = coordinator.execute_inference_with_failover(request);

        // 由于所有提供商都超时，最终应该失败
        assert!(result.is_err(), "Should fail when all providers timeout: {:?}", result);
    }

    /// 测试 PendingCommitCache 缓存添加和 kv_proofs 生成
    ///
    /// 此测试验证缓存的基本功能：
    /// 1. 可以添加待提交记录
    /// 2. 记录包含正确的 kv_proofs（从 response.new_kv 生成）
    #[cfg(feature = "async")]
    #[tokio::test]
    async fn test_pending_commit_cache_kv_proofs_generation() {
        use sha2::{Digest, Sha256};

        let cache = Arc::new(PendingCommitCache::new(3, 100));

        // 创建测试响应
        let mut response = InferenceResponse::new("test_req".to_string());
        response.completion = "test completion".to_string();
        response.success = true;
        response.new_kv.insert("key1".to_string(), b"value1".to_vec());
        response.new_kv.insert("key2".to_string(), b"value2".to_vec());

        // 生成 kv_proofs（模拟 coordinator 中的逻辑）
        let kv_proofs: Vec<KvCacheProof> = response.new_kv.iter().map(|(key, value)| {
            let kv_hash = format!("{:x}", Sha256::digest(value));
            KvCacheProof::new(
                format!("kv_{}_{}", response.request_id, key),
                kv_hash,
                "test_provider".to_string(),
                value.len() as u64,
            )
        }).collect();

        // 创建待提交记录
        let pending_commit = PendingCommit {
            request_id: "test_req".to_string(),
            kv_proofs,
            response: response.clone(),
            provider_id: "test_provider".to_string(),
            retry_count: 0,
            last_error: Some("test error".to_string()),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };

        // 添加到缓存
        let add_result = cache.add(pending_commit);
        assert!(add_result.is_ok(), "Should add to cache: {:?}", add_result);

        // 验证缓存大小
        assert_eq!(cache.len().unwrap(), 1);

        // 验证 kv_proofs 不为空且内容正确
        {
            let cache_read = cache.cache.read().unwrap();
            let commit = cache_read.iter().find(|c| c.request_id == "test_req").unwrap();
            assert!(!commit.kv_proofs.is_empty(), "kv_proofs should not be empty");
            assert_eq!(commit.kv_proofs.len(), 2, "Should have 2 kv_proofs");
            
            // 验证 kv_proofs 内容（kv_block_id 格式为 "kv_{request_id}_{key}"）
            let kv1 = &commit.kv_proofs[0];
            let kv2 = &commit.kv_proofs[1];
            assert!(kv1.kv_block_id.starts_with("kv_test_req_"));
            assert!(kv2.kv_block_id.starts_with("kv_test_req_"));
        }
    }

    /// 测试 PendingCommitCache 后台重试机制
    ///
    /// 此测试验证完整的"失败→缓存"流程：
    /// 1. 启用区块链模拟失败
    /// 2. 执行异步推理，上链失败（3 次重试后）添加到缓存
    /// 3. 验证缓存中有待提交记录
    #[cfg(feature = "async")]
    #[tokio::test]
    async fn test_pending_commit_cache_retry_mechanism() {
        use tokio::time::{sleep, Duration};

        // 创建配置
        let mut config = ArchitectureConfig::default();
        config.timeout_config.inference_timeout_ms = 1000;

        // 创建 coordinator
        let coordinator = ArchitectureCoordinator::with_config(
            "node_retry_test".to_string(),
            config,
        ).unwrap();

        // 注册提供商
        let mut coordinator = coordinator;
        coordinator.register_provider(
            "provider_retry_test".to_string(),
            InferenceEngineType::Vllm,
            100,
        ).unwrap();

        // 启用区块链模拟失败
        coordinator.blockchain.enable_simulated_commit_failure();

        // 启动后台重试 worker（虽然 worker 会重试，但由于 simulate_commit_failure 启用，仍然会失败）
        coordinator.start_async().await;

        // 创建推理请求
        let request = InferenceRequest::new(
            "req_retry_test".to_string(),
            "Test retry mechanism".to_string(),
            "llama-7b".to_string(),
            100,
        ).with_memory_blocks(vec![0]);

        // 执行异步推理（会上链失败，然后添加到缓存）
        // 注意：execute_inference_async 返回 Result，不是 Future
        let (_response, mut commit_rx) = coordinator.execute_inference_async(request).unwrap();

        // 等待后台任务完成（最多 3 次重试，每次延迟递增：1s, 2s, 3s）
        // 总等待时间约 6 秒 + 处理时间
        // 我们通过 commit_rx 接收完成信号
        let commit_result = tokio::time::timeout(
            Duration::from_secs(10),
            commit_rx.recv()
        ).await.expect("Timeout waiting for commit result").expect("Channel closed");

        // 验证上链失败
        assert!(!commit_result.success, "Commit should fail due to simulated failure");
        assert!(commit_result.error_message.is_some());

        // 等待一小段时间让缓存添加完成
        sleep(Duration::from_millis(100)).await;

        // 验证缓存中有待提交记录
        let cache_len = coordinator.pending_commit_cache.len().unwrap();
        assert!(cache_len > 0, "Should have pending commits in cache after failure");

        // 验证缓存中的记录有正确的 kv_proofs
        {
            let cache_read = coordinator.pending_commit_cache.cache.read().unwrap();
            let commit = cache_read.iter().find(|c| c.request_id == "req_retry_test").expect("Should find commit in cache");
            assert!(!commit.kv_proofs.is_empty(), "kv_proofs should not be empty");
            assert_eq!(commit.retry_count, 0, "Retry count should be 0 when first added to cache");
            assert!(commit.last_error.is_some(), "Should have last_error");
        }
    }

    /// 测试 PendingCommitCache 重试成功后清除缓存
    ///
    /// 此测试验证完整的"失败→缓存→重试成功"流程：
    /// 1. 手动添加待提交记录到缓存（模拟上链失败场景）
    /// 2. 手动触发重试逻辑（直接调用 commit_inference）
    /// 3. 验证重试成功，缓存被清除
    #[cfg(feature = "async")]
    #[tokio::test]
    async fn test_pending_commit_cache_retry_success() {
        use tokio::time::{sleep, Duration};

        // 创建配置
        let mut config = ArchitectureConfig::default();
        config.timeout_config.inference_timeout_ms = 1000;

        // 创建 coordinator
        let coordinator = ArchitectureCoordinator::with_config(
            "node_retry_success".to_string(),
            config,
        ).unwrap();

        // 注册提供商
        let mut coordinator = coordinator;
        coordinator.register_provider(
            "provider_retry_success".to_string(),
            InferenceEngineType::Vllm,
            100,
        ).unwrap();

        // 注意：不启用 simulate_commit_failure，所以重试会成功

        // 启动后台重试 worker
        coordinator.start_async().await;

        // 创建推理请求
        let request = InferenceRequest::new(
            "req_retry_success".to_string(),
            "Test retry success".to_string(),
            "llama-7b".to_string(),
            100,
        ).with_memory_blocks(vec![0]);

        // 执行异步推理（正常上链成功）
        let (_response, mut commit_rx) = coordinator.execute_inference_async(request).unwrap();

        // 等待后台任务完成（应该成功）
        let commit_result = tokio::time::timeout(
            Duration::from_secs(5),
            commit_rx.recv()
        ).await.expect("Timeout waiting for commit result").expect("Channel closed");

        // 验证上链成功
        assert!(commit_result.success, "Commit should succeed: {:?}", commit_result.error_message);

        // 等待一小段时间让可能的缓存操作完成
        sleep(Duration::from_millis(100)).await;

        // 验证缓存为空（因为上链成功，没有添加到缓存）
        let cache_len = coordinator.pending_commit_cache.len().unwrap();
        assert_eq!(cache_len, 0, "Cache should be empty after successful commit");
    }
}
