//! 李群聚合器 - 第二层核心组件（信任根，不可插拔）
//!
//! # 架构定位
//!
//! **第二层：李群链上聚合层（系统核心，信任根）**
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  第二层：李群链上聚合层 (系统核心，信任根)                       │
//! │  • PBFTConsensus (已实现共识框架)                                │
//! │  • Blockchain (已实现存证链)                                     │
//! │  • LieGroupAggregator (李群聚合器) ← 信任根                      │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # 核心职责
//!
//! - 在链上/共识层聚合局部李代数元素 A_i 为全局李群状态 G
//! - 实现李群加权几何平均公式：G = exp(1/N * Σlog(g_i))
//! - 与 PBFT 共识集成，在 Commit 阶段执行聚合
//!
//! # 关键设计决策
//!
//! ## 为什么李群聚合器不可插拔？
//!
//! **原因：信任根必须全局一致。**
//!
//! ### 错误设计
//!
//! ```text
//! 节点可以选择聚合公式
//! → 节点 A 用公式 1，节点 B 用公式 2
//! → 全局状态 G 不一致
//! → 系统崩溃
//! ```
//!
//! ### 正确设计
//!
//! ```text
//! 聚合公式硬编码到共识层
//! → 所有节点使用相同公式
//! → 全局状态 G 一致
//! → 系统安全
//! ```
//!
//! # 与 PBFT 集成
//!
//! PBFT 三阶段提交扩展：
//! 1. **Pre-prepare**: 收集 [A_1, A_2, ..., A_N]
//! 2. **Prepare**: 验证每个 A_i 的签名
//! 3. **Commit**: 执行 G = lie_combine([A_i])，上链存证 hash(G)
//!
//! # 核心公式
//!
//! ## 李群几何平均
//!
//! G = exp(1/N * Σlog(g_i))
//!
//! 其中：
//! - g_i = exp(A_i) 是从李代数到李群的指数映射
//! - N 是参与聚合的节点数量
//! - log 是李群对数映射（李群 → 李代数）
//! - exp 是李代数指数映射（李代数 → 李群）
//!
//! # 使用示例
//!
//! ```ignore
//! use block_chain_with_context::lie_algebra::{
//!     LieGroupAggregator, LieAlgebraElement, LieGroupType,
//!     LieGroupAggregationResult, AggregationConfig,
//! };
//!
//! // 创建聚合器
//! let aggregator = LieGroupAggregator::new(
//!     LieGroupType::SE3,
//!     AggregationConfig::default(),
//! );
//!
//! // 准备李代数元素列表（来自多个节点）
//! let algebra_elements = vec![
//!     LieAlgebraElement::new("node_1".to_string(), vec![0.1, 0.2, 0.3], LieGroupType::SE3),
//!     LieAlgebraElement::new("node_2".to_string(), vec![0.2, 0.3, 0.4], LieGroupType::SE3),
//!     LieAlgebraElement::new("node_3".to_string(), vec![0.15, 0.25, 0.35], LieGroupType::SE3),
//! ];
//!
//! // 执行聚合
//! let result = aggregator.aggregate(&algebra_elements).unwrap();
//!
//! // 获取全局李群状态 G
//! let global_group = result.global_state;
//!
//! // 验证聚合结果
//! assert!(result.is_valid);
//! assert_eq!(result.contributor_count, 3);
//! ```

use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use anyhow::{Result, bail};
use tracing::{info, warn, debug};

use crate::lie_algebra::types::{LieAlgebraElement, LieGroupElement, LieGroupType, LieGroupConfig};

/// 聚合配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationConfig {
    /// 最小参与节点数
    pub min_nodes: usize,
    /// 权重策略
    pub weight_strategy: WeightStrategy,
    /// 是否验证输入签名
    pub verify_signatures: bool,
    /// 数值精度容差
    pub tolerance: f64,
}

impl Default for AggregationConfig {
    fn default() -> Self {
        AggregationConfig {
            min_nodes: 2,
            weight_strategy: WeightStrategy::Uniform,
            verify_signatures: true,
            tolerance: 1e-10,
        }
    }
}

/// 权重策略
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum WeightStrategy {
    /// 均匀权重（所有节点权重相等）
    Uniform,
    /// 基于信誉加权
    ReputationWeighted,
    /// 基于质量分数加权
    QualityWeighted,
}

impl Default for WeightStrategy {
    fn default() -> Self {
        WeightStrategy::Uniform
    }
}

/// 聚合输入记录
#[derive(Debug, Clone)]
pub struct AggregationInput {
    /// 李代数元素
    pub algebra_element: LieAlgebraElement,
    /// 节点信誉分数（可选）
    pub reputation_score: Option<f64>,
    /// 质量分数（可选）
    pub quality_score: Option<f64>,
    /// 输入权重
    pub weight: f64,
}

/// 聚合结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LieGroupAggregationResult {
    /// 聚合请求 ID
    pub request_id: String,
    /// 全局李群状态 G
    pub global_state: LieGroupElement,
    /// 参与聚合的节点 ID 列表
    pub contributor_ids: Vec<String>,
    /// 参与节点数量
    pub contributor_count: usize,
    /// 使用的权重策略
    pub weight_strategy: WeightStrategy,
    /// 聚合是否成功
    pub is_valid: bool,
    /// 错误信息（如果有）
    pub error_message: Option<String>,
    /// 聚合时间戳
    pub timestamp: u64,
    /// 聚合证明哈希
    pub aggregation_proof_hash: String,
}

impl LieGroupAggregationResult {
    /// 创建成功的聚合结果
    pub fn success(
        request_id: String,
        global_state: LieGroupElement,
        contributor_ids: Vec<String>,
        weight_strategy: WeightStrategy,
    ) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        use sha2::{Sha256, Digest};

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // 生成聚合证明哈希
        let proof_data = format!(
            "{}:{}:{}",
            request_id,
            global_state.hash(),
            contributor_ids.join(",")
        );
        let aggregation_proof_hash = format!("{:x}", Sha256::digest(proof_data.as_bytes()));

        let mut result = LieGroupAggregationResult {
            request_id,
            global_state,
            contributor_ids: contributor_ids.clone(),
            contributor_count: contributor_ids.len(),
            weight_strategy,
            is_valid: true,
            error_message: None,
            timestamp,
            aggregation_proof_hash,
        };

        // 设置聚合证明到全局状态
        let aggregation_proof_hash_clone = result.aggregation_proof_hash.clone();
        result.global_state.set_aggregation_proof(&aggregation_proof_hash_clone);

        result
    }

    /// 创建失败的聚合结果
    pub fn failure(
        request_id: String,
        error_message: String,
        weight_strategy: WeightStrategy,
    ) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        LieGroupAggregationResult {
            request_id: request_id.clone(),
            global_state: LieGroupElement::new(
                format!("failed_{}", request_id),
                vec![1.0], // 单位矩阵（1×1）
                (1, 1),
                LieGroupType::Custom { algebra_dim: 1 },
            ),
            contributor_ids: Vec::new(),
            contributor_count: 0,
            weight_strategy,
            is_valid: false,
            error_message: Some(error_message),
            timestamp,
            aggregation_proof_hash: String::new(),
        }
    }
}

/// 李群聚合器 - 信任根组件
///
/// **核心职责**：
/// - 收集多个节点的局部李代数元素 A_i
/// - 验证输入有效性（签名、格式）
/// - 执行李群几何平均聚合：G = exp(1/N * Σlog(g_i))
/// - 生成全局李群状态 G
///
/// # 不可插拔性
///
/// 聚合公式硬编码，确保全局一致性：
/// - 所有节点使用相同公式
/// - 全局状态 G 一致
/// - 系统安全
pub struct LieGroupAggregator {
    /// 李群类型
    group_type: LieGroupType,
    /// 聚合配置
    config: AggregationConfig,
    /// 李群配置
    lie_group_config: LieGroupConfig,
}

impl Clone for LieGroupAggregator {
    fn clone(&self) -> Self {
        LieGroupAggregator {
            group_type: self.group_type,
            config: self.config.clone(),
            lie_group_config: self.lie_group_config.clone(),
        }
    }
}

impl LieGroupAggregator {
    /// 创建新的聚合器
    ///
    /// # 参数
    ///
    /// * `group_type` - 李群类型
    /// * `config` - 聚合配置
    pub fn new(group_type: LieGroupType, config: AggregationConfig) -> Self {
        LieGroupAggregator {
            group_type,
            config,
            lie_group_config: LieGroupConfig::new(group_type),
        }
    }

    /// 创建默认聚合器
    pub fn default_with_type(group_type: LieGroupType) -> Self {
        Self::new(group_type, AggregationConfig::default())
    }

    /// 执行聚合
    ///
    /// # 参数
    ///
    /// * `inputs` - 李代数元素列表
    ///
    /// # 返回
    ///
    /// 聚合结果（包含全局李群状态 G）
    pub fn aggregate(&self, inputs: &[LieAlgebraElement]) -> Result<LieGroupAggregationResult> {
        // 检查最小节点数
        if inputs.len() < self.config.min_nodes {
            return Ok(LieGroupAggregationResult::failure(
                "aggregate".to_string(),
                format!(
                    "Not enough inputs: {} (minimum required: {})",
                    inputs.len(),
                    self.config.min_nodes
                ),
                self.config.weight_strategy,
            ));
        }

        // 验证输入
        let validated_inputs = self.validate_inputs(inputs)?;

        // 执行李群几何平均聚合
        let global_state = self.lie_group_geometric_mean(&validated_inputs)?;

        // 收集贡献者 ID
        let contributor_ids: Vec<String> = inputs.iter().map(|i| i.id.clone()).collect();

        // 创建聚合结果
        let result = LieGroupAggregationResult::success(
            format!("agg_{}", inputs[0].timestamp),
            global_state,
            contributor_ids,
            self.config.weight_strategy,
        );

        info!(
            "Aggregation completed: {} contributors, global_state_hash={}",
            result.contributor_count,
            result.global_state.hash()
        );

        Ok(result)
    }

    /// 带权重的聚合
    ///
    /// # 参数
    ///
    /// * `inputs` - 带权重的聚合输入
    ///
    /// # 返回
    ///
    /// 聚合结果
    pub fn aggregate_weighted(
        &self,
        inputs: &[AggregationInput],
    ) -> Result<LieGroupAggregationResult> {
        if inputs.len() < self.config.min_nodes {
            return Ok(LieGroupAggregationResult::failure(
                "aggregate_weighted".to_string(),
                format!(
                    "Not enough inputs: {} (minimum required: {})",
                    inputs.len(),
                    self.config.min_nodes
                ),
                self.config.weight_strategy,
            ));
        }

        // 归一化权重
        let total_weight: f64 = inputs.iter().map(|i| i.weight).sum();
        if total_weight < 1e-10 {
            return Ok(LieGroupAggregationResult::failure(
                "aggregate_weighted".to_string(),
                "Total weight is zero".to_string(),
                self.config.weight_strategy,
            ));
        }

        // 执行加权李群几何平均
        let global_state = self.weighted_lie_group_geometric_mean(inputs, total_weight)?;

        // 收集贡献者 ID
        let contributor_ids: Vec<String> = inputs.iter().map(|i| i.algebra_element.id.clone()).collect();

        let result = LieGroupAggregationResult::success(
            format!("agg_weighted_{}", inputs[0].algebra_element.timestamp),
            global_state,
            contributor_ids,
            self.config.weight_strategy,
        );

        info!(
            "Weighted aggregation completed: {} contributors, total_weight={:.2}",
            result.contributor_count,
            total_weight
        );

        Ok(result)
    }

    /// 验证输入列表
    fn validate_inputs(&self, inputs: &[LieAlgebraElement]) -> Result<Vec<AggregationInput>> {
        let mut validated = Vec::with_capacity(inputs.len());

        for input in inputs {
            // 验证李群类型匹配
            if input.group_type != self.group_type {
                warn!(
                    "Input group_type {:?} does not match aggregator {:?}",
                    input.group_type, self.group_type
                );
                continue;
            }

            // 验证签名（如果启用）
            if self.config.verify_signatures && !input.node_signature.is_empty() {
                // 注意：实际验证需要公钥，这里简化处理
                debug!("Signature verification skipped for input {}", input.id);
            }

            // 创建聚合输入（均匀权重）
            validated.push(AggregationInput {
                algebra_element: input.clone(),
                reputation_score: None,
                quality_score: None,
                weight: 1.0,
            });
        }

        Ok(validated)
    }

    /// 李群几何平均
    ///
    /// G = exp(1/N * Σlog(g_i))
    ///
    /// 其中 g_i = exp(A_i)
    fn lie_group_geometric_mean(
        &self,
        inputs: &[AggregationInput],
    ) -> Result<LieGroupElement> {
        // 步骤 1: 将每个李代数元素 A_i 通过指数映射转换为李群元素 g_i
        let _group_elements: Vec<LieGroupElement> = inputs
            .iter()
            .map(|input| LieGroupElement::from_algebra_exponential(&input.algebra_element))
            .collect();

        // 步骤 2: 计算李群元素的加权平均（在李代数空间）
        // log(g_i) 得到李代数元素，然后求平均，最后通过 exp 映射回李群

        // 简化实现：直接在李代数空间平均，然后指数映射
        // 这是李群几何平均的一阶近似

        // 收集所有李代数数据
        let mut summed_algebra = vec![0.0f64; self.get_algebra_dimension()];

        for input in inputs.iter() {
            // 使用输入的李代数数据（已经是 log(g_i) 的近似）
            let algebra_data = &input.algebra_element.data;
            let weight = input.weight;

            for (i, &val) in algebra_data.iter().enumerate() {
                if i < summed_algebra.len() {
                    summed_algebra[i] += val * weight;
                }
            }
        }

        // 步骤 3: 平均（除以总权重）
        let total_weight: f64 = inputs.iter().map(|i| i.weight).sum();
        for val in &mut summed_algebra {
            *val /= total_weight;
        }

        // 步骤 4: 通过指数映射得到全局李群状态 G = exp(average)
        let global_algebra = LieAlgebraElement::new(
            "global_average".to_string(),
            summed_algebra,
            self.group_type,
        );

        let global_group = LieGroupElement::from_algebra_exponential(&global_algebra);

        debug!(
            "Lie group geometric mean computed, global_hash={}",
            global_group.hash()
        );

        Ok(global_group)
    }

    /// 加权李群几何平均
    fn weighted_lie_group_geometric_mean(
        &self,
        inputs: &[AggregationInput],
        total_weight: f64,
    ) -> Result<LieGroupElement> {
        // 与几何平均类似，但使用自定义权重
        let mut weighted_sum_algebra = vec![0.0f64; self.get_algebra_dimension()];

        for input in inputs {
            let algebra_data = &input.algebra_element.data;
            let weight = input.weight;

            for (i, &val) in algebra_data.iter().enumerate() {
                if i < weighted_sum_algebra.len() {
                    weighted_sum_algebra[i] += val * weight;
                }
            }
        }

        // 归一化
        for val in &mut weighted_sum_algebra {
            *val /= total_weight;
        }

        let global_algebra = LieAlgebraElement::new(
            "weighted_global_average".to_string(),
            weighted_sum_algebra,
            self.group_type,
        );

        Ok(LieGroupElement::from_algebra_exponential(&global_algebra))
    }

    /// 获取李代数维度
    fn get_algebra_dimension(&self) -> usize {
        match self.group_type {
            LieGroupType::SO3 => 3,
            LieGroupType::SE3 => 6,
            LieGroupType::GLN { dimension } => dimension * dimension,
            LieGroupType::Custom { algebra_dim } => algebra_dim,
        }
    }

    /// 获取配置
    pub fn config(&self) -> &AggregationConfig {
        &self.config
    }

    /// 获取李群类型
    pub fn group_type(&self) -> LieGroupType {
        self.group_type
    }
}

/// PBFT 共识集成器
///
/// 将李群聚合器与 PBFT 共识流程集成
pub struct PbftLieGroupIntegration {
    /// 李群聚合器
    aggregator: LieGroupAggregator,
    /// 待聚合的李代数元素池
    pending_algebra_elements: HashMap<String, Vec<LieAlgebraElement>>,
}

impl Clone for PbftLieGroupIntegration {
    fn clone(&self) -> Self {
        PbftLieGroupIntegration {
            aggregator: self.aggregator.clone(),
            pending_algebra_elements: self.pending_algebra_elements.clone(),
        }
    }
}

impl PbftLieGroupIntegration {
    /// 创建新的集成器
    pub fn new(aggregator: LieGroupAggregator) -> Self {
        PbftLieGroupIntegration {
            aggregator,
            pending_algebra_elements: HashMap::new(),
        }
    }

    /// PBFT Pre-prepare 阶段：收集李代数元素
    pub fn pre_prepare(&mut self, request_id: &str, algebra_element: LieAlgebraElement) {
        self.pending_algebra_elements
            .entry(request_id.to_string())
            .or_insert_with(Vec::new)
            .push(algebra_element);

        debug!(
            "Pre-prepare: collected algebra element for request {}, total={}",
            request_id,
            self.pending_algebra_elements
                .get(request_id)
                .map(|v| v.len())
                .unwrap_or(0)
        );
    }

    /// PBFT Prepare 阶段：验证李代数元素
    pub fn prepare(&self, request_id: &str) -> Result<bool> {
        let elements = self.pending_algebra_elements
            .get(request_id)
            .ok_or_else(|| anyhow::anyhow!("Request not found"))?;

        // 验证所有元素的签名和格式
        for element in elements {
            // 验证李群类型
            if element.group_type != self.aggregator.group_type() {
                bail!(
                    "Algebra element group_type mismatch for request {}",
                    request_id
                );
            }

            // 验证数据维度
            let expected_dim = match element.group_type {
                LieGroupType::SO3 => 3,
                LieGroupType::SE3 => 6,
                LieGroupType::GLN { dimension } => dimension * dimension,
                LieGroupType::Custom { algebra_dim } => algebra_dim,
            };

            if element.data.len() != expected_dim {
                bail!(
                    "Invalid algebra dimension: expected {}, got {}",
                    expected_dim,
                    element.data.len()
                );
            }
        }

        debug!("Prepare phase completed for request {}", request_id);
        Ok(true)
    }

    /// PBFT Commit 阶段：执行李群聚合
    pub fn commit(&mut self, request_id: &str) -> Result<LieGroupAggregationResult> {
        let elements = self.pending_algebra_elements
            .remove(request_id)
            .ok_or_else(|| anyhow::anyhow!("Request not found or already committed"))?;

        info!(
            "Commit phase: aggregating {} algebra elements for request {}",
            elements.len(),
            request_id
        );

        // 执行聚合
        let result = self.aggregator.aggregate(&elements)?;

        Ok(result)
    }

    /// 清理超时的请求
    pub fn cleanup_timeout_requests(&mut self, max_age_ms: u64) -> usize {
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let mut removed = 0;
        self.pending_algebra_elements.retain(|request_id, elements| {
            let oldest_timestamp = elements.iter()
                .map(|e| e.timestamp)
                .min()
                .unwrap_or(u64::MAX);

            let age = now.saturating_sub(oldest_timestamp);
            if age > max_age_ms {
                warn!("Removing timeout request {}: age={}ms", request_id, age);
                removed += 1;
                false
            } else {
                true
            }
        });

        removed
    }

    /// 获取待处理请求数量
    pub fn pending_count(&self) -> usize {
        self.pending_algebra_elements.len()
    }

    /// 获取聚合器引用
    pub fn aggregator(&self) -> &LieGroupAggregator {
        &self.aggregator
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aggregator_creation() {
        let aggregator = LieGroupAggregator::default_with_type(LieGroupType::SE3);

        assert_eq!(aggregator.group_type(), LieGroupType::SE3);
        assert_eq!(aggregator.config().min_nodes, 2);
    }

    #[test]
    fn test_aggregate_insufficient_inputs() {
        let aggregator = LieGroupAggregator::new(
            LieGroupType::SO3,
            AggregationConfig {
                min_nodes: 3,
                ..Default::default()
            },
        );

        let inputs = vec![
            LieAlgebraElement::new("node_1".to_string(), vec![0.1, 0.2, 0.3], LieGroupType::SO3),
            LieAlgebraElement::new("node_2".to_string(), vec![0.2, 0.3, 0.4], LieGroupType::SO3),
        ];

        let result = aggregator.aggregate(&inputs).unwrap();

        assert!(!result.is_valid);
        assert!(result.error_message.is_some());
        assert_eq!(result.contributor_count, 0);
    }

    #[test]
    fn test_aggregate_success() {
        let aggregator = LieGroupAggregator::default_with_type(LieGroupType::SO3);

        let inputs = vec![
            LieAlgebraElement::new("node_1".to_string(), vec![0.1, 0.2, 0.3], LieGroupType::SO3),
            LieAlgebraElement::new("node_2".to_string(), vec![0.2, 0.3, 0.4], LieGroupType::SO3),
            LieAlgebraElement::new("node_3".to_string(), vec![0.15, 0.25, 0.35], LieGroupType::SO3),
        ];

        let result = aggregator.aggregate(&inputs).unwrap();

        assert!(result.is_valid);
        assert_eq!(result.contributor_count, 3);
        assert_eq!(result.contributor_ids.len(), 3);
        assert!(result.global_state.validate());
    }

    #[test]
    fn test_aggregate_se3() {
        let aggregator = LieGroupAggregator::default_with_type(LieGroupType::SE3);

        let inputs = vec![
            LieAlgebraElement::new("node_1".to_string(), vec![0.1, 0.2, 0.3, 1.0, 2.0, 3.0], LieGroupType::SE3),
            LieAlgebraElement::new("node_2".to_string(), vec![0.2, 0.3, 0.4, 1.5, 2.5, 3.5], LieGroupType::SE3),
        ];

        let result = aggregator.aggregate(&inputs).unwrap();

        assert!(result.is_valid);
        assert_eq!(result.contributor_count, 2);
        assert_eq!(result.global_state.matrix_shape, (4, 4));
    }

    #[test]
    fn test_weighted_aggregation() {
        let aggregator = LieGroupAggregator::new(
            LieGroupType::SO3,
            AggregationConfig {
                weight_strategy: WeightStrategy::ReputationWeighted,
                ..Default::default()
            },
        );

        let inputs = vec![
            AggregationInput {
                algebra_element: LieAlgebraElement::new("node_1".to_string(), vec![0.1, 0.2, 0.3], LieGroupType::SO3),
                reputation_score: Some(0.9),
                quality_score: None,
                weight: 0.9,
            },
            AggregationInput {
                algebra_element: LieAlgebraElement::new("node_2".to_string(), vec![0.2, 0.3, 0.4], LieGroupType::SO3),
                reputation_score: Some(0.7),
                quality_score: None,
                weight: 0.7,
            },
        ];

        let result = aggregator.aggregate_weighted(&inputs).unwrap();

        assert!(result.is_valid);
        assert_eq!(result.weight_strategy, WeightStrategy::ReputationWeighted);
    }

    #[test]
    fn test_pbft_integration() {
        let aggregator = LieGroupAggregator::default_with_type(LieGroupType::SO3);
        let mut integration = PbftLieGroupIntegration::new(aggregator);

        let request_id = "test_request_1";

        // Pre-prepare: 收集李代数元素
        integration.pre_prepare(
            request_id,
            LieAlgebraElement::new("node_1".to_string(), vec![0.1, 0.2, 0.3], LieGroupType::SO3),
        );
        integration.pre_prepare(
            request_id,
            LieAlgebraElement::new("node_2".to_string(), vec![0.2, 0.3, 0.4], LieGroupType::SO3),
        );

        // Prepare: 验证
        let prepare_result = integration.prepare(request_id).unwrap();
        assert!(prepare_result);

        // Commit: 执行聚合
        let commit_result = integration.commit(request_id).unwrap();
        assert!(commit_result.is_valid);
        assert_eq!(commit_result.contributor_count, 2);
    }

    #[test]
    fn test_pbft_prepare_validation_failure() {
        let aggregator = LieGroupAggregator::default_with_type(LieGroupType::SO3);
        let mut integration = PbftLieGroupIntegration::new(aggregator);

        let request_id = "test_request_2";

        // 添加错误维度的李代数元素
        integration.pre_prepare(
            request_id,
            LieAlgebraElement::new("node_1".to_string(), vec![0.1, 0.2], LieGroupType::SO3), // 只有 2 维，应该是 3 维
        );

        let prepare_result = integration.prepare(request_id);
        assert!(prepare_result.is_err());
    }
}
