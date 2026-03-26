//! 存证服务 - 处理区块链存证
//!
//! **职责**：
//! - 将 KV Cache 哈希上链存证
//! - 将推理交易记录上链
//! - 验证链完整性
//!
//! **不依赖**：
//! - 不直接执行推理（由 InferenceOrchestrator 负责）
//! - 不直接监控健康（由 FailoverService 负责）

use std::sync::{Arc, RwLock};
use anyhow::Result;
use async_trait::async_trait;

use crate::blockchain::{Blockchain, BlockchainConfig};
use crate::block::KvCacheProof;
use crate::metadata::BlockMetadata;
use crate::transaction::{Transaction, TransactionType, TransactionPayload};
use crate::provider_layer::InferenceResponse;
use crate::services::CommitmentServiceTrait;

/// 存证服务
pub struct CommitmentService {
    /// 区块链（使用 Arc 包装，避免 Clone）
    blockchain: Arc<RwLock<Blockchain>>,
}

impl CommitmentService {
    /// 创建新的存证服务
    pub fn new(blockchain: Arc<RwLock<Blockchain>>) -> Self {
        CommitmentService { blockchain }
    }

    /// 创建区块链并返回存证服务
    pub fn with_config(
        owner_address: String,
        config: BlockchainConfig,
    ) -> Result<Self> {
        let blockchain = Blockchain::with_config(owner_address, config);

        Ok(CommitmentService {
            blockchain: Arc::new(RwLock::new(blockchain)),
        })
    }

    /// 获取区块链引用（只读）
    pub fn blockchain(&self) -> Arc<RwLock<Blockchain>> {
        self.blockchain.clone()
    }

    /// 提交 KV 存证到区块链
    ///
    /// # 参数
    /// - `kv_proof`: KV 存证
    ///
    /// # 返回
    /// - `Ok(())`: 成功
    /// - `Err(anyhow::Error)`: 错误上下文
    pub fn commit_kv_proof(&self, kv_proof: KvCacheProof) -> Result<()> {
        let mut bc = self.blockchain
            .write()
            .map_err(|e| anyhow::anyhow!("Blockchain lock poisoned: {}", e))?;

        bc.add_kv_proof(kv_proof);
        Ok(())
    }

    /// 提交推理交易到区块链
    ///
    /// # 参数
    /// - `from`: 发送方
    /// - `to`: 接收方
    /// - `_response`: 推理响应
    ///
    /// # 返回
    /// - `Ok(())`: 成功
    /// - `Err(anyhow::Error)`: 错误上下文
    pub fn commit_transaction(
        &self,
        from: String,
        to: String,
        _response: &InferenceResponse,
    ) -> Result<()> {
        let mut bc = self.blockchain
            .write()
            .map_err(|e| anyhow::anyhow!("Blockchain lock poisoned: {}", e))?;

        let tx = Transaction::new(
            from,
            to,
            TransactionType::InferenceResponse,
            TransactionPayload::None,
        );
        bc.add_pending_transaction(tx);

        Ok(())
    }

    /// 提交推理记录到区块链（包含 KV 存证和交易）
    ///
    /// # 参数
    /// - `metadata`: 区块元数据
    /// - `provider_id`: 提供商 ID
    /// - `response`: 推理响应
    /// - `kv_proofs`: KV 存证列表
    ///
    /// # 返回
    /// - `Ok(u64)`: 区块高度
    /// - `Err(anyhow::Error)`: 错误上下文
    pub fn commit_inference(
        &self,
        metadata: BlockMetadata,
        provider_id: &str,
        _response: &InferenceResponse,
        kv_proofs: Vec<KvCacheProof>,
    ) -> Result<u64> {
        let mut bc = self.blockchain
            .write()
            .map_err(|e| anyhow::anyhow!("Blockchain lock poisoned: {}", e))?;

        // 添加 KV 存证
        for kv_proof in kv_proofs {
            bc.add_kv_proof(kv_proof);
        }

        // 添加交易
        let owner_address = bc.owner_address().to_string();
        let tx = Transaction::new(
            provider_id.to_string(),
            owner_address,
            TransactionType::InferenceResponse,
            TransactionPayload::None,
        );
        bc.add_pending_transaction(tx);

        // 提交到区块
        let block = bc.commit_inference(metadata, provider_id.to_string())
            .map_err(|e| anyhow::anyhow!("Failed to commit to blockchain: {}", e))?;

        Ok(block.index)
    }

    /// 验证区块链完整性
    pub fn verify_blockchain(&self) -> bool {
        let bc = self.blockchain
            .read()
            .expect("Blockchain lock poisoned");
        bc.verify_chain()
    }

    /// 获取区块高度
    pub fn block_height(&self) -> u64 {
        let bc = self.blockchain
            .read()
            .expect("Blockchain lock poisoned");
        bc.chain().len() as u64
    }
}

#[async_trait]
impl CommitmentServiceTrait for CommitmentService {
    /// 提交 KV 存证和元数据到区块链
    async fn commit(&self, metadata: BlockMetadata, proofs: Vec<KvCacheProof>) -> Result<u64> {
        let mut bc = self.blockchain
            .write()
            .map_err(|e| anyhow::anyhow!("Blockchain lock poisoned: {}", e))?;

        // 添加 KV 存证
        for kv_proof in proofs {
            bc.add_kv_proof(kv_proof);
        }

        // 提交到区块
        let block = bc.commit_inference(metadata, "commitment_service".to_string())
            .map_err(|e| anyhow::anyhow!("Failed to commit to blockchain: {}", e))?;

        Ok(block.index)
    }

    /// 验证区块链完整性
    fn verify(&self) -> bool {
        let bc = self.blockchain
            .read()
            .expect("Blockchain lock poisoned");
        bc.verify_chain()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commitment_service_creation() {
        let service = CommitmentService::with_config(
            "address_1".to_string(),
            BlockchainConfig::default(),
        ).unwrap();

        assert_eq!(service.block_height(), 1); // 创世区块
    }

    #[test]
    fn test_commit_kv_proof() {
        let service = CommitmentService::with_config(
            "address_1".to_string(),
            BlockchainConfig::default(),
        ).unwrap();

        let kv_proof = KvCacheProof::new(
            "kv_001".to_string(),
            "hash_abc123".to_string(),
            "node_1".to_string(),
            1024,
        );

        assert!(service.commit_kv_proof(kv_proof).is_ok());
    }
}
