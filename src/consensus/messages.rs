//! PBFT 消息类型 - 三阶段提交消息定义
//!
//! 实现 PBFT 三阶段提交的消息类型：
//! - Request: 客户端请求
//! - PrePrepare: Leader 广播预准备消息
//! - Prepare: Replica 广播准备消息
//! - Commit: Replica 广播提交消息
//! - Reply: Replica 回复客户端
//! - ViewChange: 视图切换消息
//! - NewView: 新视图通知

use serde::{Serialize, Deserialize};
use ed25519_dalek::{Signature, VerifyingKey, SigningKey, Signer, Verifier};
use sha2::{Sha256, Digest};

/// 消息类型枚举
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MessageType {
    /// 客户端请求
    Request,
    /// 预准备消息（Leader only）
    PrePrepare,
    /// 准备消息（Replica）
    Prepare,
    /// 提交消息（Replica）
    Commit,
    /// 回复消息（Replica → Client）
    Reply,
    /// 视图切换消息
    ViewChange,
    /// 新视图通知（New Leader）
    NewView,
}

/// PBFT 消息体
///
/// **设计原则**：
/// - 所有消息都包含视图号和序列号，防止重放攻击
/// - 所有消息都需要签名，防止伪造
/// - 消息包含完整的上下文信息，支持幂等处理
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PBFTMessage {
    /// 客户端请求
    Request {
        /// 客户端 ID
        client_id: String,
        /// 操作内容（序列化后的数据）
        operation: Vec<u8>,
        /// 时间戳（防止重放）
        timestamp: u64,
    },
    /// 预准备消息（Leader only）
    PrePrepare {
        /// 视图号
        view: u64,
        /// 序列号
        sequence: u64,
        /// 请求摘要（哈希）
        digest: String,
        /// Leader 节点 ID
        leader: String,
        /// 原始请求数据
        request: Vec<u8>,
    },
    /// 准备消息（Replica）
    Prepare {
        /// 视图号
        view: u64,
        /// 序列号
        sequence: u64,
        /// 请求摘要
        digest: String,
        /// Replica 节点 ID
        replica_id: String,
        /// 签名
        signature: Vec<u8>,
    },
    /// 提交消息（Replica）
    Commit {
        /// 视图号
        view: u64,
        /// 序列号
        sequence: u64,
        /// 请求摘要
        digest: String,
        /// Replica 节点 ID
        replica_id: String,
        /// 签名
        signature: Vec<u8>,
    },
    /// 回复消息（Replica → Client）
    Reply {
        /// 视图号
        view: u64,
        /// 时间戳
        timestamp: u64,
        /// 客户端 ID
        client_id: String,
        /// Replica 节点 ID
        replica_id: String,
        /// 执行结果
        result: Vec<u8>,
        /// 签名
        signature: Vec<u8>,
    },
    /// 视图切换消息
    ViewChange {
        /// 新视图号
        new_view: u64,
        /// 发送节点 ID
        sender_id: String,
        /// 最后稳定 checkpoint 的序列号
        last_stable_checkpoint: u64,
        /// checkpoint 的摘要
        checkpoint_digest: String,
        /// 已准备但未提交的消息日志
        prepared_log: Vec<PreparedMessage>,
        /// 签名
        signature: Vec<u8>,
    },
    /// 新视图通知（New Leader）
    NewView {
        /// 新视图号
        new_view: u64,
        /// 新 Leader 节点 ID
        new_leader: String,
        /// 视图切换消息集合（2f+1 个）
        view_changes: Vec<ViewChangeData>,
        /// 预准备消息集合
        pre_prepares: Vec<PrePrepareData>,
        /// 签名
        signature: Vec<u8>,
    },
}

/// 已准备消息数据（用于视图切换）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedMessage {
    /// 视图号
    pub view: u64,
    /// 序列号
    pub sequence: u64,
    /// 请求摘要
    pub digest: String,
    /// 准备证书（2f+1 个 Prepare 签名）
    pub prepare_certificate: Vec<SignatureData>,
}

/// 视图切换数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewChangeData {
    /// 新视图号
    pub new_view: u64,
    /// 发送节点 ID
    pub sender_id: String,
    /// 最后稳定 checkpoint
    pub last_stable_checkpoint: u64,
    /// checkpoint 摘要
    pub checkpoint_digest: String,
    /// 签名
    pub signature: Vec<u8>,
}

/// 预准备数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrePrepareData {
    /// 视图号
    pub view: u64,
    /// 序列号
    pub sequence: u64,
    /// 请求摘要
    pub digest: String,
    /// 请求数据
    pub request: Vec<u8>,
}

/// 签名数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureData {
    /// 签名者 ID
    pub signer_id: String,
    /// 签名
    pub signature: Vec<u8>,
    /// 公钥（可选，用于验证）
    pub public_key: Option<Vec<u8>>,
}

/// 带签名的消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedMessage {
    /// 消息内容
    pub message: PBFTMessage,
    /// 发送者 ID
    pub sender_id: String,
    /// 签名
    pub signature: Vec<u8>,
    /// 时间戳
    pub timestamp: u64,
}

impl SignedMessage {
    /// 创建签名消息
    pub fn new(message: PBFTMessage, sender_id: String, signing_key: &SigningKey) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let message_bytes = Self::serialize_message(&message);
        let signature = signing_key.try_sign(&message_bytes).unwrap();

        SignedMessage {
            message,
            sender_id,
            signature: signature.to_bytes().to_vec(),
            timestamp,
        }
    }

    /// 验证签名
    pub fn verify(&self, public_key: &VerifyingKey) -> bool {
        let message_bytes = Self::serialize_message(&self.message);
        
        match Signature::try_from(&self.signature[..]) {
            Ok(signature) => public_key.verify(&message_bytes, &signature).is_ok(),
            Err(_) => false,
        }
    }

    /// 获取消息类型
    pub fn message_type(&self) -> MessageType {
        match &self.message {
            PBFTMessage::Request { .. } => MessageType::Request,
            PBFTMessage::PrePrepare { .. } => MessageType::PrePrepare,
            PBFTMessage::Prepare { .. } => MessageType::Prepare,
            PBFTMessage::Commit { .. } => MessageType::Commit,
            PBFTMessage::Reply { .. } => MessageType::Reply,
            PBFTMessage::ViewChange { .. } => MessageType::ViewChange,
            PBFTMessage::NewView { .. } => MessageType::NewView,
        }
    }

    /// 序列化消息用于签名
    fn serialize_message(message: &PBFTMessage) -> Vec<u8> {
        match message {
            PBFTMessage::Request { client_id, operation, timestamp } => {
                format!("Request:{}:{}:{}", client_id, hex::encode(operation), timestamp).into_bytes()
            }
            PBFTMessage::PrePrepare { view, sequence, digest, leader, request } => {
                format!("PrePrepare:{}:{}:{}:{}:{}", view, sequence, digest, leader, hex::encode(request)).into_bytes()
            }
            PBFTMessage::Prepare { view, sequence, digest, replica_id, .. } => {
                format!("Prepare:{}:{}:{}:{}", view, sequence, digest, replica_id).into_bytes()
            }
            PBFTMessage::Commit { view, sequence, digest, replica_id, .. } => {
                format!("Commit:{}:{}:{}:{}", view, sequence, digest, replica_id).into_bytes()
            }
            PBFTMessage::Reply { view, timestamp, client_id, replica_id, result, .. } => {
                format!("Reply:{}:{}:{}:{}:{}", view, timestamp, client_id, replica_id, hex::encode(result)).into_bytes()
            }
            PBFTMessage::ViewChange { new_view, sender_id, last_stable_checkpoint, checkpoint_digest, .. } => {
                format!("ViewChange:{}:{}:{}:{}", new_view, sender_id, last_stable_checkpoint, checkpoint_digest).into_bytes()
            }
            PBFTMessage::NewView { new_view, new_leader, .. } => {
                format!("NewView:{}:{}", new_view, new_leader).into_bytes()
            }
        }
    }

    /// 计算消息摘要
    pub fn digest(&self) -> String {
        let bytes = Self::serialize_message(&self.message);
        let hash = Sha256::digest(&bytes);
        format!("{:x}", hash)
    }
}

/// 操作类型 - 定义可执行的操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    /// KV 写入操作
    KvWrite {
        key: String,
        value: Vec<u8>,
    },
    /// KV 读取操作
    KvRead {
        key: String,
    },
    /// 推理结果提交
    InferenceCommit {
        node_id: String,
        output: String,
        kv_hash: String,
    },
    /// 信誉更新
    ReputationUpdate {
        node_id: String,
        event: String,
        delta: f64,
    },
    // ========================================================================
    // P2-1：质量感知共识扩展操作
    // ========================================================================
    
    /// 质量验证结果提交
    QualitySubmission {
        /// 请求 ID
        request_id: String,
        /// 提交节点 ID
        submitter_id: String,
        /// 质量分数（0.0 - 1.0）
        quality_score: f64,
        /// 验证证据哈希
        evidence_hash: String,
        /// 验证器签名
        validator_signature: String,
    },
    
    /// 恶意行为举报
    MisbehaviorReport {
        /// 举报 ID
        report_id: String,
        /// 举报者 ID
        reporter_id: String,
        /// 被举报节点 ID
        accused_node_id: String,
        /// 恶意行为类型
        misbehavior_type: MisbehaviorType,
        /// 证据列表
        evidence: Vec<String>,
        /// 举报者签名
        reporter_signature: String,
    },
    
    /// 节点状态变更
    NodeStatusChange {
        /// 节点 ID
        node_id: String,
        /// 新状态
        new_status: String,
        /// 变更原因
        reason: String,
        /// 管理员签名
        admin_signature: String,
    },
    
    /// 惩罚执行记录
    PenaltyExecution {
        /// 惩罚 ID
        penalty_id: String,
        /// 被惩罚节点 ID
        node_id: String,
        /// 惩罚类型
        penalty_type: String,
        /// 惩罚时长（秒）
        duration_secs: Option<u64>,
        /// 执行者 ID
        executor_id: String,
    },
}

/// 恶意行为类型
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum MisbehaviorType {
    /// 计算结果造假
    ComputationFraud,
    /// KV Cache 篡改
    KvCacheTampering,
    /// 合谋作弊
    Collusion,
    /// 恶意拖延
    MaliciousDelay,
    /// 资源滥用
    ResourceAbuse,
    /// 其他恶意行为
    Other,
}

impl MisbehaviorType {
    /// 获取严重程度（1-5）
    pub fn severity(&self) -> u32 {
        match self {
            MisbehaviorType::ComputationFraud => 5,
            MisbehaviorType::KvCacheTampering => 5,
            MisbehaviorType::Collusion => 4,
            MisbehaviorType::MaliciousDelay => 3,
            MisbehaviorType::ResourceAbuse => 3,
            MisbehaviorType::Other => 2,
        }
    }
    
    /// 是否可自动恢复
    pub fn is_recoverable(&self) -> bool {
        matches!(self, MisbehaviorType::MaliciousDelay | MisbehaviorType::ResourceAbuse | MisbehaviorType::Other)
    }
}

impl Operation {
    /// 序列化为字节
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("Failed to serialize operation")
    }

    /// 从字节反序列化
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(bytes).map_err(|e| format!("Failed to deserialize operation: {}", e))
    }

    /// 计算操作摘要
    pub fn digest(&self) -> String {
        let bytes = self.to_bytes();
        let hash = Sha256::digest(&bytes);
        format!("{:x}", hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    #[test]
    fn test_signed_message_creation() {
        let key_bytes: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&key_bytes);

        let message = PBFTMessage::Request {
            client_id: "client_1".to_string(),
            operation: b"test_operation".to_vec(),
            timestamp: 1234567890,
        };

        let signed = SignedMessage::new(message.clone(), "node_1".to_string(), &signing_key);

        assert_eq!(signed.sender_id, "node_1");
        assert!(!signed.signature.is_empty());
    }

    #[test]
    fn test_signed_message_verification() {
        let key_bytes: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&key_bytes);
        let verifying_key = signing_key.verifying_key();

        let message = PBFTMessage::Prepare {
            view: 0,
            sequence: 1,
            digest: "abc123".to_string(),
            replica_id: "node_1".to_string(),
            signature: vec![],
        };

        let signed = SignedMessage::new(message, "node_1".to_string(), &signing_key);

        // 正确公钥验证通过
        assert!(signed.verify(&verifying_key));

        // 错误公钥验证失败
        let wrong_key_bytes: [u8; 32] = rand::random();
        let wrong_key = SigningKey::from_bytes(&wrong_key_bytes);
        assert!(!signed.verify(&wrong_key.verifying_key()));
    }

    #[test]
    fn test_operation_serialization() {
        let op = Operation::KvWrite {
            key: "test_key".to_string(),
            value: b"test_value".to_vec(),
        };

        let bytes = op.to_bytes();
        let restored = Operation::from_bytes(&bytes).unwrap();

        match restored {
            Operation::KvWrite { key, value } => {
                assert_eq!(key, "test_key");
                assert_eq!(value, b"test_value");
            }
            _ => panic!("Wrong operation type"),
        }
    }

    #[test]
    fn test_message_digest() {
        let key_bytes: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&key_bytes);

        let message = PBFTMessage::Request {
            client_id: "client_1".to_string(),
            operation: b"test".to_vec(),
            timestamp: 1000,
        };

        let signed = SignedMessage::new(message.clone(), "node_1".to_string(), &signing_key);
        let digest = signed.digest();

        assert_eq!(digest.len(), 64); // SHA256 hex length

        // 相同消息产生相同摘要
        let signed2 = SignedMessage::new(message, "node_1".to_string(), &signing_key);
        assert_eq!(signed.digest(), signed2.digest());
    }
}
