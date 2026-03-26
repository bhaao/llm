use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use crate::traits::{Hashable, Serializable, Verifiable, Attestable, AttestationMetadata, AttestationType};
use crate::transaction::Transaction;
use crate::metadata::BlockMetadata;
use crate::utils::merkle_root;
use crate::quality_assessment::QualityProof;
use crate::audit::AuditTrail;
use crate::consensus::certificate::QuorumCertificate;

/// KV Cache 存证 - 用于链上存证 KV 数据
///
/// 对应创新点 A：KV Cache 链上存证
/// - 将 KV 块的哈希上链，实现数据不可篡改
/// - 可验证 KV 数据是否被恶意篡改
/// - 支持跨节点 KV 一致性校验
///
/// # 李群扩展（第四层）
///
/// 扩展支持李代数/李群承诺上链：
/// - `lie_algebra_commitment`: 李代数元素哈希 hash(A_i)
/// - `lie_group_root`: 全局李群状态哈希 hash(G)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KvCacheProof {
    /// KV 块标识
    pub kv_block_id: String,
    /// KV 数据哈希（上链的指纹）
    pub kv_hash: String,
    /// 所属节点 ID
    pub node_id: String,
    /// KV 块大小（token 数）
    pub kv_size: u64,
    /// 时间戳
    pub timestamp: u64,
    /// 李代数承诺哈希 hash(A_i)（可选，李群验证启用时填写）
    #[serde(default)]
    pub lie_algebra_commitment: Option<String>,
    /// 全局李群根哈希 hash(G)（可选，李群验证启用时填写）
    #[serde(default)]
    pub lie_group_root: Option<String>,
}

impl KvCacheProof {
    pub fn new(kv_block_id: String, kv_hash: String, node_id: String, kv_size: u64) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        KvCacheProof {
            kv_block_id,
            kv_hash,
            node_id,
            kv_size,
            timestamp,
            lie_algebra_commitment: None,
            lie_group_root: None,
        }
    }

    /// 创建带李群承诺的 KV 存证
    pub fn with_lie_group(
        kv_block_id: String,
        kv_hash: String,
        node_id: String,
        kv_size: u64,
        lie_algebra_commitment: String,
        lie_group_root: String,
    ) -> Self {
        let mut proof = Self::new(kv_block_id, kv_hash, node_id, kv_size);
        proof.lie_algebra_commitment = Some(lie_algebra_commitment);
        proof.lie_group_root = Some(lie_group_root);
        proof
    }

    /// 验证 KV 数据完整性
    pub fn verify_kv_integrity(&self, kv_data: &[u8]) -> bool {
        let computed_hash = Self::compute_hash(kv_data);
        computed_hash == self.kv_hash
    }

    fn compute_hash(data: &[u8]) -> String {
        format!("{:x}", Sha256::digest(data))
    }
}

impl Hashable for KvCacheProof {
    fn hash(&self) -> String {
        // 包含李群承诺字段（如果存在）
        let lie_alg = self.lie_algebra_commitment.as_deref().unwrap_or("");
        let lie_group = self.lie_group_root.as_deref().unwrap_or("");
        
        let data = format!(
            "{}:{}:{}:{}:{}:{}",
            self.kv_block_id,
            self.kv_hash,
            self.node_id,
            self.kv_size,
            lie_alg,
            lie_group
        );
        format!("{:x}", Sha256::digest(data.as_bytes()))
    }
}

/// 区块结构体
///
/// 每个区块记录一次分布式推理的上下文和存证信息：
/// - 交易：推理请求/响应记录
/// - KV Cache 存证：用于验证 KV 数据完整性（创新 A）
/// - 存证元数据：链上存证信息（创新 B/C/D）
/// - 质量证明：P11 要求 5 - 质量 proof 上链
/// - 共识证书：P11 要求 5 - 共识结果存证
/// - 审计链：P11 要求 5 - 全链路可追溯
///
/// **安全特性**：
/// - `is_sealed` 标记区块是否已提交到链上
/// - 一旦密封（sealed），不可修改区块内容（区块链不可变性）
/// - 尝试修改已密封区块会触发 panic（开发阶段快速发现问题）
#[derive(Debug, Serialize, Deserialize)]
pub struct Block {
    /// 区块高度
    pub index: u64,
    /// 时间戳
    pub timestamp: u64,
    /// 交易列表（推理请求/响应）
    pub transactions: Vec<Transaction>,
    /// 前一个区块的哈希
    pub previous_hash: String,
    /// 当前区块的哈希
    pub hash: String,
    /// 默克尔树根
    pub merkle_root: String,
    /// 区块元数据（推理信息）
    pub metadata: BlockMetadata,
    /// KV Cache 存证列表（创新 A）
    pub kv_proofs: Vec<KvCacheProof>,
    /// 存证元数据（链上存证信息）
    pub attestation: AttestationMetadata,
    /// 区块是否已密封（提交到链上后不可修改）
    #[serde(default)]
    pub is_sealed: bool,
    /// 质量证明列表（P11 要求 5）
    #[serde(default)]
    pub quality_proofs: Vec<QualityProof>,
    /// 共识证书列表（P11 要求 5）
    #[serde(default)]
    pub consensus_certificates: Vec<QuorumCertificate>,
    /// 审计链（P11 要求 5）
    #[serde(default)]
    pub audit_trail: AuditTrail,
}

impl Clone for Block {
    fn clone(&self) -> Self {
        Block {
            index: self.index,
            timestamp: self.timestamp,
            transactions: self.transactions.clone(),
            previous_hash: self.previous_hash.clone(),
            hash: self.hash.clone(),
            merkle_root: self.merkle_root.clone(),
            metadata: self.metadata.clone(),
            kv_proofs: self.kv_proofs.clone(),
            attestation: self.attestation.clone(),
            is_sealed: self.is_sealed,
            quality_proofs: self.quality_proofs.clone(),
            consensus_certificates: self.consensus_certificates.clone(),
            audit_trail: self.audit_trail.clone(),
        }
    }
}

impl Block {
    /// 创建新区块
    pub fn new(
        index: u64,
        previous_hash: String,
        transactions: Vec<Transaction>,
        metadata: BlockMetadata,
        kv_proofs: Vec<KvCacheProof>,
        attestation: AttestationMetadata,
    ) -> Self {
        let timestamp = Self::current_timestamp();
        let merkle_root = merkle_root(&transactions);

        let mut block = Block {
            index,
            timestamp,
            transactions,
            previous_hash,
            hash: String::new(),
            merkle_root,
            metadata,
            kv_proofs,
            attestation,
            is_sealed: false,
            quality_proofs: Vec::new(),
            consensus_certificates: Vec::new(),
            audit_trail: AuditTrail::new(),
        };

        block.hash = block.calculate_hash();
        block
    }

    /// 创建创世区块
    pub fn genesis(metadata: BlockMetadata) -> Self {
        let genesis_attestation = AttestationMetadata::new(
            String::from("genesis"),
            AttestationType::ContextMemory,
            1.0,
        );
        Block::new(0, String::from("0"), Vec::new(), metadata, Vec::new(), genesis_attestation)
    }

    /// 计算区块哈希
    ///
    /// 区块哈希由以下内容计算：
    /// - 区块高度、时间戳
    /// - 交易默克尔根
    /// - 前区块哈希
    /// - KV 存证默克尔根
    /// - 元数据哈希
    /// - 存证元数据哈希
    pub fn calculate_hash(&self) -> String {
        let kv_merkle = merkle_root(&self.kv_proofs);
        let data = format!(
            "{}:{}:{}:{}:{}:{}:{}",
            self.index,
            self.timestamp,
            self.merkle_root,
            self.previous_hash,
            kv_merkle,
            self.metadata.hash(),
            self.attestation.hash()
        );
        Self::sha256(&data)
    }

    /// SHA256 哈希辅助函数
    fn sha256(data: &str) -> String {
        format!("{:x}", Sha256::digest(data.as_bytes()))
    }

    /// 添加 KV Cache 存证
    ///
    /// # 注意
    ///
    /// 此方法仅在区块构建阶段使用（提交到链之前）。
    /// 一旦区块被密封（sealed），将不可修改其内容（区块链不可变性）。
    ///
    /// # Panics
    ///
    /// 如果区块已密封，调用此方法会触发 panic。
    ///
    /// # 使用场景
    ///
    /// - 在 `commit_inference` 调用前，逐步添加 KV 存证
    /// - 测试环境中构建测试区块
    ///
    /// # 安全
    ///
    /// 尝试修改已密封的区块会触发 panic，以便在开发阶段快速发现问题。
    pub fn add_kv_proof(&mut self, proof: KvCacheProof) {
        if self.is_sealed {
            panic!("Cannot modify sealed block at index {}", self.index);
        }
        self.kv_proofs.push(proof);
        // 重新计算哈希和默克尔根
        self.merkle_root = merkle_root(&self.transactions);
        self.hash = self.calculate_hash();
    }

    /// 密封区块（标记为已提交到链上）
    ///
    /// 一旦密封，区块内容不可修改。
    /// 此方法应在区块添加到区块链时调用。
    pub fn seal(&mut self) {
        self.is_sealed = true;
    }

    /// 检查区块是否已密封
    pub fn is_sealed(&self) -> bool {
        self.is_sealed
    }

    /// 获取区块中的交易数量
    pub fn transaction_count(&self) -> usize {
        self.transactions.len()
    }

    /// 获取 KV Cache 存证数量
    pub fn kv_proof_count(&self) -> usize {
        self.kv_proofs.len()
    }

    /// 获取总 Gas 使用量
    pub fn total_gas_used(&self) -> u64 {
        self.transactions.iter().map(|tx| tx.gas_used).sum()
    }

    /// 获取总 token 数
    pub fn total_tokens(&self) -> u64 {
        self.metadata.prompt_tokens + self.metadata.completion_tokens
    }

    /// 获取当前时间戳
    fn current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
}

impl Hashable for Block {
    fn hash(&self) -> String {
        self.hash.clone()
    }
}

impl Verifiable for Block {
    fn verify(&self) -> bool {
        let calculated_hash = self.calculate_hash();
        if calculated_hash != self.hash {
            return false;
        }

        let calculated_merkle = merkle_root(&self.transactions);
        if calculated_merkle != self.merkle_root {
            return false;
        }

        if !self.verify_attestation() {
            return false;
        }

        self.transactions.iter().all(|tx| tx.verify())
    }

    fn verify_with_error(&self) -> Result<(), String> {
        let calculated_hash = self.calculate_hash();
        if calculated_hash != self.hash {
            return Err(format!(
                "Hash mismatch: expected {}, got {}",
                self.hash, calculated_hash
            ));
        }

        let calculated_merkle = merkle_root(&self.transactions);
        if calculated_merkle != self.merkle_root {
            return Err(format!(
                "Merkle root mismatch: expected {}, got {}",
                self.merkle_root, calculated_merkle
            ));
        }

        self.verify_attestation_with_error()?;

        for (i, tx) in self.transactions.iter().enumerate() {
            tx.verify_with_error().map_err(|e| {
                format!("Transaction {} invalid: {}", i, e)
            })?;
        }

        Ok(())
    }
}

impl Serializable for Block {
    fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| format!("Failed to serialize block: {}", e))
    }

    fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("Failed to deserialize block: {}", e))
    }
}

impl Attestable for Block {
    fn attestation_hash(&self) -> String {
        self.hash.clone()
    }

    fn attestation_metadata(&self) -> &AttestationMetadata {
        &self.attestation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_kv_proof(node_id: &str) -> KvCacheProof {
        KvCacheProof::new(
            String::from("kv_001"),
            String::from("abc123"),
            node_id.to_string(),
            1024,
        )
    }

    fn create_test_attestation(node_id: &str) -> AttestationMetadata {
        AttestationMetadata::new(
            node_id.to_string(),
            AttestationType::KvCache,
            0.95,
        )
    }

    #[test]
    fn test_block_creation() {
        let metadata = BlockMetadata::default();
        let kv_proofs = vec![create_test_kv_proof("node_1")];
        let attestation = create_test_attestation("node_1");
        let block = Block::new(1, String::from("prev_hash"), Vec::new(), metadata, kv_proofs, attestation);

        assert_eq!(block.index, 1);
        assert_eq!(block.hash.len(), 64);
        assert_eq!(block.kv_proof_count(), 1);
    }

    #[test]
    fn test_genesis_block() {
        let metadata = BlockMetadata::default();
        let genesis = Block::genesis(metadata);

        assert_eq!(genesis.index, 0);
        assert_eq!(genesis.previous_hash, "0");
        assert_eq!(genesis.kv_proof_count(), 0);
    }

    #[test]
    fn test_block_verification() {
        let metadata = BlockMetadata::default();
        let kv_proofs = vec![create_test_kv_proof("node_1")];
        let attestation = create_test_attestation("node_1");
        let block = Block::new(1, String::from("prev_hash"), Vec::new(), metadata, kv_proofs, attestation);

        assert!(block.verify());
    }

    #[test]
    fn test_kv_proof_integrity() {
        let kv_data = b"test_kv_data";
        let kv_hash = KvCacheProof::compute_hash(kv_data);
        let proof = KvCacheProof::new(
            String::from("kv_001"),
            kv_hash,
            String::from("node_1"),
            1024,
        );

        assert!(proof.verify_kv_integrity(kv_data));

        let tampered_data = b"tampered_data";
        assert!(!proof.verify_kv_integrity(tampered_data));
    }

    #[test]
    fn test_block_serialization() {
        let metadata = BlockMetadata::default();
        let attestation = create_test_attestation("node_1");
        let block = Block::new(1, String::from("prev_hash"), Vec::new(), metadata, Vec::new(), attestation);

        let json = block.to_json().unwrap();
        let restored: Block = Block::from_json(&json).unwrap();

        assert_eq!(block.index, restored.index);
        assert_eq!(block.hash, restored.hash);
    }
}
