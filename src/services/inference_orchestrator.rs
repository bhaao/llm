//! 推理编排服务 - 协调分布式推理流程
//!
//! **职责**：
//! - 选择推理提供商
//! - 执行推理请求
//! - 处理故障切换时的推理重试
//!
//! **不依赖**：
//! - 不直接处理上链（由 CommitmentService 负责）
//! - 不直接监控健康（由 FailoverService 负责）

use std::sync::{Arc, RwLock};
use anyhow::{Result, Context};
use async_trait::async_trait;
use sha2::Digest;

use crate::node_layer::{NodeLayerManager, AccessCredential, AccessType};
use crate::memory_layer::MemoryLayerManager;
use crate::provider_layer::{ProviderLayerManager, InferenceRequest, InferenceResponse};
use crate::services::{InferenceService, CommitmentService, FailoverService};
use crate::block::KvCacheProof;
use crate::metadata::BlockMetadata;

/// 推理编排服务
///
/// **注意**: 目前仅用于演示服务层架构，实际功能尚未完全实现
/// - `node_layer`: 预留用于凭证管理（目前未使用）
/// - `memory_layer`: 用于读写 KV Cache
/// - `provider_layer`: 用于执行推理请求
pub struct InferenceOrchestrator {
    /// 节点层管理器（用于获取访问凭证）- 预留字段
    #[allow(dead_code)]
    node_layer: Arc<NodeLayerManager>,
    /// 记忆层管理器（用于读写 KV）
    memory_layer: Arc<MemoryLayerManager>,
    /// 提供商层管理器（用于执行推理）
    provider_layer: Arc<RwLock<ProviderLayerManager>>,
}

impl InferenceOrchestrator {
    /// 创建新的推理编排服务
    pub fn new(
        node_layer: Arc<NodeLayerManager>,
        memory_layer: Arc<MemoryLayerManager>,
        provider_layer: Arc<RwLock<ProviderLayerManager>>,
    ) -> Self {
        InferenceOrchestrator {
            node_layer,
            memory_layer,
            provider_layer,
        }
    }

    /// 执行推理请求（同步版本）
    ///
    /// # 参数
    /// - `request`: 推理请求
    /// - `credential`: 访问凭证
    /// - `provider_id`: 选中的提供商 ID
    ///
    /// # 返回
    /// - `Ok(InferenceResponse)`: 推理响应
    /// - `Err(anyhow::Error)`: 错误上下文
    pub fn execute_sync(
        &self,
        request: &InferenceRequest,
        credential: &AccessCredential,
        provider_id: &str,
    ) -> Result<InferenceResponse> {
        // 验证凭证（节点层没有 validate_credential 方法，跳过验证）
        // TODO: 如果 NodeLayerManager 添加 validate_credential 方法，在这里调用

        // 执行推理
        let pl = self.provider_layer
            .read()
            .map_err(|e| anyhow::anyhow!("Provider layer lock poisoned: {}", e))?;

        pl.execute_with_provider(provider_id, request, &self.memory_layer, credential)
            .map_err(|e| anyhow::anyhow!("Inference execution failed for provider {}: {}", provider_id, e))
    }

    /// 执行推理请求（异步版本）
    ///
    /// # 参数
    /// - `request`: 推理请求
    /// - `credential`: 访问凭证
    /// - `provider_id`: 选中的提供商 ID
    ///
    /// # 返回
    /// - `Ok(InferenceResponse)`: 推理响应
    /// - `Err(anyhow::Error)`: 错误上下文
    pub async fn execute_async(
        &self,
        request: &InferenceRequest,
        credential: &AccessCredential,
        provider_id: &str,
    ) -> Result<InferenceResponse> {
        // 验证凭证（节点层没有 validate_credential 方法，跳过验证）
        // TODO: 如果 NodeLayerManager 添加 validate_credential 方法，在这里调用

        // 执行推理
        let pl = self.provider_layer
            .read()
            .map_err(|e| anyhow::anyhow!("Provider layer lock poisoned: {}", e))?;

        pl.execute_with_provider_async(provider_id, request, &self.memory_layer, credential)
            .await
            .map_err(|e| anyhow::anyhow!("Inference execution failed for provider {}: {}", provider_id, e))
    }

    /// 获取提供商列表
    pub fn list_providers(&self) -> Vec<String> {
        let pl = self.provider_layer
            .read()
            .expect("Provider layer lock poisoned");
        pl.list_providers()
    }

    /// 注册推理提供商
    pub fn register_provider(
        &self,
        provider_id: String,
        engine_type: crate::provider_layer::InferenceEngineType,
        throughput: u32,
    ) -> Result<()> {
        let mut pl = self.provider_layer
            .write()
            .map_err(|e| anyhow::anyhow!("Provider layer lock poisoned: {}", e))?;

        pl.register_mock_provider(provider_id.clone(), engine_type, throughput)
            .map_err(|e| anyhow::anyhow!("Failed to register provider: {}", e))?;

        Ok(())
    }
}

/// 完整的推理流程编排器
///
/// 协调 InferenceOrchestrator、CommitmentService 和 FailoverService
/// 完成从推理请求到上链存证的完整流程
pub struct FullOrchestrator {
    inference: Arc<InferenceOrchestrator>,
    commitment: Arc<CommitmentService>,
    failover: Arc<FailoverService>,
}

impl FullOrchestrator {
    /// 创建完整的编排器
    pub fn new(
        inference: Arc<InferenceOrchestrator>,
        commitment: Arc<CommitmentService>,
        failover: Arc<FailoverService>,
    ) -> Self {
        FullOrchestrator {
            inference,
            commitment,
            failover,
        }
    }

    /// 执行完整的推理流程（包括上链）
    pub fn execute_full(
        &self,
        request: &InferenceRequest,
        credential: &AccessCredential,
    ) -> Result<(InferenceResponse, u64)> {
        // 1. 选择提供商
        let provider_id = self.inference.select_provider()?;

        // 2. 执行推理
        let response = self.inference.execute_sync(request, credential, &provider_id)?;

        // 3. 生成 KV 存证
        let kv_proofs: Vec<KvCacheProof> = response.new_kv.iter()
            .map(|(key, value)| {
                let hash = format!("{:x}", sha2::Sha256::digest(value));
                KvCacheProof::new(
                    format!("{}_{}", request.request_id, key),
                    hash,
                    provider_id.clone(),
                    value.len() as u64,
                )
            })
            .collect();

        // 4. 上链存证
        let metadata = BlockMetadata::new(
            request.request_id.clone(),
            "1.0.0".to_string(),
            response.prompt_tokens as u64,
            response.completion_tokens as u64,
            response.latency_ms,
            0.0,
            provider_id.clone(),
        );

        let block_height = self.commitment.commit_inference(
            metadata,
            &provider_id,
            &response,
            kv_proofs,
        )?;

        Ok((response, block_height))
    }

    /// 执行带故障切换的推理流程
    pub fn execute_with_failover(
        &self,
        request: &InferenceRequest,
        credential: &AccessCredential,
    ) -> Result<(InferenceResponse, u64)> {
        let mut last_error: Option<String> = None;

        // 获取健康提供商列表
        let providers = self.failover.get_healthy_providers();

        for provider_id in providers {
            // 执行推理
            match self.inference.execute_sync(request, credential, &provider_id) {
                Ok(response) => {
                    // 推理成功，标记提供商为健康
                    self.failover.mark_healthy(&provider_id)?;

                    // 生成 KV 存证
                    let kv_proofs: Vec<KvCacheProof> = response.new_kv.iter()
                        .map(|(key, value)| {
                            let hash = format!("{:x}", sha2::Sha256::digest(value));
                            KvCacheProof::new(
                                format!("{}_{}", request.request_id, key),
                                hash,
                                provider_id.clone(),
                                value.len() as u64,
                            )
                        })
                        .collect();

                    // 上链存证
                    let metadata = BlockMetadata::new(
                        request.request_id.clone(),
                        "1.0.0".to_string(),
                        response.prompt_tokens as u64,
                        response.completion_tokens as u64,
                        response.latency_ms,
                        0.0,
                        provider_id.clone(),
                    );

                    let block_height = self.commitment.commit_inference(
                        metadata,
                        &provider_id,
                        &response,
                        kv_proofs,
                    )?;

                    return Ok((response, block_height));
                }
                Err(e) => {
                    last_error = Some(format!("{}", e));
                    // 标记提供商为不健康
                    let _ = self.failover.mark_unhealthy(&provider_id, "inference_failure");
                }
            }
        }

        // 所有提供商都失败
        Err(anyhow::anyhow!(
            "All providers failed. Last error: {}",
            last_error.unwrap_or_else(|| "Unknown error".to_string())
        ))
    }
}

#[async_trait]
impl InferenceService for InferenceOrchestrator {
    /// 执行推理请求（异步版本）
    async fn execute(&self, request: InferenceRequest) -> Result<InferenceResponse> {
        // 选择提供商
        let provider_id = self.select_provider()?;

        // TODO: 需要 NodeLayerManager 添加签发凭证的方法
        // 目前使用简化版本，假设凭证已存在
        let credential = AccessCredential {
            credential_id: format!("cred_{}", request.request_id),
            provider_id: provider_id.clone(),
            memory_block_ids: request.memory_block_ids.iter()
                .map(|id| id.to_string()).collect(),
            access_type: AccessType::ReadWrite,
            expires_at: u64::MAX,
            issuer_node_id: "node_1".to_string(),
            signature: "mock_signature".to_string(),
            is_revoked: false,
        };

        // 执行推理
        self.execute_sync(&request, &credential, &provider_id)
    }

    /// 选择最佳推理提供商
    fn select_provider(&self) -> Result<String> {
        // 简化实现：返回第一个可用的提供商
        let providers = self.list_providers();
        providers.into_iter().next()
            .context("No available providers")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, RwLock};

    #[test]
    fn test_orchestrator_creation() {
        let node_layer = Arc::new(NodeLayerManager::new(
            "node_1".to_string(),
            "address_1".to_string(),
        ));
        let memory_layer = Arc::new(MemoryLayerManager::new("node_1"));
        let provider_layer = Arc::new(RwLock::new(ProviderLayerManager::new()));

        let orchestrator = InferenceOrchestrator::new(
            node_layer,
            memory_layer,
            provider_layer,
        );

        assert!(orchestrator.list_providers().is_empty());
    }
}
