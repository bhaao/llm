//! 李代数映射器 - 第一层核心组件（可插拔）
//!
//! # 架构定位
//!
//! **第一层：分布式上下文分片层**
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  第一层：分布式上下文分片层 (不可信节点)                         │
//! │  • ContextShardManager (已实现)                                  │
//! │  • ProviderLayerManager (已实现)                                 │
//! │  • LieAlgebraMapper (李代数映射器) ← 本模块                      │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # 核心职责
//!
//! - 将局部特征 h_i（推理隐层状态）映射为李代数元素 A_i
//! - 支持多种映射策略（指数映射、对数映射、线性映射）
//! - 支持不同李群类型（SO(3)、SE(3)、GL(n)）
//! - 通过配置文件选择映射器
//!
//! # 可插拔设计
//!
//! | 组件 | 现有实现 | 李群扩展 | 插拔方式 |
//! |------|----------|----------|----------|
//! | 特征提取器 | InferenceResponse.completion | 提取隐层状态 | Trait 抽象 FeatureExtractor |
//! | 映射函数 | 无 | to_algebra(h_i) | 策略模式 MappingStrategy |
//! | 提交协议 | KvCacheProof | LieAlgebraCommitment | 扩展数据结构 |
//!
//! # 使用示例
//!
//! ```ignore
//! use block_chain_with_context::lie_algebra::{
//!     LieAlgebraMapper, MappingStrategy, LieGroupType,
//!     ExponentialMapping, LinearMapping, LogarithmicMapping,
//! };
//!
//! // 创建映射器（使用指数映射策略）
//! let mapper = LieAlgebraMapper::new(
//!     MappingStrategy::Exponential(Box::new(ExponentialMapping::default())),
//!     LieGroupType::SE3,
//! );
//!
//! // 提取局部特征（从推理响应中）
//! let features = extract_features_from_response(&response);
//!
//! // 映射为李代数元素
//! let algebra_element = mapper.to_algebra("request_1", &features);
//!
//! // 生成提交承诺
//! let commitment = mapper.commit(&algebra_element);
//! ```

use serde::{Serialize, Deserialize};
use crate::lie_algebra::types::{LieAlgebraElement, LieGroupType, LieGroupConfig};

/// 特征提取器 Trait
///
/// 用于从推理响应中提取局部特征向量
///
/// # 可扩展性
///
/// 支持不同的特征提取策略：
/// - 直接从 completion 文本提取
/// - 从隐层状态提取
/// - 从注意力权重提取
pub trait FeatureExtractor: Send + Sync {
    /// 从原始数据提取特征向量
    ///
    /// # 参数
    ///
    /// * `raw_data` - 原始数据（可以是文本、JSON 等）
    ///
    /// # 返回
    ///
    /// 特征向量（f32 数组）
    fn extract_features(&self, raw_data: &[u8]) -> Vec<f32>;

    /// 获取特征维度
    fn feature_dimension(&self) -> usize;

    /// 克隆为 Box
    fn clone_box(&self) -> Box<dyn FeatureExtractor>;
}

/// 为 Box<dyn FeatureExtractor> 实现 Clone
impl Clone for Box<dyn FeatureExtractor> {
    fn clone(&self) -> Self {
        self.as_ref().clone_box()
    }
}

/// 简单特征提取器（基于文本哈希）
///
/// 简化实现：将文本哈希映射为固定维度的特征向量
pub struct SimpleFeatureExtractor {
    dimension: usize,
}

impl SimpleFeatureExtractor {
    pub fn new(dimension: usize) -> Self {
        SimpleFeatureExtractor { dimension }
    }
}

impl FeatureExtractor for SimpleFeatureExtractor {
    fn extract_features(&self, raw_data: &[u8]) -> Vec<f32> {
        use sha2::{Sha256, Digest};
        
        // 计算 SHA256 哈希
        let hash = Sha256::digest(raw_data);
        
        // 将哈希字节映射为 f32 特征
        let mut features = Vec::with_capacity(self.dimension);
        for i in 0..self.dimension {
            let byte_idx = i % hash.len();
            let feature = (hash[byte_idx] as f32) / 255.0;
            features.push(feature);
        }
        
        features
    }

    fn feature_dimension(&self) -> usize {
        self.dimension
    }

    fn clone_box(&self) -> Box<dyn FeatureExtractor> {
        Box::new(SimpleFeatureExtractor { dimension: self.dimension })
    }
}

/// 映射策略 Trait
///
/// 定义特征向量到李代数元素的映射方式
///
/// # 数学背景
///
/// 映射策略决定了局部特征如何编码到李代数空间：
/// - **线性映射**：直接投影，保持线性结构
/// - **指数映射**：通过指数函数编码非线性关系
/// - **对数映射**：通过对数函数压缩动态范围
pub trait MappingStrategy: Send + Sync {
    /// 将特征向量映射为李代数数据
    ///
    /// # 参数
    ///
    /// * `features` - 输入特征向量
    /// * `group_type` - 目标李群类型
    ///
    /// # 返回
    ///
    /// 李代数数据向量
    fn map_to_algebra(&self, features: &[f32], group_type: LieGroupType) -> Vec<f64>;

    /// 获取映射策略名称
    fn strategy_name(&self) -> &'static str;

    /// 克隆为 Box
    fn clone_box(&self) -> Box<dyn MappingStrategy>;
}

/// 为 Box<dyn MappingStrategy> 实现 Clone
impl Clone for Box<dyn MappingStrategy> {
    fn clone(&self) -> Self {
        self.as_ref().clone_box()
    }
}

/// 线性映射策略
///
/// 最简单的映射：直接将特征缩放到李代数空间
///
/// # 公式
///
/// A_i = scale * h_i + bias
///
/// # 适用场景
///
/// - 特征已经归一化到 [0, 1] 区间
/// - 需要保持特征的线性关系
#[derive(Debug, Clone)]
pub struct LinearMapping {
    scale: f64,
    bias: f64,
}

impl LinearMapping {
    pub fn new(scale: f64, bias: f64) -> Self {
        LinearMapping { scale, bias }
    }

    pub fn default() -> Self {
        LinearMapping {
            scale: 1.0,
            bias: 0.0,
        }
    }
}

impl Default for LinearMapping {
    fn default() -> Self {
        Self::new(1.0, 0.0)
    }
}

impl MappingStrategy for LinearMapping {
    fn map_to_algebra(&self, features: &[f32], _group_type: LieGroupType) -> Vec<f64> {
        features
            .iter()
            .map(|&f| self.scale * (f as f64) + self.bias)
            .collect()
    }

    fn strategy_name(&self) -> &'static str {
        "linear"
    }

    fn clone_box(&self) -> Box<dyn MappingStrategy> {
        Box::new(self.clone())
    }
}

/// 指数映射策略
///
/// 通过指数函数编码非线性关系
///
/// # 公式
///
/// A_i = exp(scale * h_i) - 1
///
/// # 适用场景
///
/// - 需要放大特征差异
/// - 特征值较小，需要非线性增强
#[derive(Debug, Clone)]
pub struct ExponentialMapping {
    scale: f64,
}

impl ExponentialMapping {
    pub fn new(scale: f64) -> Self {
        ExponentialMapping { scale }
    }

    pub fn default() -> Self {
        ExponentialMapping { scale: 0.5 }
    }
}

impl Default for ExponentialMapping {
    fn default() -> Self {
        Self::new(0.5)
    }
}

impl MappingStrategy for ExponentialMapping {
    fn map_to_algebra(&self, features: &[f32], _group_type: LieGroupType) -> Vec<f64> {
        features
            .iter()
            .map(|&f| (self.scale * (f as f64)).exp() - 1.0)
            .collect()
    }

    fn strategy_name(&self) -> &'static str {
        "exponential"
    }

    fn clone_box(&self) -> Box<dyn MappingStrategy> {
        Box::new(self.clone())
    }
}

/// 对数映射策略
///
/// 通过对数函数压缩动态范围
///
/// # 公式
///
/// A_i = log(1 + scale * |h_i|) * sign(h_i)
///
/// # 适用场景
///
/// - 特征动态范围大，需要压缩
/// - 需要保持符号信息
#[derive(Debug, Clone)]
pub struct LogarithmicMapping {
    scale: f64,
}

impl LogarithmicMapping {
    pub fn new(scale: f64) -> Self {
        LogarithmicMapping { scale }
    }

    pub fn default() -> Self {
        LogarithmicMapping { scale: 10.0 }
    }
}

impl Default for LogarithmicMapping {
    fn default() -> Self {
        Self::new(10.0)
    }
}

impl MappingStrategy for LogarithmicMapping {
    fn map_to_algebra(&self, features: &[f32], _group_type: LieGroupType) -> Vec<f64> {
        features
            .iter()
            .map(|&f| {
                let abs_f = (f as f64).abs();
                let sign = if f >= 0.0 { 1.0 } else { -1.0 };
                (1.0 + self.scale * abs_f).ln() * sign
            })
            .collect()
    }

    fn strategy_name(&self) -> &'static str {
        "logarithmic"
    }

    fn clone_box(&self) -> Box<dyn MappingStrategy> {
        Box::new(self.clone())
    }
}

/// 李代数映射器 - 核心组件
///
/// **职责**：
/// - 使用特征提取器从原始数据提取特征
/// - 使用映射策略将特征映射为李代数元素
/// - 生成提交承诺（哈希）
///
/// # 可插拔性
///
/// 通过组合不同的特征提取器和映射策略，支持多种映射方式：
/// - 特征提取器：SimpleFeatureExtractor, 自定义提取器
/// - 映射策略：LinearMapping, ExponentialMapping, LogarithmicMapping
pub struct LieAlgebraMapper {
    /// 特征提取器
    feature_extractor: Box<dyn FeatureExtractor>,
    /// 映射策略
    mapping_strategy: Box<dyn MappingStrategy>,
    /// 李群类型
    group_type: LieGroupType,
    /// 配置
    config: LieGroupConfig,
}

impl Clone for LieAlgebraMapper {
    fn clone(&self) -> Self {
        LieAlgebraMapper {
            feature_extractor: self.feature_extractor.clone_box(),
            mapping_strategy: self.mapping_strategy.clone_box(),
            group_type: self.group_type,
            config: self.config.clone(),
        }
    }
}

impl LieAlgebraMapper {
    /// 创建新的映射器
    ///
    /// # 参数
    ///
    /// * `feature_extractor` - 特征提取器
    /// * `mapping_strategy` - 映射策略
    /// * `group_type` - 李群类型
    pub fn new(
        feature_extractor: Box<dyn FeatureExtractor>,
        mapping_strategy: Box<dyn MappingStrategy>,
        group_type: LieGroupType,
    ) -> Self {
        LieAlgebraMapper {
            feature_extractor,
            mapping_strategy,
            group_type,
            config: LieGroupConfig::default(),
        }
    }

    /// 创建使用线性映射的映射器
    pub fn with_linear_mapping(dimension: usize, group_type: LieGroupType) -> Self {
        Self::new(
            Box::new(SimpleFeatureExtractor::new(dimension)),
            Box::new(LinearMapping::default()),
            group_type,
        )
    }

    /// 创建使用指数映射的映射器
    pub fn with_exponential_mapping(dimension: usize, group_type: LieGroupType) -> Self {
        Self::new(
            Box::new(SimpleFeatureExtractor::new(dimension)),
            Box::new(ExponentialMapping::default()),
            group_type,
        )
    }

    /// 创建使用对数映射的映射器
    pub fn with_logarithmic_mapping(dimension: usize, group_type: LieGroupType) -> Self {
        Self::new(
            Box::new(SimpleFeatureExtractor::new(dimension)),
            Box::new(LogarithmicMapping::default()),
            group_type,
        )
    }

    /// 从原始数据创建李代数元素
    ///
    /// # 参数
    ///
    /// * `element_id` - 元素标识
    /// * `raw_data` - 原始数据（推理响应等）
    ///
    /// # 返回
    ///
    /// 李代数元素
    pub fn to_algebra(&self, element_id: &str, raw_data: &[u8]) -> LieAlgebraElement {
        // 1. 提取特征
        let features = self.feature_extractor.extract_features(raw_data);
        
        // 2. 映射为李代数数据
        let algebra_data = self.mapping_strategy.map_to_algebra(&features, self.group_type);
        
        // 3. 创建李代数元素
        LieAlgebraElement::new(
            element_id.to_string(),
            algebra_data,
            self.group_type,
        )
    }

    /// 直接从特征向量创建李代数元素
    ///
    /// # 参数
    ///
    /// * `element_id` - 元素标识
    /// * `features` - 特征向量
    ///
    /// # 返回
    ///
    /// 李代数元素
    pub fn to_algebra_from_features(&self, element_id: &str, features: &[f32]) -> LieAlgebraElement {
        let algebra_data = self.mapping_strategy.map_to_algebra(features, self.group_type);
        
        LieAlgebraElement::new(
            element_id.to_string(),
            algebra_data,
            self.group_type,
        )
    }

    /// 生成提交承诺（哈希）
    ///
    /// # 参数
    ///
    /// * `element` - 李代数元素
    ///
    /// # 返回
    ///
    /// 承诺哈希
    pub fn commit(&self, element: &LieAlgebraElement) -> String {
        element.hash()
    }

    /// 获取映射策略名称
    pub fn strategy_name(&self) -> &'static str {
        self.mapping_strategy.strategy_name()
    }

    /// 获取李群类型
    pub fn group_type(&self) -> LieGroupType {
        self.group_type
    }

    /// 获取特征维度
    pub fn feature_dimension(&self) -> usize {
        self.feature_extractor.feature_dimension()
    }

    /// 设置配置
    pub fn with_config(mut self, config: LieGroupConfig) -> Self {
        self.config = config;
        self
    }

    /// 获取配置
    pub fn config(&self) -> &LieGroupConfig {
        &self.config
    }
}

/// 李代数承诺 - 用于上链存证
///
/// 包含李代数元素的哈希和元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LieAlgebraCommitment {
    /// 承诺 ID
    pub commitment_id: String,
    /// 李代数元素哈希
    pub algebra_hash: String,
    /// 节点 ID
    pub node_id: String,
    /// 请求 ID
    pub request_id: String,
    /// 时间戳
    pub timestamp: u64,
    /// 节点签名
    pub node_signature: String,
}

impl LieAlgebraCommitment {
    /// 创建新的承诺
    pub fn new(
        algebra_hash: String,
        node_id: String,
        request_id: String,
    ) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        use sha2::{Sha256, Digest};

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // 生成承诺 ID
        let data = format!("{}:{}:{}:{}", algebra_hash, node_id, request_id, timestamp);
        let commitment_id = format!("commit_{:x}", Sha256::digest(data.as_bytes()));

        LieAlgebraCommitment {
            commitment_id,
            algebra_hash,
            node_id,
            request_id,
            timestamp,
            node_signature: String::new(),
        }
    }

    /// 对承诺进行签名
    pub fn sign(&mut self, private_key: &[u8; 32]) -> Result<(), String> {
        use ed25519_dalek::{SigningKey, Signer};

        let signing_key = SigningKey::from_bytes(private_key);
        let message = self.signing_message();
        let signature = signing_key.sign(message.as_bytes());
        self.node_signature = hex::encode(signature.to_bytes());
        Ok(())
    }

    /// 获取签名消息
    pub fn signing_message(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.commitment_id,
            self.algebra_hash,
            self.request_id,
            self.timestamp
        )
    }

    /// 验证签名
    pub fn verify_signature(&self, public_key: &[u8; 32]) -> bool {
        use ed25519_dalek::{VerifyingKey, Verifier};
        use ed25519_dalek::Signature;

        let verifying_key = VerifyingKey::from_bytes(public_key)
            .unwrap_or_else(|_| VerifyingKey::from_bytes(&[0u8; 32]).unwrap());

        let signature_bytes = match hex::decode(&self.node_signature) {
            Ok(bytes) => bytes,
            Err(_) => return false,
        };

        let signature = match Signature::try_from(&signature_bytes[..]) {
            Ok(sig) => sig,
            Err(_) => return false,
        };

        verifying_key.verify(self.signing_message().as_bytes(), &signature).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_feature_extractor() {
        let extractor = SimpleFeatureExtractor::new(6);
        let data = b"test inference response";
        
        let features = extractor.extract_features(data);
        
        assert_eq!(features.len(), 6);
        assert!(features.iter().all(|&f| f >= 0.0 && f <= 1.0));
    }

    #[test]
    fn test_linear_mapping() {
        let mapping = LinearMapping::new(2.0, 0.5);
        let features = vec![0.1f32, 0.5, 0.9];
        
        let algebra = mapping.map_to_algebra(&features, LieGroupType::SE3);
        
        assert_eq!(algebra.len(), 3);
        // 验证线性映射：2.0 * 0.1 + 0.5 = 0.7
        assert!((algebra[0] - 0.7).abs() < 1e-10);
    }

    #[test]
    fn test_exponential_mapping() {
        let mapping = ExponentialMapping::new(1.0);
        let features = vec![0.0f32, 1.0];
        
        let algebra = mapping.map_to_algebra(&features, LieGroupType::SE3);
        
        assert_eq!(algebra.len(), 2);
        // exp(0) - 1 = 0
        assert!((algebra[0] - 0.0).abs() < 1e-10);
        // exp(1) - 1 ≈ 1.718
        assert!((algebra[1] - (std::f64::consts::E - 1.0)).abs() < 1e-10);
    }

    #[test]
    fn test_logarithmic_mapping() {
        let mapping = LogarithmicMapping::new(1.0);
        let features = vec![0.0f32, 1.0, -1.0];
        
        let algebra = mapping.map_to_algebra(&features, LieGroupType::SE3);
        
        assert_eq!(algebra.len(), 3);
        // log(1 + 0) = 0
        assert!((algebra[0] - 0.0).abs() < 1e-10);
        // log(1 + 1) ≈ 0.693
        assert!((algebra[1] - 2.0f64.ln()).abs() < 1e-10);
        // -log(1 + 1) ≈ -0.693
        assert!((algebra[2] + 2.0f64.ln()).abs() < 1e-10);
    }

    #[test]
    fn test_lie_algebra_mapper_creation() {
        let mapper = LieAlgebraMapper::with_linear_mapping(6, LieGroupType::SE3);
        
        assert_eq!(mapper.feature_dimension(), 6);
        assert_eq!(mapper.strategy_name(), "linear");
        assert_eq!(mapper.group_type(), LieGroupType::SE3);
    }

    #[test]
    fn test_lie_algebra_mapper_to_algebra() {
        let mapper = LieAlgebraMapper::with_exponential_mapping(6, LieGroupType::SE3);
        let data = b"test inference data";
        
        let element = mapper.to_algebra("test_1", data);
        
        assert_eq!(element.id, "test_1");
        assert_eq!(element.data.len(), 6);
        assert_eq!(element.group_type, LieGroupType::SE3);
    }

    #[test]
    fn test_lie_algebra_commitment() {
        let algebra_hash = "abc123".to_string();
        let node_id = "node_1".to_string();
        let request_id = "req_1".to_string();
        
        let commitment = LieAlgebraCommitment::new(
            algebra_hash.clone(),
            node_id.clone(),
            request_id.clone(),
        );
        
        assert!(!commitment.commitment_id.is_empty());
        assert_eq!(commitment.algebra_hash, algebra_hash);
        assert_eq!(commitment.node_id, node_id);
        assert_eq!(commitment.request_id, request_id);
    }

    #[test]
    fn test_mapper_strategies() {
        // 测试三种映射策略都可以正常工作
        let linear = LieAlgebraMapper::with_linear_mapping(6, LieGroupType::SO3);
        let exponential = LieAlgebraMapper::with_exponential_mapping(6, LieGroupType::SO3);
        let logarithmic = LieAlgebraMapper::with_logarithmic_mapping(6, LieGroupType::SO3);
        
        let data = b"test data";
        
        let linear_elem = linear.to_algebra("linear", data);
        let exp_elem = exponential.to_algebra("exp", data);
        let log_elem = logarithmic.to_algebra("log", data);
        
        // 不同策略应产生不同结果
        assert_ne!(linear_elem.data, exp_elem.data);
        assert_ne!(linear_elem.data, log_elem.data);
        assert_ne!(exp_elem.data, log_elem.data);
    }
}
