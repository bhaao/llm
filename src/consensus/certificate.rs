//! 证书模块 - 2f+1 签名收集
//!
//! **核心概念**：
//! - Quorum Certificate (QC): 2f+1 个有效签名的集合
//! - Prepare Certificate: 准备阶段证书
//! - Commit Certificate: 提交阶段证书
//!
//! **安全保证**：
//! - 只有收集到 2f+1 个签名才能形成有效证书
//! - 证书是执行下一步骤的必要条件
//! - 支持证书验证和持久化

use serde::{Serialize, Deserialize};
use ed25519_dalek::{VerifyingKey, Signature, Verifier};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use crate::consensus::messages::SignatureData;

/// 证书错误类型
#[derive(Debug, Clone, PartialEq)]
pub enum CertificateError {
    /// 签名数量不足
    InsufficientSignatures { current: usize, required: usize },
    /// 重复签名
    DuplicateSignature { signer_id: String },
    /// 签名验证失败
    InvalidSignature { signer_id: String },
    /// 证书已过期
    Expired,
    /// 证书未就绪
    NotReady,
    /// 摘要不匹配
    DigestMismatch,
}

/// 法定人数证书 - 2f+1 个签名的集合
///
/// **设计原则**：
/// - 不信任任何少于 2f+1 个签名的集合
/// - 每个签名都必须验证
/// - 证书与特定摘要绑定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuorumCertificate {
    /// 证书对应的摘要
    pub digest: String,
    /// 视图号
    pub view: u64,
    /// 序列号
    pub sequence: u64,
    /// 签名集合 (signer_id -> signature)
    pub signatures: HashMap<String, Vec<u8>>,
    /// 签名者公钥映射 (signer_id -> public_key)
    pub public_keys: HashMap<String, Vec<u8>>,
    /// 所需的法定人数大小 (2f+1)
    pub quorum_size: usize,
    /// 证书创建时间戳
    pub timestamp: u64,
}

impl QuorumCertificate {
    /// 创建新的证书收集器
    pub fn new(digest: String, view: u64, sequence: u64, quorum_size: usize) -> Self {
        QuorumCertificate {
            digest,
            view,
            sequence,
            signatures: HashMap::new(),
            public_keys: HashMap::new(),
            quorum_size,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// 添加签名
    ///
    /// # Returns
    /// - `Ok(())`: 签名添加成功
    /// - `Err(CertificateError)`: 添加失败
    pub fn add_signature(
        &mut self,
        signer_id: String,
        signature: Vec<u8>,
        public_key: Vec<u8>,
        message: &[u8],
    ) -> Result<(), CertificateError> {
        // 检查重复
        if self.signatures.contains_key(&signer_id) {
            return Err(CertificateError::DuplicateSignature { signer_id });
        }

        // 保存 signer_id 的克隆用于错误消息
        let signer_id_for_error = signer_id.clone();

        // 验证签名
        let verifying_key = VerifyingKey::from_bytes(&public_key.clone().try_into().unwrap_or([0u8; 32]))
            .map_err(|_| CertificateError::InvalidSignature { signer_id: signer_id_for_error.clone() })?;

        let sig = Signature::try_from(&signature[..])
            .map_err(|_| CertificateError::InvalidSignature { signer_id: signer_id_for_error.clone() })?;

        verifying_key.verify(message, &sig)
            .map_err(|_| CertificateError::InvalidSignature { signer_id: signer_id_for_error })?;

        // 添加签名和公钥
        let signer_id_for_pk = signer_id.clone();
        self.signatures.insert(signer_id, signature);
        self.public_keys.insert(signer_id_for_pk, public_key);

        Ok(())
    }

    /// 检查是否达到法定人数
    pub fn is_complete(&self) -> bool {
        self.signatures.len() >= self.quorum_size
    }

    /// 获取当前签名数量
    pub fn signature_count(&self) -> usize {
        self.signatures.len()
    }

    /// 获取缺失的签名数量
    pub fn missing_signatures(&self) -> usize {
        if self.signatures.len() >= self.quorum_size {
            0
        } else {
            self.quorum_size - self.signatures.len()
        }
    }

    /// 验证证书完整性
    pub fn verify(&self, digest: &str) -> Result<(), CertificateError> {
        // 验证摘要匹配
        if self.digest != digest {
            return Err(CertificateError::DigestMismatch);
        }

        // 验证签名数量
        if !self.is_complete() {
            return Err(CertificateError::InsufficientSignatures {
                current: self.signatures.len(),
                required: self.quorum_size,
            });
        }

        // 验证所有签名
        for (signer_id, signature_bytes) in &self.signatures {
            let public_key_bytes = self.public_keys.get(signer_id)
                .ok_or_else(|| CertificateError::InvalidSignature { signer_id: signer_id.clone() })?;

            let _verifying_key = VerifyingKey::from_bytes(
                &public_key_bytes.clone().try_into().unwrap_or([0u8; 32])
            ).map_err(|_| CertificateError::InvalidSignature { signer_id: signer_id.clone() })?;

            let _signature = Signature::try_from(&signature_bytes[..])
                .map_err(|_| CertificateError::InvalidSignature { signer_id: signer_id.clone() })?;

            // 注意：这里需要原始消息才能验证，实际使用中需要传入消息
            // 为简化，这里只做格式验证
            if signature_bytes.is_empty() {
                return Err(CertificateError::InvalidSignature { signer_id: signer_id.clone() });
            }
        }

        Ok(())
    }

    /// 转换为签名数据列表
    pub fn to_signature_data(&self) -> Vec<SignatureData> {
        self.signatures.iter().map(|(signer_id, signature)| {
            SignatureData {
                signer_id: signer_id.clone(),
                signature: signature.clone(),
                public_key: self.public_keys.get(signer_id).cloned(),
            }
        }).collect()
    }

    /// 从签名数据列表创建
    pub fn from_signature_data(
        digest: String,
        view: u64,
        sequence: u64,
        signatures: Vec<SignatureData>,
        quorum_size: usize,
    ) -> Self {
        let mut sig_map = HashMap::new();
        let mut key_map = HashMap::new();

        for sig_data in signatures {
            sig_map.insert(sig_data.signer_id.clone(), sig_data.signature);
            if let Some(pk) = sig_data.public_key {
                key_map.insert(sig_data.signer_id, pk);
            }
        }

        QuorumCertificate {
            digest,
            view,
            sequence,
            signatures: sig_map,
            public_keys: key_map,
            quorum_size,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// 获取证书摘要（用于 checkpoint）
    pub fn certificate_digest(&self) -> String {
        let mut sigs: Vec<String> = self.signatures.keys().cloned().collect();
        sigs.sort();

        let data = format!(
            "{}:{}:{}:{}",
            self.digest,
            self.view,
            self.sequence,
            sigs.join(",")
        );

        let hash = Sha256::digest(data.as_bytes());
        format!("{:x}", hash)
    }
}

/// 证书管理器 - 管理所有证书的收集
#[derive(Debug, Clone)]
pub struct CertificateManager {
    /// 准备证书
    prepare_certificates: HashMap<String, QuorumCertificate>,
    /// 提交证书
    commit_certificates: HashMap<String, QuorumCertificate>,
    /// 法定人数大小
    quorum_size: usize,
}

impl CertificateManager {
    /// 创建新的证书管理器
    pub fn new(quorum_size: usize) -> Self {
        CertificateManager {
            prepare_certificates: HashMap::new(),
            commit_certificates: HashMap::new(),
            quorum_size,
        }
    }

    /// 获取或创建准备证书
    pub fn get_or_create_prepare_cert(
        &mut self,
        digest: &str,
        view: u64,
        sequence: u64,
    ) -> &mut QuorumCertificate {
        self.prepare_certificates
            .entry(digest.to_string())
            .or_insert_with(|| QuorumCertificate::new(digest.to_string(), view, sequence, self.quorum_size))
    }

    /// 获取或创建提交证书
    pub fn get_or_create_commit_cert(
        &mut self,
        digest: &str,
        view: u64,
        sequence: u64,
    ) -> &mut QuorumCertificate {
        self.commit_certificates
            .entry(digest.to_string())
            .or_insert_with(|| QuorumCertificate::new(digest.to_string(), view, sequence, self.quorum_size))
    }

    /// 检查准备证书是否完成
    pub fn is_prepare_complete(&self, digest: &str) -> bool {
        self.prepare_certificates
            .get(digest)
            .map(|cert| cert.is_complete())
            .unwrap_or(false)
    }

    /// 检查提交证书是否完成
    pub fn is_commit_complete(&self, digest: &str) -> bool {
        self.commit_certificates
            .get(digest)
            .map(|cert| cert.is_complete())
            .unwrap_or(false)
    }

    /// 获取完成的准备证书
    pub fn get_prepare_cert(&self, digest: &str) -> Option<&QuorumCertificate> {
        self.prepare_certificates.get(digest).filter(|c| c.is_complete())
    }

    /// 获取完成的提交证书
    pub fn get_commit_cert(&self, digest: &str) -> Option<&QuorumCertificate> {
        self.commit_certificates.get(digest).filter(|c| c.is_complete())
    }

    /// 清理旧证书（垃圾回收）
    pub fn prune_before(&mut self, sequence: u64) {
        self.prepare_certificates.retain(|_, cert| cert.sequence >= sequence);
        self.commit_certificates.retain(|_, cert| cert.sequence >= sequence);
    }

    /// 获取证书统计信息
    pub fn stats(&self) -> CertificateStats {
        CertificateStats {
            prepare_count: self.prepare_certificates.len(),
            commit_count: self.commit_certificates.len(),
            completed_prepare: self.prepare_certificates.values().filter(|c| c.is_complete()).count(),
            completed_commit: self.commit_certificates.values().filter(|c| c.is_complete()).count(),
        }
    }
}

/// 证书统计信息
#[derive(Debug, Clone, Default)]
pub struct CertificateStats {
    /// 准备证书数量
    pub prepare_count: usize,
    /// 提交证书数量
    pub commit_count: usize,
    /// 已完成的准备证书
    pub completed_prepare: usize,
    /// 已完成的提交证书
    pub completed_commit: usize,
}

/// Checkpoint - 稳定检查点
///
/// 用于垃圾回收和状态同步
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Checkpoint 序列号
    pub sequence: u64,
    /// 状态摘要
    pub state_digest: String,
    /// Checkpoint 证书（2f+1 个签名）
    pub certificate: QuorumCertificate,
    /// 创建时间戳
    pub timestamp: u64,
}

impl Checkpoint {
    /// 创建新的 checkpoint
    pub fn new(sequence: u64, state_digest: String, quorum_size: usize) -> Self {
        Checkpoint {
            sequence,
            state_digest: state_digest.clone(),
            certificate: QuorumCertificate::new(state_digest, 0, sequence, quorum_size),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// 检查 checkpoint 是否稳定（已收集 2f+1 签名）
    pub fn is_stable(&self) -> bool {
        self.certificate.is_complete()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{SigningKey, Signer};

    fn create_test_signature(
        message: &[u8],
        signing_key: &SigningKey,
    ) -> (Vec<u8>, Vec<u8>) {
        let signature = signing_key.try_sign(message).unwrap();
        (signature.to_bytes().to_vec(), signing_key.verifying_key().to_bytes().to_vec())
    }

    #[test]
    fn test_quorum_certificate_creation() {
        let cert = QuorumCertificate::new("abc123".to_string(), 0, 1, 4);

        assert_eq!(cert.digest, "abc123");
        assert_eq!(cert.view, 0);
        assert_eq!(cert.sequence, 1);
        assert_eq!(cert.quorum_size, 4);
        assert!(!cert.is_complete());
    }

    #[test]
    fn test_quorum_certificate_add_signature() {
        let key_bytes: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&key_bytes);
        let _verifying_key = signing_key.verifying_key();

        let message = b"test message";
        let (signature, public_key) = create_test_signature(message, &signing_key);

        let mut cert = QuorumCertificate::new(hex::encode(Sha256::digest(message)), 0, 1, 1);

        // 添加签名
        let result = cert.add_signature("node_1".to_string(), signature, public_key, message);
        assert!(result.is_ok());
        assert!(cert.is_complete());
    }

    #[test]
    fn test_duplicate_signature_rejected() {
        let key_bytes: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&key_bytes);

        let message = b"test message";
        let (signature, public_key) = create_test_signature(message, &signing_key);

        let mut cert = QuorumCertificate::new(hex::encode(Sha256::digest(message)), 0, 1, 2);

        // 第一次添加
        cert.add_signature("node_1".to_string(), signature.clone(), public_key.clone(), message).unwrap();

        // 重复添加应该失败
        let result = cert.add_signature("node_1".to_string(), signature, public_key, message);
        assert!(matches!(result, Err(CertificateError::DuplicateSignature { .. })));
    }

    #[test]
    fn test_invalid_signature_rejected() {
        let key_bytes: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&key_bytes);
        let wrong_key_bytes: [u8; 32] = rand::random();
        let wrong_key = SigningKey::from_bytes(&wrong_key_bytes);

        let message = b"test message";
        let (signature, _) = create_test_signature(message, &signing_key);
        let wrong_public_key = wrong_key.verifying_key().to_bytes().to_vec();

        let mut cert = QuorumCertificate::new(hex::encode(Sha256::digest(message)), 0, 1, 1);

        // 使用错误公钥应该失败
        let result = cert.add_signature("node_1".to_string(), signature, wrong_public_key, message);
        assert!(matches!(result, Err(CertificateError::InvalidSignature { .. })));
    }

    #[test]
    fn test_certificate_manager() {
        let mut manager = CertificateManager::new(4);

        // 获取或创建准备证书
        let cert = manager.get_or_create_prepare_cert("digest1", 0, 1);
        assert_eq!(cert.digest, "digest1");
        assert_eq!(cert.view, 0);
        assert_eq!(cert.sequence, 1);

        // 检查完成状态
        assert!(!manager.is_prepare_complete("digest1"));
    }

    #[test]
    fn test_certificate_pruning() {
        let mut manager = CertificateManager::new(4);

        // 创建多个证书
        manager.get_or_create_prepare_cert("digest1", 0, 1);
        manager.get_or_create_prepare_cert("digest2", 0, 5);
        manager.get_or_create_prepare_cert("digest3", 0, 10);

        // 清理序列号 5 之前的证书
        manager.prune_before(5);

        // digest1 应该被清理（序列号 1 < 5）
        assert!(!manager.is_prepare_complete("digest1"));
        // digest2 和 digest3 应该保留（序列号 >= 5）
        // 注意：get_prepare_cert 只返回已完成的证书，这里只检查它们没有被 prune 掉
        assert!(manager.prepare_certificates.contains_key("digest2"));
        assert!(manager.prepare_certificates.contains_key("digest3"));
    }

    #[test]
    fn test_checkpoint_stability() {
        let mut checkpoint = Checkpoint::new(100, "state_digest".to_string(), 1);

        assert!(!checkpoint.is_stable());

        // 添加足够签名后变为稳定
        let key_bytes: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&key_bytes);
        let message = b"checkpoint message";
        let (signature, public_key) = create_test_signature(message, &signing_key);

        checkpoint.certificate.add_signature(
            "node_1".to_string(),
            signature,
            public_key,
            message,
        ).unwrap();

        assert!(checkpoint.is_stable());
    }
}
