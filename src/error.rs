//! 错误模块 - 简化错误处理
//!
//! **重构说明** (P11 锐评修复):
//! - 库层 (block, blockchain, transaction): 保留 thiserror 用于结构化错误
//! - 应用层 (coordinator, services): 使用 anyhow::Result，避免 .map_err(|e| format!(...)) 模式
//!
//! 设计原则：
//! - 库层：需要精确错误分类和 pattern match 的场景使用 thiserror
//! - 应用层：直接透传错误上下文，使用 anyhow::Result 简化签名

use thiserror::Error;

// ==================== 区块链层错误（库层） ====================
// 保留用于库边界的精确错误类型

/// 区块链核心错误类型
#[derive(Error, Debug, Clone, PartialEq)]
pub enum BlockchainError {
    /// 交易错误
    #[error("交易错误：{0}")]
    Transaction(#[from] TransactionError),

    /// 区块错误
    #[error("区块错误：{0}")]
    Block(#[from] BlockError),

    /// 链验证错误
    #[error("链验证错误：{0}")]
    ChainValidation(String),

    /// 节点错误
    #[error("节点错误：{0}")]
    Node(#[from] NodeError),

    /// 序列化错误
    #[error("序列化错误：{0}")]
    Serialization(String),

    /// KV 存证错误
    #[error("KV 存证错误：{0}")]
    KvProof(String),

    /// 通用错误
    #[error("{0}")]
    General(String),
}

/// 交易错误
#[derive(Error, Debug, Clone, PartialEq)]
pub enum TransactionError {
    /// 交易验证失败
    #[error("交易验证失败：{0}")]
    ValidationFailed(String),

    /// 交易格式错误
    #[error("交易格式错误：{0}")]
    Malformed(String),

    /// 区块已密封，无法修改
    #[error("区块已密封，无法修改")]
    BlockSealed,
}

/// 区块错误
#[derive(Error, Debug, Clone, PartialEq)]
pub enum BlockError {
    /// 区块验证失败
    #[error("区块验证失败：{0}")]
    ValidationFailed(String),

    /// 区块哈希不匹配
    #[error("区块哈希不匹配：expected={expected}, got={got}")]
    HashMismatch { expected: String, got: String },

    /// 前驱哈希不匹配
    #[error("前驱哈希不匹配：index={index}, expected={expected}, got={got}")]
    PreviousHashMismatch { index: u64, expected: String, got: String },

    /// 创世区块错误
    #[error("创世区块错误：{0}")]
    GenesisError(String),

    /// 区块已密封，无法修改
    #[error("区块已密封，无法修改")]
    BlockSealed,
}

// ==================== 节点层错误（库层） ====================

// ==================== 节点层错误（库层） ====================

/// 节点层错误
#[derive(Error, Debug, Clone, PartialEq)]
pub enum NodeError {
    /// 节点未找到
    #[error("节点未找到：{0}")]
    NotFound(String),

    /// 节点已存在
    #[error("节点已存在：{0}")]
    AlreadyExists(String),

    /// 凭证错误
    #[error("凭证错误：{0}")]
    Credential(String),

    /// 提供商错误
    #[error("提供商错误：{0}")]
    Provider(String),

    /// 调度错误
    #[error("调度错误：{0}")]
    Scheduling(String),
}

/// 访问凭证错误
#[derive(Error, Debug, Clone, PartialEq)]
pub enum CredentialError {
    /// 凭证已过期
    #[error("凭证已过期：expires_at={expires_at}, current={current}")]
    Expired { expires_at: u64, current: u64 },

    /// 凭证未找到
    #[error("凭证未找到：{0}")]
    NotFound(String),

    /// 权限不足
    #[error("权限不足：required={required}, current={current}")]
    InsufficientPermission { required: String, current: String },
}

/// 提供商管理错误
#[derive(Error, Debug, Clone, PartialEq)]
pub enum ProviderError {
    /// 提供商未找到
    #[error("提供商未找到：{0}")]
    NotFound(String),

    /// 提供商已存在
    #[error("提供商已存在：{0}")]
    AlreadyExists(String),

    /// 提供商不可用
    #[error("提供商不可用：{0}")]
    Unavailable(String),
}

// ==================== 记忆层错误（库层） ====================

/// 记忆层错误
#[derive(Error, Debug, Clone, PartialEq)]
pub enum MemoryLayerError {
    /// KV 未找到
    #[error("KV 未找到：key={key}, shard={shard}")]
    KvNotFound { key: String, shard: String },
    
    /// KV 已存在
    #[error("KV 已存在：key={key}")]
    KvAlreadyExists { key: String },
    
    /// 哈希校验失败
    #[error("哈希校验失败：expected={expected}, got={got}")]
    HashVerificationFailed { expected: String, got: String },
    
    /// 版本冲突
    #[error("版本冲突：key={key}, expected_version={expected_version}, current_version={current_version}")]
    VersionConflict { key: String, expected_version: u64, current_version: u64 },
    
    /// 区块错误
    #[error("记忆区块错误：{0}")]
    BlockError(String),
    
    /// 授权错误
    #[error("授权错误：{0}")]
    Authorization(String),
    
    /// 存储错误
    #[error("存储错误：{0}")]
    Storage(String),
    
    /// 副本同步错误
    #[error("副本同步错误：{0}")]
    ReplicaSync(String),
}

// ==================== 推理提供商层错误 ====================

/// 推理提供商层错误
#[derive(Error, Debug, Clone, PartialEq)]
pub enum ProviderLayerError {
    /// 提供商未找到
    #[error("推理提供商未找到：{0}")]
    ProviderNotFound(String),
    
    /// 提供商已存在
    #[error("推理提供商已存在：{0}")]
    ProviderAlreadyExists(String),
    
    /// 推理执行失败
    #[error("推理执行失败：{0}")]
    ExecutionFailed(String),
    
    /// 推理超时
    #[error("推理超时：timeout_ms={timeout_ms}, elapsed_ms={elapsed_ms}")]
    Timeout { timeout_ms: u64, elapsed_ms: u64 },
    
    /// KV 读取失败
    #[error("KV 读取失败：{0}")]
    KvReadFailed(String),
    
    /// KV 写入失败
    #[error("KV 写入失败：{0}")]
    KvWriteFailed(String),
    
    /// 模型未找到
    #[error("模型未找到：{0}")]
    ModelNotFound(String),
    
    /// 输出截断
    #[error("输出截断：max_tokens={max_tokens}, actual={actual}")]
    OutputTruncated { max_tokens: u32, actual: u32 },

    /// 记忆层错误
    #[error("记忆层错误：{0}")]
    MemoryLayer(#[from] MemoryLayerError),

    /// 节点层错误
    #[error("节点层错误：{0}")]
    NodeLayer(String),
}

// ==================== 错误转换实现 ====================

impl From<String> for BlockchainError {
    fn from(err: String) -> Self {
        BlockchainError::General(err)
    }
}

impl From<&str> for BlockchainError {
    fn from(err: &str) -> Self {
        BlockchainError::General(err.to_string())
    }
}

impl From<String> for NodeError {
    fn from(err: String) -> Self {
        NodeError::Scheduling(err)
    }
}

impl From<&str> for NodeError {
    fn from(err: &str) -> Self {
        NodeError::Scheduling(err.to_string())
    }
}

impl From<String> for MemoryLayerError {
    fn from(err: String) -> Self {
        MemoryLayerError::Storage(err)
    }
}

impl From<&str> for MemoryLayerError {
    fn from(err: &str) -> Self {
        MemoryLayerError::Storage(err.to_string())
    }
}

impl From<String> for ProviderLayerError {
    fn from(err: String) -> Self {
        ProviderLayerError::ExecutionFailed(err)
    }
}

impl From<&str> for ProviderLayerError {
    fn from(err: &str) -> Self {
        ProviderLayerError::ExecutionFailed(err.to_string())
    }
}

impl From<crate::failover::circuit_breaker::CircuitBreakerError> for ProviderLayerError {
    fn from(err: crate::failover::circuit_breaker::CircuitBreakerError) -> Self {
        ProviderLayerError::ExecutionFailed(format!("{}", err))
    }
}

// ==================== 统一 Result 类型别名 ====================

/// 区块链操作结果（库层）
pub type BlockchainResult<T> = std::result::Result<T, BlockchainError>;

/// 节点层操作结果（库层）
pub type NodeResult<T> = std::result::Result<T, NodeError>;

/// 记忆层操作结果（库层）
pub type MemoryResult<T> = std::result::Result<T, MemoryLayerError>;

/// 推理提供商层操作结果（库层）
pub type ProviderResult<T> = std::result::Result<T, ProviderLayerError>;

/// 通用 Result 类型别名（库层）
pub type Result<T> = std::result::Result<T, BlockchainError>;

// ==================== 应用层错误处理 ====================
// 应用层（coordinator, services）使用 anyhow::Result
// 避免 .map_err(|e| format!(...)) 模式

pub use anyhow::{Context, Result as AnyhowResult};

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blockchain_error_display() {
        let err = BlockchainError::Transaction(TransactionError::ValidationFailed("余额不足".to_string()));
        assert_eq!(format!("{}", err), "交易错误：交易验证失败：余额不足");
    }

    #[test]
    fn test_error_from_string() {
        let err: BlockchainError = "test error".into();
        assert_eq!(format!("{}", err), "test error");
    }

    #[test]
    fn test_transaction_error_from() {
        let tx_err = TransactionError::ValidationFailed("测试错误".to_string());
        let bc_err: BlockchainError = tx_err.into();
        assert!(matches!(bc_err, BlockchainError::Transaction(_)));
    }

    #[test]
    fn test_credential_error_expired() {
        let err = CredentialError::Expired { expires_at: 1000, current: 2000 };
        assert_eq!(format!("{}", err), "凭证已过期：expires_at=1000, current=2000");
    }

    #[test]
    fn test_memory_layer_error() {
        let err = MemoryLayerError::KvNotFound { 
            key: "test_key".to_string(), 
            shard: "shard_1".to_string() 
        };
        assert_eq!(format!("{}", err), "KV 未找到：key=test_key, shard=shard_1");
    }

    #[test]
    fn test_provider_layer_timeout_error() {
        let err = ProviderLayerError::Timeout { timeout_ms: 5000, elapsed_ms: 6000 };
        assert_eq!(format!("{}", err), "推理超时：timeout_ms=5000, elapsed_ms=6000");
    }

    #[test]
    fn test_result_aliases() {
        let result: BlockchainResult<()> = Ok(());
        assert!(result.is_ok());

        let result: NodeResult<()> = Err(NodeError::NotFound("node_1".to_string()));
        assert!(result.is_err());

        let result: MemoryResult<()> = Err(MemoryLayerError::KvNotFound {
            key: "key".to_string(),
            shard: "shard".to_string()
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_error_pattern_match() {
        let err = BlockchainError::Block(BlockError::ValidationFailed("test error".to_string()));

        match err {
            BlockchainError::Block(BlockError::ValidationFailed(msg)) => {
                assert_eq!(msg, "test error");
            }
            _ => panic!("Expected ValidationFailed error"),
        }
    }

    #[test]
    fn test_block_sealed_error() {
        let err = BlockError::BlockSealed;
        assert_eq!(format!("{}", err), "区块已密封，无法修改");
    }
}
