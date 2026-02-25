use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use ed25519_dalek::{VerifyingKey, Signature, Verifier};

/// 可哈希 trait，用于生成对象的哈希值
pub trait Hashable {
    /// 计算并返回对象的哈希值
    fn hash(&self) -> String;

    /// 计算默克尔根
    ///
    /// **已废弃**：请使用 `crate::utils::merkle_root()` 函数
    ///
    /// 此方法保留仅用于向后兼容，新代码应直接使用工具函数
    #[deprecated(since = "0.1.1", note = "请使用 crate::utils::merkle_root() 函数")]
    fn merkle_root(items: &[impl Hashable]) -> String {
        if items.is_empty() {
            return Self::empty_hash();
        }

        let mut hashes: Vec<String> = items.iter().map(|t| t.hash()).collect();

        while hashes.len() > 1 {
            let mut new_hashes = Vec::new();
            for chunk in hashes.chunks(2) {
                let combined = match chunk.len() {
                    2 => format!("{}{}", chunk[0], chunk[1]),
                    1 => format!("{}{}", chunk[0], chunk[0]),
                    _ => unreachable!(),
                };
                new_hashes.push(Self::sha256(&combined));
            }
            hashes = new_hashes;
        }

        hashes.into_iter().next().unwrap_or_else(Self::empty_hash)
    }

    /// 空哈希值
    fn empty_hash() -> String {
        "0000000000000000000000000000000000000000000000000000000000000000".to_string()
    }

    /// SHA256 哈希辅助函数（使用 sha2 crate）
    fn sha256(data: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data.as_bytes());
        let result = hasher.finalize();
        format!("{:x}", result)
    }
}

/// 可序列化 trait，用于对象的序列化和反序列化
pub trait Serializable: Sized {
    /// 序列化为 JSON 字符串
    fn to_json(&self) -> Result<String, String>;

    /// 从 JSON 字符串反序列化
    fn from_json(json: &str) -> Result<Self, String>;
}

/// 可验证 trait，用于验证对象的有效性
pub trait Verifiable {
    /// 验证对象是否有效
    fn verify(&self) -> bool;

    /// 验证并返回错误信息
    fn verify_with_error(&self) -> Result<(), String>;
}

/// 签名验证器 trait - 用于插件式签名验证
///
/// 生产级解决方案：
/// - 支持多种签名算法（ECDSA、EdDSA 等）
/// - 可切换真实验证和模拟验证（用于测试）
pub trait SignatureVerifier: Send + Sync {
    /// 验证签名
    ///
    /// 参数：
    /// - message: 原始消息
    /// - signature: 签名
    /// - public_key: 公钥
    fn verify_signature(&self, message: &str, signature: &str, public_key: &str) -> bool;

    /// 克隆为 Box（用于对象安全克隆）
    fn clone_box(&self) -> Box<dyn SignatureVerifier>;
}

/// 为 Box<dyn SignatureVerifier> 实现 Clone
impl Clone for Box<dyn SignatureVerifier> {
    fn clone(&self) -> Self {
        self.as_ref().clone_box()
    }
}

/// 空验证器 - 用于测试环境（始终返回 true）
pub struct NullVerifier;

impl SignatureVerifier for NullVerifier {
    fn verify_signature(&self, _message: &str, _signature: &str, _public_key: &str) -> bool {
        true
    }

    fn clone_box(&self) -> Box<dyn SignatureVerifier> {
        Box::new(NullVerifier)
    }
}

/// 简单验证器 - 基于简单哈希的模拟验证（用于原型开发）
pub struct SimpleVerifier;

impl SignatureVerifier for SimpleVerifier {
    fn verify_signature(&self, message: &str, signature: &str, _public_key: &str) -> bool {
        // 模拟验证：签名必须是消息的 SHA256 哈希
        let expected = <Sha256 as Digest>::digest(message.as_bytes());
        let expected_hex = format!("{:x}", expected);
        signature == expected_hex
    }

    fn clone_box(&self) -> Box<dyn SignatureVerifier> {
        Box::new(SimpleVerifier)
    }
}

/// Ed25519 签名验证器 - 生产级真实签名验证
///
/// 使用 ed25519-dalek crate 实现真实的 Ed25519 数字签名
pub struct Ed25519Verifier;

impl Ed25519Verifier {
    pub fn new() -> Self {
        Ed25519Verifier
    }
}

impl Default for Ed25519Verifier {
    fn default() -> Self {
        Self::new()
    }
}

impl SignatureVerifier for Ed25519Verifier {
    fn verify_signature(&self, message: &str, signature: &str, public_key: &str) -> bool {
        // 解析公钥和签名
        let public_key_bytes = match hex::decode(public_key) {
            Ok(bytes) => bytes,
            Err(_) => return false,
        };

        let signature_bytes = match hex::decode(signature) {
            Ok(bytes) => bytes,
            Err(_) => return false,
        };

        // 转换为 VerifyingKey 和 Signature
        let verifying_key = match VerifyingKey::from_bytes(&public_key_bytes.try_into().unwrap_or([0u8; 32])) {
            Ok(key) => key,
            Err(_) => return false,
        };

        let signature: Signature = match Signature::try_from(&signature_bytes[..]) {
            Ok(sig) => sig,
            Err(_) => return false,
        };

        // 验证签名
        verifying_key.verify(message.as_bytes(), &signature).is_ok()
    }

    fn clone_box(&self) -> Box<dyn SignatureVerifier> {
        Box::new(Ed25519Verifier)
    }
}

/// 可存证 trait - 用于 KV Cache 链上存证
///
/// 区块链在分布式 LLM 中的正确定位是"可信增强工具"：
/// - 将 KV Cache 的哈希上链，实现数据不可篡改
/// - 将推理记录上链，实现可追溯、可验证
/// - 将节点信誉上链，实现可信调度
///
/// 这与 PoW 挖矿无关，而是利用区块链的"分布式账本"特性
pub trait Attestable {
    /// 获取存证哈希（上链的数据指纹）
    fn attestation_hash(&self) -> String;

    /// 获取存证元数据
    fn attestation_metadata(&self) -> &AttestationMetadata;

    /// 验证存证是否有效
    fn verify_attestation(&self) -> bool {
        let metadata = self.attestation_metadata();
        !metadata.attester_id.is_empty() && metadata.timestamp > 0
    }

    /// 验证存证并返回错误
    fn verify_attestation_with_error(&self) -> Result<(), String> {
        let metadata = self.attestation_metadata();

        if metadata.attester_id.is_empty() {
            return Err("Missing attester ID".to_string());
        }

        if metadata.timestamp == 0 {
            return Err("Invalid timestamp".to_string());
        }

        Ok(())
    }
}

/// 存证元数据 - 记录链上存证的关键信息
///
/// 对应创新点：
/// - 区块链创新 A：KV Cache 链上存证
/// - 区块链创新 B：链上可信分布式调度
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttestationMetadata {
    /// 存证者 ID（节点标识）
    pub attester_id: String,

    /// 存证时间戳
    pub timestamp: u64,

    /// 存证类型
    pub attestation_type: AttestationType,

    /// 链上交易哈希（存证所在的区块链交易）
    pub chain_tx_hash: Option<String>,

    /// 区块高度（存证所在的区块）
    pub block_height: Option<u64>,

    /// 信誉评分（用于节点信誉管理）
    pub reputation_score: f64,
}

impl AttestationMetadata {
    pub fn new(
        attester_id: String,
        attestation_type: AttestationType,
        reputation_score: f64,
    ) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        AttestationMetadata {
            attester_id,
            timestamp,
            attestation_type,
            chain_tx_hash: None,
            block_height: None,
            reputation_score,
        }
    }

    pub fn with_chain_info(
        mut self,
        chain_tx_hash: String,
        block_height: u64,
    ) -> Self {
        self.chain_tx_hash = Some(chain_tx_hash);
        self.block_height = Some(block_height);
        self
    }
}

impl Hashable for AttestationMetadata {
    fn hash(&self) -> String {
        // 修复：包含所有字段，包括 block_height
        let data = format!(
            "{}:{}:{}:{}:{}:{}",
            self.attester_id,
            self.timestamp,
            self.attestation_type.as_str(),
            self.reputation_score,
            self.chain_tx_hash.as_deref().unwrap_or(""),
            self.block_height.unwrap_or(0)
        );
        Self::sha256(&data)
    }
}

/// 存证类型 - 区分不同类型的链上存证
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AttestationType {
    /// KV Cache 存证（创新 A）
    KvCache,
    /// 调度记录存证（创新 B）
    Scheduling,
    /// 上下文记忆存证（创新 C）
    ContextMemory,
    /// 资源贡献存证（创新 D）
    ResourceContribution,
    /// 节点信誉存证
    NodeReputation,
}

impl AttestationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AttestationType::KvCache => "kv_cache",
            AttestationType::Scheduling => "scheduling",
            AttestationType::ContextMemory => "context_memory",
            AttestationType::ResourceContribution => "resource_contribution",
            AttestationType::NodeReputation => "node_reputation",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Sha256;

    #[test]
    fn test_sha256_hash() {
        let hash = <AttestationMetadata as Hashable>::sha256("test_data");
        assert_eq!(hash.len(), 64); // SHA256 输出 64 个十六进制字符
    }

    #[test]
    fn test_merkle_root() {
        let items = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        // 使用简单的 Hashable 实现测试
        #[derive(Debug)]
        struct TestItem(String);
        impl Hashable for TestItem {
            fn hash(&self) -> String {
                <AttestationMetadata as Hashable>::sha256(&self.0)
            }
        }

        let test_items: Vec<TestItem> = items.into_iter().map(TestItem).collect();
        let root = crate::utils::merkle_root(&test_items);
        assert_eq!(root.len(), 64);
    }

    #[test]
    fn test_simple_verifier() {
        let verifier = SimpleVerifier;
        let message = "test message";
        let signature = format!("{:x}", Sha256::digest(message.as_bytes()));
        
        assert!(verifier.verify_signature(message, &signature, "key"));
        assert!(!verifier.verify_signature("wrong message", &signature, "key"));
    }

    #[test]
    fn test_attestation_metadata_hash_includes_all_fields() {
        let metadata = AttestationMetadata::new(
            "node_1".to_string(),
            AttestationType::KvCache,
            0.95,
        ).with_chain_info("tx_hash_123".to_string(), 100);

        let hash1 = metadata.hash();
        
        // 修改 block_height 应该改变哈希
        let mut metadata2 = metadata.clone();
        metadata2.block_height = Some(200);
        let hash2 = metadata2.hash();
        
        assert_ne!(hash1, hash2, "block_height 变化应该影响哈希值");
    }
}
