use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use crate::traits::{Hashable, Serializable, Verifiable};

/// 李群聚合证明 - 第四层存证扩展
///
/// 包含李群聚合的关键信息，用于链上验证：
/// - 聚合的全局李群状态哈希
/// - 参与聚合的节点列表
/// - 聚合距离度量结果
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LieGroupAggregationProof {
    /// 全局李群状态哈希 hash(G)
    pub global_state_hash: String,
    /// 参与聚合的节点 ID 列表
    pub contributor_ids: Vec<String>,
    /// 聚合距离度量（与参考状态的距離）
    pub aggregation_distance: f64,
    /// 是否通过验证
    pub is_validated: bool,
}

impl LieGroupAggregationProof {
    /// 创建新的李群聚合证明
    pub fn new(
        global_state_hash: String,
        contributor_ids: Vec<String>,
        aggregation_distance: f64,
    ) -> Self {
        LieGroupAggregationProof {
            global_state_hash,
            contributor_ids,
            aggregation_distance,
            is_validated: false,
        }
    }

    /// 标记为已验证
    pub fn mark_validated(&mut self) {
        self.is_validated = true;
    }

    /// 计算证明哈希
    pub fn hash(&self) -> String {
        let data = format!(
            "{}:{}:{}",
            self.global_state_hash,
            self.contributor_ids.join(","),
            self.aggregation_distance
        );
        format!("{:x}", Sha256::digest(data.as_bytes()))
    }
}

/// 区块元数据，用于记录 AI 模型推理相关信息
///
/// # 李群扩展（第四层）
///
/// 扩展支持李群聚合证明：
/// - `lie_group_aggregation`: 李群聚合证明（可选）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockMetadata {
    /// 模型名称
    pub model_name: String,
    /// 模型版本
    pub model_version: String,
    /// 输入 token 数
    pub prompt_tokens: u64,
    /// 输出 token 数
    pub completion_tokens: u64,
    /// 推理耗时 (毫秒)
    pub inference_time_ms: u64,
    /// 计算成本
    pub compute_cost: f64,
    /// 服务提供商
    pub provider: String,
    /// 李群聚合证明（可选）
    #[serde(default)]
    pub lie_group_aggregation: Option<LieGroupAggregationProof>,
}

impl BlockMetadata {
    /// 创建新的元数据
    pub fn new(
        model_name: String,
        model_version: String,
        prompt_tokens: u64,
        completion_tokens: u64,
        inference_time_ms: u64,
        compute_cost: f64,
        provider: String,
    ) -> Self {
        BlockMetadata {
            model_name,
            model_version,
            prompt_tokens,
            completion_tokens,
            inference_time_ms,
            compute_cost,
            provider,
            lie_group_aggregation: None,
        }
    }

    /// 创建带李群聚合证明的元数据
    pub fn with_lie_group(
        model_name: String,
        model_version: String,
        prompt_tokens: u64,
        completion_tokens: u64,
        inference_time_ms: u64,
        compute_cost: f64,
        provider: String,
        lie_group_proof: LieGroupAggregationProof,
    ) -> Self {
        BlockMetadata {
            model_name,
            model_version,
            prompt_tokens,
            completion_tokens,
            inference_time_ms,
            compute_cost,
            provider,
            lie_group_aggregation: Some(lie_group_proof),
        }
    }

    /// 创建默认元数据（用于测试或空区块）
    pub fn default() -> Self {
        BlockMetadata {
            model_name: String::from("unknown"),
            model_version: String::from("0.0.0"),
            prompt_tokens: 0,
            completion_tokens: 0,
            inference_time_ms: 0,
            compute_cost: 0.0,
            provider: String::from("unknown"),
            lie_group_aggregation: None,
        }
    }

    /// 获取总 token 数
    pub fn total_tokens(&self) -> u64 {
        self.prompt_tokens + self.completion_tokens
    }

    /// 获取每秒 token 处理速度
    pub fn tokens_per_second(&self) -> f64 {
        if self.inference_time_ms == 0 {
            return 0.0;
        }
        self.total_tokens() as f64 / (self.inference_time_ms as f64 / 1000.0)
    }

    /// 设置李群聚合证明
    pub fn set_lie_group_aggregation(&mut self, proof: LieGroupAggregationProof) {
        self.lie_group_aggregation = Some(proof);
    }

    /// 获取李群聚合证明
    pub fn lie_group_aggregation(&self) -> Option<&LieGroupAggregationProof> {
        self.lie_group_aggregation.as_ref()
    }
}

impl Hashable for BlockMetadata {
    fn hash(&self) -> String {
        // 包含李群聚合证明哈希（如果存在）
        let lie_group_hash = self.lie_group_aggregation
            .as_ref()
            .map(|p| p.hash())
            .unwrap_or_else(|| String::from(""));

        let data = format!(
            "{}:{}:{}:{}:{}:{}:{}",
            self.model_name,
            self.model_version,
            self.prompt_tokens,
            self.completion_tokens,
            self.inference_time_ms,
            self.provider,
            lie_group_hash
        );
        format!("{:x}", Sha256::digest(data.as_bytes()))
    }
}

impl Verifiable for BlockMetadata {
    fn verify(&self) -> bool {
        !self.model_name.is_empty() && !self.model_version.is_empty() && !self.provider.is_empty()
    }

    fn verify_with_error(&self) -> Result<(), String> {
        if self.model_name.is_empty() {
            return Err("Model name is empty".to_string());
        }
        if self.model_version.is_empty() {
            return Err("Model version is empty".to_string());
        }
        if self.provider.is_empty() {
            return Err("Provider is empty".to_string());
        }
        Ok(())
    }
}

impl Serializable for BlockMetadata {
    fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| e.to_string())
    }

    fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_creation() {
        let metadata = BlockMetadata::new(
            "GPT-4".to_string(),
            "1.0.0".to_string(),
            100,
            200,
            500,
            0.002,
            "OpenAI".to_string(),
        );

        assert_eq!(metadata.model_name, "GPT-4");
        assert_eq!(metadata.total_tokens(), 300);
    }

    #[test]
    fn test_metadata_verification() {
        let valid_metadata = BlockMetadata::new(
            "GPT-4".to_string(),
            "1.0.0".to_string(),
            100,
            200,
            500,
            0.002,
            "OpenAI".to_string(),
        );
        assert!(valid_metadata.verify());

        let invalid_metadata = BlockMetadata::new(
            "".to_string(),
            "1.0.0".to_string(),
            100,
            200,
            500,
            0.002,
            "OpenAI".to_string(),
        );
        assert!(!invalid_metadata.verify());
    }

    #[test]
    fn test_metadata_serialization() {
        let metadata = BlockMetadata::new(
            "GPT-4".to_string(),
            "1.0.0".to_string(),
            100,
            200,
            500,
            0.002,
            "OpenAI".to_string(),
        );

        let json = metadata.to_json().unwrap();
        let restored: BlockMetadata = BlockMetadata::from_json(&json).unwrap();

        assert_eq!(metadata.model_name, restored.model_name);
        assert_eq!(metadata.total_tokens(), restored.total_tokens());
    }
}
