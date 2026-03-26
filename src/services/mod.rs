//! 服务层模块 - 拆分 God Object 后的三个核心服务
//!
//! **重构说明** (P11 锐评修复):
//! - 原 `coordinator.rs` (1843 行) 是典型的"上帝对象"反模式
//! - 拆分为三个单一职责的服务：
//!   - `InferenceOrchestrator` - 协调推理流程
//!   - `CommitmentService` - 处理上链存证
//!   - `FailoverService` - 健康监控和故障切换
//!
//! # 服务职责划分
//!
//! | 服务 | 职责 | 不依赖 |
//! |------|------|--------|
//! | **InferenceOrchestrator** | 选择提供商、执行推理 | 不处理上链、不监控健康 |
//! | **CommitmentService** | KV 存证上链、交易记录上链 | 不执行推理、不监控健康 |
//! | **FailoverService** | 健康监控、故障切换 | 不执行推理、不处理上链 |
//!
//! # 使用示例
//!
//! ```ignore
//! use std::sync::Arc;
//! use block_chain_with_context::services::{
//!     InferenceOrchestrator, CommitmentService, FailoverService,
//! };
//! use block_chain_with_context::blockchain::BlockchainConfig;
//!
//! // 创建各层管理器
//! let node_layer = Arc::new(NodeLayerManager::new("node_1".into(), "addr_1".into()));
//! let memory_layer = Arc::new(MemoryLayerManager::new("node_1"));
//! let provider_layer = Arc::new(ProviderLayerManager::new());
//!
//! // 创建三个服务
//! let orchestrator = InferenceOrchestrator::new(
//!     node_layer.clone(),
//!     memory_layer.clone(),
//!     provider_layer.clone(),
//! );
//!
//! let commitment = CommitmentService::with_config(
//!     "addr_1".into(),
//!     BlockchainConfig::default(),
//! ).unwrap();
//!
//! let failover = FailoverService::new(
//!     provider_layer.clone(),
//!     TimeoutConfig::default(),
//! );
//!
//! // 执行推理流程
//! let request = InferenceRequest::new("req_1".into(), "prompt".into(), "model".into(), 100);
//! let provider_id = orchestrator.select_provider().unwrap();
//! let credential = /* ... */;
//! let response = orchestrator.execute(&request, &credential, &provider_id).unwrap();
//!
//! // 上链存证
//! commitment.commit_inference(metadata, &provider_id, &response, kv_proofs).unwrap();
//! ```

pub mod inference_orchestrator;
pub mod commitment_service;
pub mod failover_service;
pub mod qaas_service;

// 重新导出服务类型
pub use inference_orchestrator::{InferenceOrchestrator, FullOrchestrator};
pub use commitment_service::CommitmentService;
pub use failover_service::FailoverService;
pub use qaas_service::{QaaSService, QaaSConfig};

// 导入 anyhow Context trait 用于错误处理
#[allow(unused_imports)]
use anyhow::Context;

// ==================== 服务 Trait 定义（P11 要求 7：EaaS 架构解耦） ====================

use async_trait::async_trait;
use crate::provider_layer::{InferenceRequest, InferenceResponse};
use crate::quality_assessment::{QualityProof, QualityAssessmentRequest, QualityAssessmentResponse};
use crate::consensus::messages::Operation;
use anyhow::Result;

/// 推理服务 Trait
#[async_trait]
pub trait InferenceService {
    /// 执行推理请求
    async fn execute(&self, request: InferenceRequest) -> Result<InferenceResponse>;

    /// 选择最佳提供商
    fn select_provider(&self) -> Result<String>;
}

/// 质量验证服务 Trait（P11 要求 7：独立验证服务）
#[async_trait]
pub trait QualityVerificationService {
    /// 验证输出质量
    async fn verify_quality(
        &self,
        request: QualityAssessmentRequest,
    ) -> Result<QualityAssessmentResponse>;

    /// 获取验证证明
    async fn get_proof(&self, proof_id: &str) -> Result<QualityProof>;

    /// 验证证明有效性
    async fn verify_proof(&self, proof: &QualityProof) -> Result<ProofValidity>;
}

/// 证明有效性结果
#[derive(Debug, Clone, PartialEq)]
pub struct ProofValidity {
    /// 是否有效
    pub is_valid: bool,
    /// 验证详情
    pub details: Vec<String>,
}

/// 共识服务 Trait（P11 要求 7：独立共识模块）
#[async_trait]
pub trait ConsensusServiceTrait {
    /// 提交共识提案
    async fn propose(&self, operation: Operation) -> Result<ConsensusResult>;

    /// 查询共识状态
    async fn get_consensus_status(&self) -> Result<ConsensusStatus>;
}

/// 共识结果
#[derive(Debug, Clone, PartialEq)]
pub enum ConsensusResult {
    /// 已提交
    Committed,
    /// 已拒绝
    Rejected,
    /// 共识中
    Pending,
}

/// 共识状态
#[derive(Debug, Clone)]
pub struct ConsensusStatus {
    /// 当前视图号
    pub view: u64,
    /// 当前序列号
    pub sequence: u64,
    /// 节点状态
    pub node_status: String,
}

/// 区块链存证服务 Trait（P11 要求 7：独立存证服务）
#[async_trait]
pub trait BlockchainAttestationService {
    /// 提交存证
    async fn submit_attestation(&self, attestation: AttestationData) -> Result<u64>;

    /// 查询存证
    async fn get_attestation(&self, height: u64) -> Result<AttestationData>;
}

/// 存证数据
#[derive(Debug, Clone)]
pub struct AttestationData {
    /// 存证类型
    pub attestation_type: String,
    /// 存证内容
    pub content: Vec<u8>,
    /// 时间戳
    pub timestamp: u64,
}

// ==================== 向后兼容的旧 Trait 定义 ====================

/// 存证服务 Trait（旧版，保持向后兼容）
#[async_trait]
pub trait CommitmentServiceTrait {
    /// 提交 KV 存证和元数据到区块链
    async fn commit(&self, metadata: BlockMetadata, proofs: Vec<KvCacheProof>) -> Result<u64>;

    /// 验证区块链完整性
    fn verify(&self) -> bool;
}

/// 故障切换服务 Trait（旧版，保持向后兼容）
#[async_trait]
pub trait FailoverServiceTrait {
    /// 执行故障切换
    async fn failover(&self, from: &str, to: &str, reason: FailoverReason) -> Result<()>;

    /// 获取健康提供商列表
    fn get_healthy_providers(&self) -> Vec<String>;
}

// 需要导入旧 trait 使用的类型
use crate::block::KvCacheProof;
use crate::metadata::BlockMetadata;
use crate::failover::FailoverReason;
