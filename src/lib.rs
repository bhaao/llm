//! 区块链模块 - 分布式大模型上下文可信存储
//!
//! 本模块实现了区块链与分布式 LLM 的正确结合方式：
//! - 区块链作为"可信增强工具"，而非"计算过程类比"
//! - **核心创新**：KV Cache 链上存证（简单但有效）
//!
//! # 双链架构
//!
//! 本项目采用"双链架构"，两条链各司其职：
//!
//! - **区块链（Blockchain）**：全局可信存证主链，存储 KV 哈希、元数据、信誉记录
//! - **记忆链（MemoryChain）**：分布式 KV 数据链，存储实际上下文数据
//!
//! 区块链仅存证 KV 哈希，不存储实际数据；记忆链存储实际 KV 数据，哈希上链存证。
//! 两条链配合实现"数据本地存储 + 哈希全网共识"的可信架构。
//!
//! # 核心创新：KV Cache 存证
//!
//! 将 KV 块的哈希上链，这个设计简单但有用：
//! - **防篡改**：验证 KV 数据有没有被篡改
//! - **跨节点一致性**：跨节点 KV 一致性校验
//! - **版本追溯**：追溯历史 KV 版本
//!
//! 这不是为了创新而创新，是真的能解决分布式推理中的信任问题。
//!
//! # 架构逻辑
//!
//! ## 传统四层架构
//!
//! - **推理负责算得对**：分布式推理模块负责高效计算
//! - **评估器负责验得准**：质量评估器负责验证结果
//! - **多节点负责保安全**：并行计算 + 结果比对
//! - **区块链负责记可信**：不可篡改的存证记录
//!
//! ## 企业级三层解耦架构（区块链原生版）
//!
//! 基于联盟链设计原则，实现节点（身份/可信）、记忆（分布式上下文/KV）、推理（计算执行）三层彻底解耦：
//!
//! | 层级 | 核心定位 | 核心职责 | 关键约束 |
//! |------|----------|----------|----------|
//! | **区块链节点层** | 可信管控中枢 | 1. 节点身份/公钥/信誉管理<br>2. 推理提供商准入/调度/切换/惩罚<br>3. 记忆层哈希校验/存证上链<br>4. 跨节点共识/仲裁 | 1. 无状态（不存储任何上下文/KV）<br>2. 只做轻量逻辑（<5ms/次）<br>3. **支持异步上链**（不阻塞主流程） |
//! | **分布式记忆层** | 区块链化上下文存储核心 | 1. 以"区块"为单位存储 KV/上下文分片<br>2. 哈希链式串联（防篡改）<br>3. 分布式多副本存储（容灾）<br>4. 版本控制/访问授权 | 1. 仅对接节点层做哈希校验<br>2. 仅向推理提供商开放只读/写权限<br>3. 热点数据本地化缓存 |
//! | **推理服务提供商层** | 无状态计算执行单元 | 1. 从记忆层读取 KV/上下文<br>2. 执行 LLM 推理<br>3. 向记忆层写入新生成的 KV<br>4. 向节点层上报推理指标 | 1. 无区块链能力（仅认节点授权）<br>2. 无记忆存储能力（仅临时加载）<br>3. 标准化接口（适配多引擎） |
//!
//! ### 依赖关系（单向依赖，杜绝递归/闭环）
//!
//! ```text
//! 推理提供商 → 依赖 → 记忆层（读取/写入 KV）
//! 推理提供商 → 依赖 → 节点层（获取访问授权/上报指标）
//! 记忆层 → 依赖 → 节点层（哈希校验/存证上链）
//! 节点层 → 不依赖 → 推理提供商/记忆层（仅做管控，不做执行）
//! ```
//!
//! # 使用示例
//!
//! ## 传统区块链 API
//!
//! ```
//! use block_chain_with_context::{Blockchain, Transaction, TransactionType, TransactionPayload};
//! use block_chain_with_context::{BlockMetadata, KvCacheProof};
//!
//! // 创建区块链
//! let mut blockchain = Blockchain::new("user_address".to_string());
//!
//! // 注册推理节点
//! blockchain.register_node("node_1".to_string());
//!
//! // 添加推理请求
//! let tx = Transaction::new(
//!     "user".to_string(),
//!     "assistant".to_string(),
//!     TransactionType::Transfer,
//!     TransactionPayload::None,
//! );
//! blockchain.add_pending_transaction(tx);
//!
//! // 添加 KV Cache 存证（创新 A）
//! let kv_proof = KvCacheProof::new(
//!     "kv_001".to_string(),
//!     "kv_hash".to_string(),
//!     "node_1".to_string(),
//!     1024,
//! );
//! blockchain.add_kv_proof(kv_proof);
//!
//! // 提交推理记录到链上
//! let metadata = BlockMetadata::default();
//! blockchain.commit_inference(metadata, "node_1".to_string());
//! ```
//!
//! ## 三层解耦架构 API
//!
//! ```ignore
//! use block_chain_with_context::coordinator::ArchitectureCoordinator;
//! use block_chain_with_context::provider_layer::InferenceEngineType;
//! use block_chain_with_context::provider_layer::InferenceRequest;
//!
//! // 创建架构协调器
//! let mut coordinator = ArchitectureCoordinator::new("node_1".to_string());
//!
//! // 注册推理提供商
//! coordinator.register_provider(
//!     "provider_1".to_string(),
//!     InferenceEngineType::Vllm,
//!     100, // 100 token/s
//! ).unwrap();
//!
//! // 执行完整推理流程（自动处理授权、记忆读写、上链存证）
//! let request = InferenceRequest::new(
//!     "req_1".to_string(),
//!     "Hello, AI!".to_string(),
//!     "llama-7b".to_string(),
//!     100,
//! );
//! let response = coordinator.execute_inference(request).unwrap();
//!
//! // 验证链完整性
//! assert!(coordinator.verify_memory_chain());
//! assert!(coordinator.verify_blockchain());
//! ```

pub mod traits;
pub mod transaction;
pub mod metadata;
pub mod block;
pub mod blockchain;
pub mod error;
pub mod quality_assessment;
pub mod reputation;
pub mod utils;
pub mod storage;

// 三层解耦架构模块
pub mod node_layer;
pub mod memory_layer;
pub mod provider_layer;
pub mod coordinator;
pub mod failover;

// 重新导出常用类型
pub use traits::{Hashable, Serializable, Verifiable, Attestable};
pub use traits::{AttestationMetadata, AttestationType};
pub use traits::{SignatureVerifier, NullVerifier, SimpleVerifier, Ed25519Verifier};
pub use transaction::{Transaction, TransactionType, TransactionPayload, TransactionStatus};
pub use metadata::BlockMetadata;
pub use block::{Block, KvCacheProof};
pub use blockchain::{Blockchain, SafeBlockchain};
pub use error::{
    BlockchainError, BlockchainResult,
    NodeError, NodeResult,
    MemoryLayerError, MemoryResult,
    ProviderLayerError, ProviderResult,
    CoordinatorError, CoordinatorResult,
    TransactionError, BlockError,
    CredentialError, ProviderError,
};
pub use error::Result;
pub use blockchain::{ConsensusEngine, ConsensusDecision};
pub use blockchain::{
    BlockchainConfig, LogConfig, TimeoutConfig, RetryConfig,
    ConnectionPoolConfig, ConsensusConfig,
};

// 重新导出质量评估模块类型
pub use quality_assessment::{QualityAssessor, QualityAssessment, AssessmentDetails};
pub use quality_assessment::{SemanticCheckResult, IntegrityCheckResult};
pub use quality_assessment::{NullAssessor, SimpleAssessor, MultiNodeComparator, SemanticCheckMode};

// 重新导出信誉模块类型
pub use reputation::{NodeReputation, NodeStatus, ReputationManager};
pub use reputation::{ReputationRecord, ReputationEvent};

// 重新导出存储模块类型
pub use storage::{JsonStorage, BlockchainData, BlockchainConfigData};

// 重新导出三层架构模块类型
pub use node_layer::{
    NodeLayerManager, NodeIdentity, NodeRole,
    ProviderRecord, ProviderStatus, SchedulingStrategy,
    AccessCredential, AccessType,
};

pub use memory_layer::{
    MemoryLayerManager, MemoryBlock, MemoryBlockHeader, KvShard,
    KvProof,
};

#[cfg(feature = "tiered-storage")]
pub use memory_layer::tiered_storage::{
    TieredStorageManager, TieredStorageConfig,
    KvData, AccessStats, StorageTier,
    serialize_kv, deserialize_kv,
};

#[cfg(feature = "tiered-storage")]
pub use memory_layer::kv_compression::{
    KvCompressor, CompressionAlgorithm, CompressedKv, QuantizedKv,
    calculate_mse, calculate_max_absolute_error,
};

pub use provider_layer::{
    ProviderLayerManager, InferenceProvider, InferenceRequest, InferenceResponse,
    InferenceEngineType, MockInferenceProvider,
};

#[cfg(feature = "http")]
pub use provider_layer::http_client::{InferenceHttpClient, GenerateRequest, GenerateResponse};

pub use coordinator::{
    ArchitectureCoordinator, ArchitectureConfig,
    NodeLayerConfig, MemoryLayerConfig,
    InferenceContext, InferenceStats,
    AsyncCommitResult,
};

// 重新导出故障转移模块类型
pub use failover::{
    ProviderHealthMonitor, ProviderHealthStatus,
    ProviderHealthRecord, TimeoutError, FailoverEvent, FailoverReason,
};
