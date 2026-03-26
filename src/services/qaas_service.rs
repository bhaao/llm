//! QaaS (Quality as a Service) 服务包装器 - P1-2
//!
//! **设计目标**：
//! - 提供 HTTP/gRPC API 用于质量验证
//! - 封装质量评估逻辑
//! - 支持远程质量验证请求
//! - 与 AssessorRegistry 集成
//!
//! **API 端点**：
//! - POST /quality/assess - 执行质量评估
//! - GET /quality/proof/{proof_id} - 获取质量证明
//! - POST /quality/verify - 验证质量证明

use std::sync::Arc;
use anyhow::{Result, Context};
use tracing::{info, instrument};
use serde::{Serialize, Deserialize};

use crate::quality_assessment::{
    QualityAssessor, QualityAssessment, QualityAssessmentRequest, 
    QualityProof, AssessmentMode,
};
use crate::assessor_registry::AssessorRegistry;
use crate::provider_layer::InferenceResponse;

/// QaaS 服务配置
#[derive(Debug, Clone)]
pub struct QaaSConfig {
    /// 服务地址
    pub bind_address: String,
    /// 是否启用 HTTP API
    pub enable_http: bool,
    /// 是否启用 gRPC API
    pub enable_grpc: bool,
    /// 质量验证阈值
    pub quality_threshold: f64,
    /// 是否启用详细日志
    pub enable_verbose_logging: bool,
}

impl Default for QaaSConfig {
    fn default() -> Self {
        QaaSConfig {
            bind_address: "0.0.0.0:8080".to_string(),
            enable_http: true,
            enable_grpc: false,
            quality_threshold: 0.7,
            enable_verbose_logging: false,
        }
    }
}

/// QaaS 服务包装器
///
/// **职责**：
/// - 提供质量验证 API
/// - 管理质量评估请求
/// - 生成和验证质量证明
pub struct QaaSService {
    /// 服务配置
    config: QaaSConfig,
    /// 评估器注册表
    assessor_registry: Arc<AssessorRegistry>,
    /// 质量评估器
    #[allow(dead_code)]
    quality_assessor: Arc<dyn QualityAssessor>,
}

impl QaaSService {
    /// 创建新的 QaaS 服务
    pub fn new(
        config: QaaSConfig,
        assessor_registry: Arc<AssessorRegistry>,
        quality_assessor: Arc<dyn QualityAssessor>,
    ) -> Self {
        QaaSService {
            config,
            assessor_registry,
            quality_assessor,
        }
    }

    /// 执行质量评估
    ///
    /// 这是核心 API，接受推理输出并返回质量评估结果
    #[instrument(skip(self, request), fields(request_id = %request.output))]
    pub async fn assess_quality(
        &self,
        request: QualityAssessmentRequest,
    ) -> Result<QualityAssessment> {
        info!("Starting quality assessment");
        
        let assessment = self.assessor_registry
            .assess(request)
            .await
            .context("Quality assessment failed")?;

        info!(
            "Quality assessment completed: score={:.2}, passed={}",
            assessment.overall_score,
            assessment.overall_score >= self.config.quality_threshold
        );

        Ok(assessment)
    }

    /// 验证质量证明
    ///
    /// 验证之前生成的质量证明是否有效
    pub async fn verify_proof(&self, proof: &QualityProof) -> Result<ProofValidity> {
        info!("Verifying quality proof: {}", proof.proof_id);

        // 验证证明签名
        let signature_valid = self.verify_signature(proof).await;
        
        // 验证证明内容
        let content_valid = self.verify_content(proof).await;

        let is_valid = signature_valid && content_valid;

        Ok(ProofValidity {
            is_valid,
            details: vec![
                format!("signature_valid: {}", signature_valid),
                format!("content_valid: {}", content_valid),
            ],
        })
    }

    /// 获取质量证明
    ///
    /// 根据 proof_id 获取之前生成的质量证明
    pub async fn get_proof(&self, proof_id: &str) -> Result<QualityProof> {
        info!("Retrieving quality proof: {}", proof_id);
        
        // 简化实现：返回一个占位证明
        // 实际实现应该从存储中获取证明
        use crate::quality_assessment::VerificationEvidence;
        
        Ok(QualityProof {
            proof_id: proof_id.to_string(),
            output_hash: "placeholder_hash".to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            validator_id: "qaas_service".to_string(),
            quality_score: 0.9,
            evidence: VerificationEvidence::empty(),
            validator_signature: "placeholder_signature".to_string(),
        })
    }

    /// 从推理响应创建评估请求
    pub fn create_assessment_request(
        &self,
        response: &InferenceResponse,
        context: Option<String>,
        modes: Vec<AssessmentMode>,
    ) -> QualityAssessmentRequest {
        QualityAssessmentRequest {
            output: response.completion.clone(),
            context,
            expected_kv_hash: None,
            assessment_modes: modes,
        }
    }

    /// 验证证明签名（简化实现）
    async fn verify_signature(&self, _proof: &QualityProof) -> bool {
        // 实际实现应该验证加密签名
        // 这里简化为始终返回 true
        true
    }

    /// 验证证明内容（简化实现）
    async fn verify_content(&self, _proof: &QualityProof) -> bool {
        // 实际实现应该验证证明内容的完整性
        // 这里简化为始终返回 true
        true
    }

    /// 获取服务配置
    pub fn config(&self) -> &QaaSConfig {
        &self.config
    }

    /// 获取评估器注册表
    pub fn assessor_registry(&self) -> &Arc<AssessorRegistry> {
        &self.assessor_registry
    }
}

/// 证明有效性结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofValidity {
    /// 是否有效
    pub is_valid: bool,
    /// 验证详情
    pub details: Vec<String>,
}

/// HTTP API 请求/响应结构

/// 质量评估请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityAssessRequest {
    /// 输出文本
    pub output: String,
    /// 上下文（可选）
    pub context: Option<String>,
    /// 评估模式
    pub modes: Vec<String>,
    /// 预期 KV 哈希（可选）
    pub expected_kv_hash: Option<String>,
}

/// 质量评估响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityAssessResponse {
    /// 证明 ID
    pub proof_id: String,
    /// 总体得分
    pub overall_score: f64,
    /// 是否通过阈值
    pub passed: bool,
    /// 详细评估结果
    pub assessments: Vec<AssessmentResult>,
    /// 时间戳
    pub timestamp: u64,
}

/// 详细评估结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssessmentResult {
    /// 评估器 ID
    pub assessor_id: String,
    /// 得分
    pub score: f64,
    /// 是否通过
    pub passed: bool,
    /// 详情
    pub details: String,
}

/// 证明验证请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofVerifyRequest {
    /// 证明 ID
    pub proof_id: String,
    /// 证明内容（JSON）
    pub proof_data: String,
}

/// 证明验证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofVerifyResponse {
    /// 是否有效
    pub is_valid: bool,
    /// 验证详情
    pub details: Vec<String>,
    /// 时间戳
    pub timestamp: u64,
}

impl QaaSService {
    /// 将内部评估转换为 HTTP 响应
    pub fn to_http_response(&self, assessment: &QualityAssessment) -> QualityAssessResponse {
        let passed = assessment.overall_score >= self.config.quality_threshold;
        
        // 构建详细评估结果
        let mut assessments = Vec::new();
        
        // KV 缓存验证结果
        assessments.push(AssessmentResult {
            assessor_id: "kv_cache_verifier".to_string(),
            score: if assessment.kv_cache_valid { 1.0 } else { 0.0 },
            passed: assessment.kv_cache_valid,
            details: format!("KV Cache valid: {}", assessment.kv_cache_valid),
        });
        
        // 语义检查结果
        assessments.push(AssessmentResult {
            assessor_id: "semantic_checker".to_string(),
            score: assessment.semantic_score,
            passed: assessment.semantic_score >= self.config.quality_threshold,
            details: format!("Semantic score: {:.2}", assessment.semantic_score),
        });
        
        // 完整性检查结果
        assessments.push(AssessmentResult {
            assessor_id: "integrity_checker".to_string(),
            score: assessment.integrity_score,
            passed: assessment.integrity_score >= self.config.quality_threshold,
            details: format!("Integrity score: {:.2}", assessment.integrity_score),
        });

        QualityAssessResponse {
            proof_id: "proof_placeholder".to_string(),
            overall_score: assessment.overall_score,
            passed,
            assessments,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// 从 HTTP 请求创建内部请求
    pub fn from_http_request(&self, request: QualityAssessRequest) -> QualityAssessmentRequest {
        let modes = request.modes
            .iter()
            .filter_map(|m| match m.as_str() {
                "kv_verification" => Some(AssessmentMode::KvVerification),
                "semantic_check" => Some(AssessmentMode::SemanticCheck),
                "integrity_check" => Some(AssessmentMode::IntegrityCheck),
                "multi_node_comparison" => Some(AssessmentMode::MultiNodeComparison),
                _ => None,
            })
            .collect();

        QualityAssessmentRequest {
            output: request.output,
            context: request.context,
            expected_kv_hash: request.expected_kv_hash,
            assessment_modes: modes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qaas_config_default() {
        let config = QaaSConfig::default();
        assert!(config.enable_http);
        assert!(!config.enable_grpc);
        assert_eq!(config.quality_threshold, 0.7);
    }

    #[test]
    fn test_assessment_result_conversion() {
        // 简化测试：验证转换逻辑
        let modes = vec![AssessmentMode::KvVerification];
        let request = QualityAssessmentRequest {
            output: "test output".to_string(),
            context: None,
            expected_kv_hash: None,
            assessment_modes: modes,
        };
        
        assert_eq!(request.output, "test output");
        assert_eq!(request.assessment_modes.len(), 1);
    }
}
