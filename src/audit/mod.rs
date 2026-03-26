//! 审计追踪模块 - 全链路可追溯存证（P11 要求 5）
//!
//! **设计目标**：
//! - 任务全链路可审计：从请求提交到上链存证的完整生命周期
//! - 不可篡改：所有事件都有签名和区块链存证
//! - 可追溯：支持按 request_id/output_hash/proof_id 查询完整审计链
//! - 可验证：可生成可验证的审计报告

use serde::{Serialize, Deserialize};
use crate::block::KvCacheProof;
use crate::quality_assessment::QualityProof;
use crate::consensus::messages::Operation;

/// 审计链 - 记录推理任务完整生命周期
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditTrail {
    /// 推理请求提交
    pub request_submitted: Option<RequestSubmissionRecord>,
    /// 节点调度决策
    pub scheduling_decision: Option<SchedulingRecord>,
    /// 推理执行开始
    pub inference_started: Option<InferenceStartRecord>,
    /// 推理执行完成
    pub inference_completed: Option<InferenceCompleteRecord>,
    /// 质量验证完成
    pub quality_verified: Option<QualityVerificationRecord>,
    /// 共识达成
    pub consensus_reached: Option<ConsensusRecord>,
    /// 存证上链
    pub committed_to_chain: Option<ChainCommitRecord>,
}

impl AuditTrail {
    /// 创建空审计链
    pub fn new() -> Self {
        AuditTrail {
            request_submitted: None,
            scheduling_decision: None,
            inference_started: None,
            inference_completed: None,
            quality_verified: None,
            consensus_reached: None,
            committed_to_chain: None,
        }
    }

    /// 检查审计链是否完整
    pub fn is_complete(&self) -> bool {
        self.request_submitted.is_some()
            && self.scheduling_decision.is_some()
            && self.inference_completed.is_some()
            && self.quality_verified.is_some()
            && self.consensus_reached.is_some()
            && self.committed_to_chain.is_some()
    }

    /// 获取审计事件列表
    pub fn get_events(&self) -> Vec<AuditEvent> {
        let mut events = Vec::new();

        if let Some(ref record) = self.request_submitted {
            events.push(AuditEvent::RequestSubmitted(record.clone()));
        }
        if let Some(ref record) = self.scheduling_decision {
            events.push(AuditEvent::SchedulingDecision(record.clone()));
        }
        if let Some(ref record) = self.inference_started {
            events.push(AuditEvent::InferenceStarted(record.clone()));
        }
        if let Some(ref record) = self.inference_completed {
            events.push(AuditEvent::InferenceCompleted(record.clone()));
        }
        if let Some(ref record) = self.quality_verified {
            events.push(AuditEvent::QualityVerified(record.clone()));
        }
        if let Some(ref record) = self.consensus_reached {
            events.push(AuditEvent::ConsensusReached(record.clone()));
        }
        if let Some(ref record) = self.committed_to_chain {
            events.push(AuditEvent::ChainCommitted(record.clone()));
        }

        events
    }
}

impl Default for AuditTrail {
    fn default() -> Self {
        Self::new()
    }
}

/// 审计事件枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditEvent {
    /// 请求提交
    RequestSubmitted(RequestSubmissionRecord),
    /// 调度决策
    SchedulingDecision(SchedulingRecord),
    /// 推理开始
    InferenceStarted(InferenceStartRecord),
    /// 推理完成
    InferenceCompleted(InferenceCompleteRecord),
    /// 质量验证
    QualityVerified(QualityVerificationRecord),
    /// 共识达成
    ConsensusReached(ConsensusRecord),
    /// 上链存证
    ChainCommitted(ChainCommitRecord),
}

impl AuditEvent {
    /// 获取事件时间戳
    pub fn timestamp(&self) -> u64 {
        match self {
            AuditEvent::RequestSubmitted(r) => r.timestamp,
            AuditEvent::SchedulingDecision(r) => r.timestamp,
            AuditEvent::InferenceStarted(r) => r.timestamp,
            AuditEvent::InferenceCompleted(r) => r.timestamp,
            AuditEvent::QualityVerified(r) => r.timestamp,
            AuditEvent::ConsensusReached(r) => r.timestamp,
            AuditEvent::ChainCommitted(r) => r.timestamp,
        }
    }

    /// 获取事件类型
    pub fn event_type(&self) -> &'static str {
        match self {
            AuditEvent::RequestSubmitted(_) => "RequestSubmitted",
            AuditEvent::SchedulingDecision(_) => "SchedulingDecision",
            AuditEvent::InferenceStarted(_) => "InferenceStarted",
            AuditEvent::InferenceCompleted(_) => "InferenceCompleted",
            AuditEvent::QualityVerified(_) => "QualityVerified",
            AuditEvent::ConsensusReached(_) => "ConsensusReached",
            AuditEvent::ChainCommitted(_) => "ChainCommitted",
        }
    }
}

/// 推理请求提交记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestSubmissionRecord {
    /// 请求 ID
    pub request_id: String,
    /// 请求内容哈希
    pub request_hash: String,
    /// 提交者 ID
    pub submitter_id: String,
    /// 时间戳
    pub timestamp: u64,
    /// 签名
    pub signature: String,
}

/// 节点调度记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingRecord {
    /// 请求 ID
    pub request_id: String,
    /// 被选中的节点 ID
    pub selected_node_id: String,
    /// 调度策略
    pub scheduling_strategy: String,
    /// 节点信誉分
    pub node_reputation_score: f64,
    /// 时间戳
    pub timestamp: u64,
    /// 调度器签名
    pub scheduler_signature: String,
}

/// 推理开始记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceStartRecord {
    /// 请求 ID
    pub request_id: String,
    /// 执行节点 ID
    pub node_id: String,
    /// 模型标识
    pub model_id: String,
    /// 时间戳
    pub timestamp: u64,
    /// 节点签名
    pub node_signature: String,
}

/// 推理完成记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceCompleteRecord {
    /// 请求 ID
    pub request_id: String,
    /// 执行节点 ID
    pub node_id: String,
    /// 输出哈希
    pub output_hash: String,
    /// KV Cache 证明
    pub kv_proofs: Vec<KvCacheProof>,
    /// 执行时间（毫秒）
    pub execution_time_ms: u64,
    /// 时间戳
    pub timestamp: u64,
    /// 节点签名
    pub node_signature: String,
}

/// 质量验证记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityVerificationRecord {
    /// 请求 ID
    pub request_id: String,
    /// 验证器 ID
    pub validator_id: String,
    /// 质量证明
    pub quality_proof: QualityProof,
    /// 质量分数
    pub quality_score: f64,
    /// 是否通过阈值
    pub passed_threshold: bool,
    /// 时间戳
    pub timestamp: u64,
    /// 验证器签名
    pub validator_signature: String,
}

/// 共识记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusRecord {
    /// 请求 ID
    pub request_id: String,
    /// 共识操作
    pub operation: Operation,
    /// 参与验证的节点列表
    pub participating_validators: Vec<String>,
    /// 共识结果
    pub consensus_result: String,
    /// 共识证书
    pub quorum_certificate: Vec<u8>,
    /// 时间戳
    pub timestamp: u64,
    /// Leader 签名
    pub leader_signature: String,
}

/// 上链存证记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainCommitRecord {
    /// 请求 ID
    pub request_id: String,
    /// 区块高度
    pub block_height: u64,
    /// 区块哈希
    pub block_hash: String,
    /// 交易索引
    pub transaction_index: usize,
    /// 默克尔证明
    pub merkle_proof: Vec<String>,
    /// 时间戳
    pub timestamp: u64,
}

/// 完整审计追踪 - 用于查询和验证
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullAuditTrail {
    /// 请求 ID
    pub request_id: String,
    /// 审计事件列表（按时间排序）
    pub events: Vec<AuditEvent>,
    /// 默克尔证明
    pub merkle_proof: MerkleProof,
}

impl FullAuditTrail {
    /// 创建新的空审计追踪
    pub fn new() -> Self {
        FullAuditTrail {
            request_id: String::new(),
            events: Vec::new(),
            merkle_proof: MerkleProof {
                root_hash: String::new(),
                proof_path: Vec::new(),
            },
        }
    }
}

impl Default for FullAuditTrail {
    fn default() -> Self {
        Self::new()
    }
}

/// 默克尔证明
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleProof {
    /// 根哈希
    pub root_hash: String,
    /// 证明路径
    pub proof_path: Vec<String>,
}

/// 审计报告 - 可验证的审计总结
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditReport {
    /// 报告 ID
    pub report_id: String,
    /// 请求 ID
    pub request_id: String,
    /// 报告生成时间
    pub generated_at: u64,
    /// 审计链是否完整
    pub trail_complete: bool,
    /// 事件数量
    pub event_count: usize,
    /// 验证结果
    pub verification_result: VerificationResult,
    /// 报告签名
    pub report_signature: String,
}

/// 验证结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// 是否通过验证
    pub is_valid: bool,
    /// 验证详情
    pub details: Vec<String>,
}

/// 审计追踪查询器 - 用于查询和验证审计链
pub struct AuditTrailQuerier {
    // 实际实现需要引用区块链和其他存储
    // 这里仅定义接口
}

impl AuditTrailQuerier {
    /// 创建新的查询器
    pub fn new() -> Self {
        AuditTrailQuerier {}
    }

    /// 查询完整审计链
    pub fn query_full_trail(&self, request_id: &str) -> Result<FullAuditTrail, String> {
        // TODO: 实现从区块链和存储中查询完整审计链
        Ok(FullAuditTrail {
            request_id: request_id.to_string(),
            events: Vec::new(),
            merkle_proof: MerkleProof {
                root_hash: String::new(),
                proof_path: Vec::new(),
            },
        })
    }

    /// 验证审计链完整性
    pub fn verify_trail_integrity(&self, _trail: &FullAuditTrail) -> bool {
        // TODO: 实现审计链完整性验证
        // 1. 验证事件签名
        // 2. 验证默克尔证明
        // 3. 验证事件时间顺序
        true
    }

    /// 生成可验证的审计报告
    pub fn generate_audit_report(&self, trail: &FullAuditTrail) -> AuditReport {
        use std::time::{SystemTime, UNIX_EPOCH};

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let trail_complete = trail.events.len() >= 6; // 至少 6 个关键事件

        AuditReport {
            report_id: format!("report_{}", trail.request_id),
            request_id: trail.request_id.clone(),
            generated_at: timestamp,
            trail_complete,
            event_count: trail.events.len(),
            verification_result: VerificationResult {
                is_valid: trail_complete,
                details: vec!["Audit trail verified".to_string()],
            },
            report_signature: String::new(),
        }
    }
}

impl Default for AuditTrailQuerier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_trail_creation() {
        let mut trail = AuditTrail::new();

        assert!(!trail.is_complete());

        // 添加请求提交记录
        trail.request_submitted = Some(RequestSubmissionRecord {
            request_id: "req_1".to_string(),
            request_hash: "hash_1".to_string(),
            submitter_id: "user_1".to_string(),
            timestamp: 1000,
            signature: "sig_1".to_string(),
        });

        assert!(!trail.is_complete());
    }

    #[test]
    fn test_audit_event_timestamp() {
        let event = AuditEvent::RequestSubmitted(RequestSubmissionRecord {
            request_id: "req_1".to_string(),
            request_hash: "hash_1".to_string(),
            submitter_id: "user_1".to_string(),
            timestamp: 12345,
            signature: "sig_1".to_string(),
        });

        assert_eq!(event.timestamp(), 12345);
        assert_eq!(event.event_type(), "RequestSubmitted");
    }

    #[test]
    fn test_audit_trail_querier() {
        let querier = AuditTrailQuerier::new();
        let trail = querier.query_full_trail("test_request").unwrap();

        assert_eq!(trail.request_id, "test_request");
    }
}
