//! 可验证推理包 - P3-1
//!
//! **设计目标**：
//! - 打包推理请求、响应、证明和审计追踪
//! - 支持端到端验证
//! - 支持上链存证
//! - 支持离线验证
//!
//! **包内容**：
//! 1. 推理请求（输入）
//! 2. 推理响应（输出）
//! 3. 质量证明（QaaS 验证结果）
//! 4. 审计追踪（完整流程记录）
//! 5. 节点签名（可验证来源）

use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use anyhow::{Result, Context};

use crate::provider_layer::{InferenceRequest, InferenceResponse};
use crate::quality_assessment::{QualityProof, QualityAssessment};
use crate::audit::FullAuditTrail;
use crate::consensus::quality_aware_consensus::WeightedVoteResult;

/// 可验证推理包
///
/// **用途**：
/// - 作为推理结果的完整凭证
/// - 支持第三方验证
/// - 支持上链存证
/// - 支持离线审计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiableInferencePackage {
    /// 包 ID（唯一标识）
    pub package_id: String,
    /// 版本号
    pub version: String,
    /// 创建时间戳
    pub created_at: u64,
    /// 推理请求
    pub request: InferenceRequest,
    /// 推理响应
    pub response: InferenceResponse,
    /// 质量证明
    pub quality_proof: Option<QualityProof>,
    /// 质量评估结果
    pub quality_assessment: Option<QualityAssessment>,
    /// 共识投票结果
    pub consensus_result: Option<WeightedVoteResult>,
    /// 审计追踪
    pub audit_trail: Option<FullAuditTrail>,
    /// 参与节点列表
    pub participating_nodes: Vec<NodeParticipation>,
    /// 包哈希（用于快速验证）
    pub package_hash: String,
    /// 创建者签名
    pub creator_signature: String,
}

/// 节点参与记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeParticipation {
    /// 节点 ID
    pub node_id: String,
    /// 参与角色
    pub role: NodeRole,
    /// 贡献内容哈希
    pub contribution_hash: String,
    /// 节点签名
    pub signature: String,
    /// 时间戳
    pub timestamp: u64,
}

/// 节点角色
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum NodeRole {
    /// 推理执行节点
    InferenceExecutor,
    /// 质量验证节点
    QualityValidator,
    /// 共识投票节点
    ConsensusVoter,
    /// 区块打包节点
    BlockProducer,
}

/// 验证结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// 是否有效
    pub is_valid: bool,
    /// 验证详情
    pub details: Vec<VerificationDetail>,
    /// 总体置信度
    pub confidence: f64,
}

/// 验证详情
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationDetail {
    /// 验证项名称
    pub item: String,
    /// 是否通过
    pub passed: bool,
    /// 详情描述
    pub description: String,
}

impl VerifiableInferencePackage {
    /// 创建新的推理包
    pub fn new(
        request: InferenceRequest,
        response: InferenceResponse,
        _creator_node_id: &str,
    ) -> Self {
        let now = current_timestamp();
        let package_id = generate_package_id(&request, now);

        VerifiableInferencePackage {
            package_id: package_id.clone(),
            version: "1.0.0".to_string(),
            created_at: now,
            request,
            response,
            quality_proof: None,
            quality_assessment: None,
            consensus_result: None,
            audit_trail: None,
            participating_nodes: Vec::new(),
            package_hash: String::new(),
            creator_signature: String::new(),
        }
    }

    /// 添加质量证明
    pub fn with_quality_proof(mut self, proof: QualityProof, assessment: QualityAssessment) -> Self {
        self.quality_proof = Some(proof);
        self.quality_assessment = Some(assessment);
        self.update_hash();
        self
    }

    /// 添加共识结果
    pub fn with_consensus_result(mut self, result: WeightedVoteResult) -> Self {
        self.consensus_result = Some(result);
        self.update_hash();
        self
    }

    /// 添加审计追踪
    pub fn with_audit_trail(mut self, trail: FullAuditTrail) -> Self {
        self.audit_trail = Some(trail);
        self.update_hash();
        self
    }

    /// 添加节点参与记录
    pub fn add_node_participation(
        mut self,
        node_id: String,
        role: NodeRole,
        contribution_hash: String,
        signature: String,
    ) -> Self {
        let participation = NodeParticipation {
            node_id,
            role,
            contribution_hash,
            signature,
            timestamp: current_timestamp(),
        };
        self.participating_nodes.push(participation);
        self.update_hash();
        self
    }

    /// 设置创建者签名
    pub fn sign(mut self, signature: &str) -> Self {
        self.creator_signature = signature.to_string();
        self.update_hash();
        self
    }

    /// 更新包哈希
    fn update_hash(&mut self) {
        self.package_hash = self.calculate_hash();
    }

    /// 计算包哈希
    pub fn calculate_hash(&self) -> String {
        let mut hasher = Sha256::new();
        
        // 哈希请求
        hasher.update(self.request.request_id.as_bytes());
        hasher.update(self.request.prompt.as_bytes());
        
        // 哈希响应
        hasher.update(self.response.request_id.as_bytes());
        hasher.update(self.response.completion.as_bytes());
        
        // 哈希质量证明（如果有）
        if let Some(ref proof) = self.quality_proof {
            hasher.update(proof.proof_id.as_bytes());
            hasher.update(proof.output_hash.as_bytes());
        }
        
        // 哈希共识结果（如果有）
        if let Some(ref result) = self.consensus_result {
            hasher.update(result.request_id.as_bytes());
        }
        
        // 哈希审计追踪（如果有）
        if let Some(ref trail) = self.audit_trail {
            hasher.update(trail.request_id.as_bytes());
        }
        
        // 哈希节点参与
        for node in &self.participating_nodes {
            hasher.update(node.node_id.as_bytes());
            hasher.update(node.contribution_hash.as_bytes());
        }
        
        // 时间戳
        hasher.update(self.created_at.to_le_bytes());
        
        format!("{:x}", hasher.finalize())
    }

    /// 验证包的完整性
    pub fn verify(&self) -> VerificationResult {
        let mut details = Vec::new();
        let mut all_passed = true;
        let mut confidence: f64 = 1.0;

        // 1. 验证包哈希
        let hash_valid = self.package_hash == self.calculate_hash();
        details.push(VerificationDetail {
            item: "Package Hash".to_string(),
            passed: hash_valid,
            description: if hash_valid {
                "Package hash matches".to_string()
            } else {
                "Package hash mismatch - possible tampering".to_string()
            },
        });
        if !hash_valid {
            all_passed = false;
            confidence -= 0.3;
        }

        // 2. 验证请求 - 响应匹配
        let request_response_match = self.request.request_id == self.response.request_id;
        details.push(VerificationDetail {
            item: "Request-Response Match".to_string(),
            passed: request_response_match,
            description: if request_response_match {
                "Request ID matches response ID".to_string()
            } else {
                "Request ID does not match response ID".to_string()
            },
        });
        if !request_response_match {
            all_passed = false;
            confidence -= 0.3;
        }

        // 3. 验证质量证明（如果有）
        if let Some(ref proof) = self.quality_proof {
            let proof_valid = !proof.proof_id.is_empty() && !proof.output_hash.is_empty();
            details.push(VerificationDetail {
                item: "Quality Proof".to_string(),
                passed: proof_valid,
                description: if proof_valid {
                    format!("Quality proof present (score: {:.2})", proof.quality_score)
                } else {
                    "Quality proof is incomplete".to_string()
                },
            });
            if !proof_valid {
                all_passed = false;
                confidence -= 0.2;
            }
        }

        // 4. 验证共识结果（如果有）
        if let Some(ref result) = self.consensus_result {
            let consensus_valid = result.vote_count > 0;
            details.push(VerificationDetail {
                item: "Consensus Result".to_string(),
                passed: consensus_valid,
                description: if consensus_valid {
                    format!(
                        "Consensus reached with {} votes (score: {:.2}, decision: {:?})",
                        result.vote_count, result.weighted_score, result.decision
                    )
                } else {
                    "Consensus result is incomplete".to_string()
                },
            });
            if !consensus_valid {
                all_passed = false;
                confidence -= 0.2;
            }
        }

        // 5. 验证节点参与
        let has_executor = self.participating_nodes
            .iter()
            .any(|n| n.role == NodeRole::InferenceExecutor);
        details.push(VerificationDetail {
            item: "Node Participation".to_string(),
            passed: has_executor,
            description: if has_executor {
                format!("{} nodes participated", self.participating_nodes.len())
            } else {
                "No inference executor found".to_string()
            },
        });
        if !has_executor {
            all_passed = false;
            confidence -= 0.1;
        }

        // 6. 验证创建者签名
        let signature_valid = !self.creator_signature.is_empty();
        details.push(VerificationDetail {
            item: "Creator Signature".to_string(),
            passed: signature_valid,
            description: if signature_valid {
                "Creator signature present".to_string()
            } else {
                "Creator signature missing".to_string()
            },
        });
        if !signature_valid {
            confidence = (confidence - 0.1).max(0.0_f64);
        }

        VerificationResult {
            is_valid: all_passed,
            details,
            confidence,
        }
    }

    /// 获取包摘要（用于显示）
    pub fn summary(&self) -> PackageSummary {
        PackageSummary {
            package_id: self.package_id.clone(),
            request_id: self.request.request_id.clone(),
            has_quality_proof: self.quality_proof.is_some(),
            has_consensus: self.consensus_result.is_some(),
            has_audit_trail: self.audit_trail.is_some(),
            node_count: self.participating_nodes.len(),
            package_hash: self.package_hash.clone(),
        }
    }

    /// 序列化为 JSON
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .context("Failed to serialize package to JSON")
    }

    /// 从 JSON 反序列化
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json)
            .context("Failed to deserialize package from JSON")
    }

    /// 获取质量分数
    pub fn quality_score(&self) -> Option<f64> {
        self.quality_assessment
            .as_ref()
            .map(|a| a.overall_score)
            .or_else(|| {
                self.quality_proof
                    .as_ref()
                    .map(|p| p.quality_score)
            })
    }

    /// 获取共识决策
    pub fn consensus_decision(&self) -> Option<crate::consensus::quality_aware_consensus::ConsensusDecision> {
        self.consensus_result
            .as_ref()
            .map(|r| r.decision)
    }
}

/// 包摘要（用于快速查看）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageSummary {
    /// 包 ID
    pub package_id: String,
    /// 请求 ID
    pub request_id: String,
    /// 是否有质量证明
    pub has_quality_proof: bool,
    /// 是否有共识结果
    pub has_consensus: bool,
    /// 是否有审计追踪
    pub has_audit_trail: bool,
    /// 参与节点数
    pub node_count: usize,
    /// 包哈希
    pub package_hash: String,
}

// 辅助函数

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn generate_package_id(request: &InferenceRequest, timestamp: u64) -> String {
    let data = format!("{}:{}:{}", request.request_id, request.prompt, timestamp);
    let hash = Sha256::digest(data.as_bytes());
    format!("pkg_{:x}", hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_request() -> InferenceRequest {
        InferenceRequest {
            request_id: "test_req_1".to_string(),
            prompt: "What is AI?".to_string(),
            model_id: "gpt-4".to_string(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 0.9,
            stop_sequences: vec![],
        }
    }

    fn create_test_response() -> InferenceResponse {
        let mut response = InferenceResponse::new("test_req_1".to_string());
        response.completion = "AI stands for Artificial Intelligence...".to_string();
        response.prompt_tokens = 10;
        response.completion_tokens = 50;
        response.latency_ms = 200;
        response.success = true;
        response
    }

    #[test]
    fn test_package_creation() {
        let request = create_test_request();
        let response = create_test_response();

        let package = VerifiableInferencePackage::new(
            request,
            response,
            "node_1",
        );

        assert!(!package.package_id.is_empty());
        assert_eq!(package.version, "1.0.0");
        assert!(package.participating_nodes.is_empty());
    }

    #[test]
    fn test_package_hash_update() {
        let request = create_test_request();
        let response = create_test_response();

        let mut package = VerifiableInferencePackage::new(
            request,
            response,
            "node_1",
        );

        let hash1 = package.package_hash.clone();
        assert!(!hash1.is_empty());

        // 添加质量证明后哈希应该改变
        use crate::quality_assessment::VerificationEvidence;
        let proof = QualityProof {
            proof_id: "proof_1".to_string(),
            output_hash: "output_hash".to_string(),
            timestamp: 123456,
            validator_id: "validator_1".to_string(),
            quality_score: 0.9,
            evidence: VerificationEvidence::empty(),
            validator_signature: "sig".to_string(),
        };

        let assessment = QualityAssessment {
            overall_score: 0.9,
            kv_cache_valid: true,
            semantic_score: 0.85,
            integrity_score: 0.95,
            is_tampered: false,
            details: Default::default(),
        };

        package = package.with_quality_proof(proof, assessment);
        let hash2 = &package.package_hash;

        assert_ne!(hash1, *hash2);
    }

    #[test]
    fn test_package_verification() {
        let request = create_test_request();
        let response = create_test_response();

        let package = VerifiableInferencePackage::new(
            request.clone(),
            response.clone(),
            "node_1",
        )
        .sign("creator_signature");

        let result = package.verify();

        assert!(result.is_valid);
        assert!(result.confidence > 0.8);
        assert!(!result.details.is_empty());
    }

    #[test]
    fn test_package_serialization() {
        let request = create_test_request();
        let response = create_test_response();

        let package = VerifiableInferencePackage::new(
            request,
            response,
            "node_1",
        );

        let json = package.to_json().unwrap();
        let restored = VerifiableInferencePackage::from_json(&json).unwrap();

        assert_eq!(package.package_id, restored.package_id);
        assert_eq!(package.request.request_id, restored.request.request_id);
    }
}
