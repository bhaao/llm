//! 验证 SDK - P3-2：端到端验证工具
//!
//! **设计目标**：
//! - 提供简单易用的验证 API
//! - 支持多种验证模式（在线/离线）
//! - 支持批量验证
//! - 生成验证报告
//!
//! **核心功能**：
//! 1. 验证推理包完整性
//! 2. 验证质量证明有效性
//! 3. 验证共识结果正确性
//! 4. 验证审计追踪一致性
//! 5. 生成详细验证报告

use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;
use tracing::{info, instrument};
use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};

use crate::verifiable_package::{
    VerifiableInferencePackage, VerificationResult,
};
use crate::quality_assessment::QualityProof;
use crate::consensus::quality_aware_consensus::{
    QualityAwareConsensusManager, WeightedVoteResult, ConsensusDecision,
};

/// SDK 配置
#[derive(Debug, Clone)]
pub struct VerificationSDKConfig {
    /// 是否启用在线验证（需要连接节点）
    pub enable_online_verification: bool,
    /// 是否启用离线验证（本地验证）
    pub enable_offline_verification: bool,
    /// 是否验证质量证明
    pub verify_quality_proof: bool,
    /// 是否验证共识结果
    pub verify_consensus: bool,
    /// 是否验证审计追踪
    pub verify_audit_trail: bool,
    /// 最小置信度阈值
    pub min_confidence_threshold: f64,
    /// 是否生成详细报告
    pub generate_detailed_report: bool,
}

impl Default for VerificationSDKConfig {
    fn default() -> Self {
        VerificationSDKConfig {
            enable_online_verification: false,
            enable_offline_verification: true,
            verify_quality_proof: true,
            verify_consensus: true,
            verify_audit_trail: true,
            min_confidence_threshold: 0.7,
            generate_detailed_report: true,
        }
    }
}

/// 验证报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    /// 报告 ID
    pub report_id: String,
    /// 包 ID
    pub package_id: String,
    /// 验证时间戳
    pub verified_at: u64,
    /// 总体结果
    pub overall_result: VerificationStatus,
    /// 包验证结果
    pub package_verification: VerificationResult,
    /// 质量验证结果（如果有）
    pub quality_verification: Option<QualityVerificationResult>,
    /// 共识验证结果（如果有）
    pub consensus_verification: Option<ConsensusVerificationResult>,
    /// 审计验证结果（如果有）
    pub audit_verification: Option<AuditVerificationResult>,
    /// 总体置信度
    pub confidence: f64,
    /// 警告列表
    pub warnings: Vec<String>,
    /// 错误列表
    pub errors: Vec<String>,
}

/// 验证状态
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum VerificationStatus {
    /// 完全有效
    Valid,
    /// 部分有效（有警告）
    PartiallyValid,
    /// 无效（有错误）
    Invalid,
    /// 无法验证
    Unverifiable,
}

/// 质量验证结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityVerificationResult {
    /// 是否有效
    pub is_valid: bool,
    /// 质量分数
    pub quality_score: f64,
    /// 证明 ID
    pub proof_id: String,
    /// 验证器 ID
    pub validator_id: String,
    /// 详情
    pub details: String,
}

/// 共识验证结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusVerificationResult {
    /// 是否有效
    pub is_valid: bool,
    /// 共识决策
    pub decision: ConsensusDecision,
    /// 加权分数
    pub weighted_score: f64,
    /// 投票数
    pub vote_count: usize,
    /// 置信度
    pub confidence: f64,
    /// 详情
    pub details: String,
}

/// 审计验证结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditVerificationResult {
    /// 是否完整
    pub is_complete: bool,
    /// 事件数量
    pub event_count: usize,
    /// 时间线是否连续
    pub timeline_continuous: bool,
    /// 详情
    pub details: String,
}

/// 验证 SDK
///
/// **核心职责**：
/// - 提供统一的验证接口
/// - 协调多个验证器
/// - 生成验证报告
/// - 支持批量验证
pub struct VerificationSDK {
    /// SDK 配置
    config: VerificationSDKConfig,
    /// 质量共识管理器（用于在线验证）
    consensus_manager: Option<Arc<QualityAwareConsensusManager>>,
    /// 验证历史
    verification_history: Arc<RwLock<Vec<VerificationReport>>>,
}

impl VerificationSDK {
    /// 创建新的验证 SDK
    pub fn new(config: VerificationSDKConfig) -> Self {
        VerificationSDK {
            config,
            consensus_manager: None,
            verification_history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 设置共识管理器（用于在线验证）
    pub fn with_consensus_manager(mut self, manager: Arc<QualityAwareConsensusManager>) -> Self {
        self.consensus_manager = Some(manager);
        self
    }

    /// 验证推理包（主接口）
    #[instrument(skip(self, package), fields(package_id = %package.package_id))]
    pub async fn verify(&self, package: &VerifiableInferencePackage) -> Result<VerificationReport> {
        info!("Starting verification for package: {}", package.package_id);

        let mut warnings = Vec::new();
        let mut errors = Vec::new();
        let mut overall_status = VerificationStatus::Valid;

        // 1. 包完整性验证
        let package_result = package.verify();
        
        if !package_result.is_valid {
            errors.push("Package integrity verification failed".to_string());
            overall_status = VerificationStatus::Invalid;
        }

        let confidence = package_result.confidence;

        // 2. 质量证明验证
        let quality_result = if self.config.verify_quality_proof {
            if let Some(ref proof) = package.quality_proof {
                let result = self.verify_quality_proof(proof, package).await;
                if !result.is_valid {
                    warnings.push(format!(
                        "Quality proof validation failed: {}",
                        result.details
                    ));
                    if overall_status == VerificationStatus::Valid {
                        overall_status = VerificationStatus::PartiallyValid;
                    }
                }
                Some(result)
            } else {
                warnings.push("Quality proof is missing".to_string());
                None
            }
        } else {
            None
        };

        // 3. 共识结果验证
        let consensus_result = if self.config.verify_consensus {
            if let Some(ref result) = package.consensus_result {
                let verification = self.verify_consensus_result(result).await;
                if !verification.is_valid {
                    warnings.push(format!(
                        "Consensus validation failed: {}",
                        verification.details
                    ));
                    if overall_status == VerificationStatus::Valid {
                        overall_status = VerificationStatus::PartiallyValid;
                    }
                }
                Some(verification)
            } else {
                warnings.push("Consensus result is missing".to_string());
                None
            }
        } else {
            None
        };

        // 4. 审计追踪验证
        let audit_result = if self.config.verify_audit_trail {
            if let Some(ref trail) = package.audit_trail {
                let verification = self.verify_audit_trail(trail, package).await;
                if !verification.is_complete {
                    warnings.push(format!(
                        "Audit trail is incomplete: {}",
                        verification.details
                    ));
                }
                Some(verification)
            } else {
                warnings.push("Audit trail is missing".to_string());
                None
            }
        } else {
            None
        };

        // 5. 检查置信度阈值
        if confidence < self.config.min_confidence_threshold {
            errors.push(format!(
                "Confidence {:.2} is below threshold {:.2}",
                confidence, self.config.min_confidence_threshold
            ));
            overall_status = VerificationStatus::Invalid;
        }

        // 生成报告
        let report = VerificationReport {
            report_id: generate_report_id(),
            package_id: package.package_id.clone(),
            verified_at: current_timestamp(),
            overall_result: overall_status,
            package_verification: package_result,
            quality_verification: quality_result,
            consensus_verification: consensus_result,
            audit_verification: audit_result,
            confidence,
            warnings,
            errors,
        };

        // 保存验证历史
        self.save_verification_history(&report).await;

        info!(
            "Verification completed for package {}: {:?}",
            package.package_id, report.overall_result
        );

        Ok(report)
    }

    /// 批量验证
    pub async fn verify_batch(
        &self,
        packages: &[VerifiableInferencePackage],
    ) -> Vec<Result<VerificationReport>> {
        let mut results = Vec::with_capacity(packages.len());

        for package in packages {
            let result = self.verify(package).await;
            results.push(result);
        }

        results
    }

    /// 快速验证（仅验证包完整性）
    pub fn verify_quick(&self, package: &VerifiableInferencePackage) -> VerificationResult {
        package.verify()
    }

    /// 获取验证历史
    pub async fn get_verification_history(&self) -> Vec<VerificationReport> {
        let history = self.verification_history.read().await;
        history.clone()
    }

    /// 获取验证统计
    pub async fn get_verification_stats(&self) -> VerificationStats {
        let history = self.verification_history.read().await;
        
        let total = history.len();
        let valid = history.iter().filter(|r| r.overall_result == VerificationStatus::Valid).count();
        let partially_valid = history.iter().filter(|r| r.overall_result == VerificationStatus::PartiallyValid).count();
        let invalid = history.iter().filter(|r| r.overall_result == VerificationStatus::Invalid).count();
        
        let avg_confidence = if total > 0 {
            history.iter().map(|r| r.confidence).sum::<f64>() / total as f64
        } else {
            0.0
        };

        VerificationStats {
            total_verifications: total,
            valid_count: valid,
            partially_valid_count: partially_valid,
            invalid_count: invalid,
            average_confidence: avg_confidence,
        }
    }

    // ========== 内部验证方法 ==========

    async fn verify_quality_proof(
        &self,
        proof: &QualityProof,
        package: &VerifiableInferencePackage,
    ) -> QualityVerificationResult {
        // 验证证明字段完整性
        let is_valid = !proof.proof_id.is_empty()
            && !proof.output_hash.is_empty()
            && !proof.validator_id.is_empty()
            && proof.quality_score >= 0.0
            && proof.quality_score <= 1.0;

        // 验证输出哈希匹配
        let output_hash_matches = proof.output_hash == calculate_output_hash(&package.response.completion);

        QualityVerificationResult {
            is_valid: is_valid && output_hash_matches,
            quality_score: proof.quality_score,
            proof_id: proof.proof_id.clone(),
            validator_id: proof.validator_id.clone(),
            details: if is_valid && output_hash_matches {
                format!("Quality proof is valid with score {:.2}", proof.quality_score)
            } else if !output_hash_matches {
                "Output hash mismatch".to_string()
            } else {
                "Quality proof is incomplete".to_string()
            },
        }
    }

    async fn verify_consensus_result(
        &self,
        result: &WeightedVoteResult,
    ) -> ConsensusVerificationResult {
        let is_valid = result.vote_count > 0
            && result.weighted_score >= 0.0
            && result.weighted_score <= 1.0
            && result.confidence >= 0.0
            && result.confidence <= 1.0;

        ConsensusVerificationResult {
            is_valid,
            decision: result.decision,
            weighted_score: result.weighted_score,
            vote_count: result.vote_count,
            confidence: result.confidence,
            details: if is_valid {
                format!(
                    "Consensus is valid: {:?} with {} votes (score: {:.2})",
                    result.decision, result.vote_count, result.weighted_score
                )
            } else {
                "Consensus result is invalid".to_string()
            },
        }
    }

    async fn verify_audit_trail(
        &self,
        trail: &crate::audit::FullAuditTrail,
        package: &VerifiableInferencePackage,
    ) -> AuditVerificationResult {
        // 验证请求 ID 匹配
        let request_id_matches = trail.request_id == package.request.request_id;
        
        // 验证事件数量
        let event_count = trail.events.len();
        let has_events = event_count > 0;

        // 验证时间线连续性（简化实现）
        let timeline_continuous = if event_count >= 2 {
            // 检查事件时间戳是否递增
            let timestamps: Vec<_> = trail.events.iter().map(|e| e.timestamp()).collect();
            timestamps.windows(2).all(|w| w[0] <= w[1])
        } else {
            true
        };

        AuditVerificationResult {
            is_complete: request_id_matches && has_events,
            event_count,
            timeline_continuous,
            details: if request_id_matches && has_events {
                format!(
                    "Audit trail is complete with {} events",
                    event_count
                )
            } else if !request_id_matches {
                "Request ID mismatch in audit trail".to_string()
            } else {
                "Audit trail has no events".to_string()
            },
        }
    }

    async fn save_verification_history(&self, report: &VerificationReport) {
        let mut history = self.verification_history.write().await;
        history.push(report.clone());

        // 保持历史记录数量在合理范围内
        if history.len() > 1000 {
            history.remove(0);
        }
    }
}

/// 验证统计
#[derive(Debug, Clone, Default, Serialize)]
pub struct VerificationStats {
    /// 总验证数
    pub total_verifications: usize,
    /// 有效数
    pub valid_count: usize,
    /// 部分有效数
    pub partially_valid_count: usize,
    /// 无效数
    pub invalid_count: usize,
    /// 平均置信度
    pub average_confidence: f64,
}

// 辅助函数

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn generate_report_id() -> String {
    use sha2::{Sha256, Digest};
    let timestamp = current_timestamp();
    let random = rand::random::<u64>();
    let data = format!("{}:{}", timestamp, random);
    let hash = Sha256::digest(data.as_bytes());
    format!("report_{:x}", hash)
}

fn calculate_output_hash(output: &str) -> String {
    let hash = Sha256::digest(output.as_bytes());
    format!("{:x}", hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verifiable_package::VerifiableInferencePackage;
    use crate::provider_layer::InferenceRequest;

    fn create_test_package() -> VerifiableInferencePackage {
        let request = InferenceRequest {
            request_id: "test_req".to_string(),
            prompt: "test".to_string(),
            model_id: "model".to_string(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 0.9,
            stop_sequences: vec![],
        };

        let mut response = crate::provider_layer::InferenceResponse::new("test_req".to_string());
        response.completion = "test completion".to_string();
        response.success = true;

        VerifiableInferencePackage::new(request, response, "node_1")
            .sign("test_signature")
    }

    #[tokio::test]
    async fn test_sdk_verification() {
        let config = VerificationSDKConfig::default();
        let sdk = VerificationSDK::new(config);

        let package = create_test_package();
        let report = sdk.verify(&package).await.unwrap();

        assert_eq!(report.package_id, package.package_id);
        assert!(report.confidence > 0.0);
        assert!(!report.warnings.is_empty()); // 应该有警告因为缺少质量证明等
    }

    #[tokio::test]
    async fn test_quick_verification() {
        let sdk = VerificationSDK::new(VerificationSDKConfig::default());
        let package = create_test_package();

        let result = sdk.verify_quick(&package);

        assert!(result.confidence > 0.8);
    }

    #[tokio::test]
    async fn test_verification_stats() {
        let sdk = VerificationSDK::new(VerificationSDKConfig::default());
        
        let stats = sdk.get_verification_stats().await;
        assert_eq!(stats.total_verifications, 0);

        // 验证一个包
        let package = create_test_package();
        sdk.verify(&package).await.ok();

        let stats = sdk.get_verification_stats().await;
        assert_eq!(stats.total_verifications, 1);
    }
}
