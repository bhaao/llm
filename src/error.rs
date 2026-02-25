//! 错误模块 - 使用 thiserror 定义结构化错误类型
//!
//! 提供生产级的错误处理机制：
//! - 统一的错误类型枚举
//! - 支持错误链（source）
//! - 自动实现 Display/Error trait
//! - 支持错误分类和 pattern match

use thiserror::Error;

// ==================== 区块链层错误 ====================

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
    
    /// 签名验证错误
    #[error("签名验证错误：{0}")]
    Signature(String),
    
    /// KV 存证错误
    #[error("KV 存证错误：{0}")]
    KvProof(String),
    
    /// 质量评估错误
    #[error("质量评估错误：{0}")]
    QualityAssessment(String),
    
    /// 信誉管理错误
    #[error("信誉管理错误：{0}")]
    Reputation(String),
    
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
    
    /// 交易签名无效
    #[error("交易签名无效：{0}")]
    InvalidSignature(String),
    
    /// 交易格式错误
    #[error("交易格式错误：{0}")]
    Malformed(String),
    
    /// 余额不足
    #[error("余额不足：{0}")]
    InsufficientBalance(String),
    
    /// Gas 不足
    #[error("Gas 不足：{0}")]
    InsufficientGas(String),
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
    
    /// 区块超过 Gas 限制
    #[error("区块超过 Gas 限制：current={current}, max={max}")]
    GasLimitExceeded { current: u64, max: u64 },
    
    /// 区块超过交易数限制
    #[error("区块超过交易数限制：current={current}, max={max}")]
    TransactionLimitExceeded { current: usize, max: usize },
    
    /// 创世区块错误
    #[error("创世区块错误：{0}")]
    GenesisError(String),
    
    /// 区块已密封，无法修改
    #[error("区块已密封，无法修改")]
    BlockSealed,
}

// ==================== 节点层错误 ====================

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
    Credential(#[from] CredentialError),
    
    /// 提供商错误
    #[error("提供商错误：{0}")]
    Provider(#[from] ProviderError),
    
    /// 调度错误
    #[error("调度错误：{0}")]
    Scheduling(String),
    
    /// 授权失败
    #[error("授权失败：{0}")]
    AuthorizationFailed(String),
}

/// 访问凭证错误
#[derive(Error, Debug, Clone, PartialEq)]
pub enum CredentialError {
    /// 凭证已过期
    #[error("凭证已过期：expires_at={expires_at}, current={current}")]
    Expired { expires_at: u64, current: u64 },
    
    /// 凭证已撤销
    #[error("凭证已撤销：{0}")]
    Revoked(String),
    
    /// 凭证签名无效
    #[error("凭证签名无效：{0}")]
    InvalidSignature(String),
    
    /// 凭证未找到
    #[error("凭证未找到：{0}")]
    NotFound(String),
    
    /// 凭证已存在
    #[error("凭证已存在：{0}")]
    AlreadyExists(String),
    
    /// 权限不足
    #[error("权限不足：required={required}, current={current}")]
    InsufficientPermission { required: String, current: String },
    
    /// 重复撤销（幂等处理）
    #[error("凭证已被撤销或不存在：{0}")]
    AlreadyRevoked(String),
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
    
    /// 提供商状态无效
    #[error("提供商状态无效：{0}")]
    InvalidStatus(String),
    
    /// 提供商不可用
    #[error("提供商不可用：{0}")]
    Unavailable(String),
    
    /// 提供商已暂停
    #[error("提供商已暂停：{0}")]
    Suspended(String),
    
    /// 提供商已拉黑
    #[error("提供商已拉黑：{0}")]
    Blacklisted(String),
}

// ==================== 记忆层错误 ====================

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

// ==================== 协调器层错误 ====================

/// 架构协调器错误
#[derive(Error, Debug, Clone, PartialEq)]
pub enum CoordinatorError {
    /// 节点层错误
    #[error("节点层错误：{0}")]
    NodeLayer(String),
    
    /// 记忆层错误
    #[error("记忆层错误：{0}")]
    MemoryLayer(String),
    
    /// 提供商层错误
    #[error("提供商层错误：{0}")]
    ProviderLayer(String),
    
    /// 区块链错误
    #[error("区块链错误：{0}")]
    Blockchain(String),
    
    /// 无可用提供商
    #[error("无可用推理提供商")]
    NoAvailableProvider,
    
    /// 提供商选择失败
    #[error("提供商选择失败：{0}")]
    ProviderSelectionFailed(String),
    
    /// 上链失败
    #[error("上链失败：{0}")]
    CommitFailed(String),
    
    /// 异步任务失败
    #[error("异步任务失败：{0}")]
    AsyncTaskFailed(String),
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

impl From<String> for CoordinatorError {
    fn from(err: String) -> Self {
        CoordinatorError::ProviderLayer(err)
    }
}

impl From<&str> for CoordinatorError {
    fn from(err: &str) -> Self {
        CoordinatorError::ProviderLayer(err.to_string())
    }
}

// ==================== 统一 Result 类型别名 ====================

/// 区块链操作结果
pub type BlockchainResult<T> = std::result::Result<T, BlockchainError>;

/// 节点层操作结果
pub type NodeResult<T> = std::result::Result<T, NodeError>;

/// 记忆层操作结果
pub type MemoryResult<T> = std::result::Result<T, MemoryLayerError>;

/// 推理提供商层操作结果
pub type ProviderResult<T> = std::result::Result<T, ProviderLayerError>;

/// 协调器操作结果
pub type CoordinatorResult<T> = std::result::Result<T, CoordinatorError>;

/// 通用 Result 类型别名（向后兼容）
pub type Result<T> = std::result::Result<T, BlockchainError>;

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
    fn test_coordinator_error() {
        let err = CoordinatorError::NoAvailableProvider;
        assert_eq!(format!("{}", err), "无可用推理提供商");
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
        let err = BlockchainError::Block(BlockError::GasLimitExceeded { current: 150, max: 100 });
        
        match err {
            BlockchainError::Block(BlockError::GasLimitExceeded { current, max }) => {
                assert_eq!(current, 150);
                assert_eq!(max, 100);
            }
            _ => panic!("Expected GasLimitExceeded error"),
        }
    }

    #[test]
    fn test_block_sealed_error() {
        let err = BlockError::BlockSealed;
        assert_eq!(format!("{}", err), "区块已密封，无法修改");
    }

    #[test]
    fn test_credential_already_revoked() {
        let err = CredentialError::AlreadyRevoked("cred_123".to_string());
        assert_eq!(format!("{}", err), "凭证已被撤销或不存在：cred_123");
    }
}
