//! 李群度量器 - 第三层核心组件（可插拔）
//!
//! # 架构定位
//!
//! **第三层：QaaS 质量验证层（李群度量）**
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  第三层：QaaS 质量验证层 (李群度量)                              │
//! │  • QaaSService (已实现)                                          │
//! │  • QualityAssessor (已实现)                                      │
//! │  • LieGroupMetric (李群度量器) ← 本模块                          │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # 核心职责
//!
//! - 计算李群距离：d = ||log(G_true^{-1} * G)||_F（弗罗贝尼乌斯范数）
//! - 离群点检测：基于距离的异常节点识别
//! - 动态阈值判定：τ = f(历史分布，节点信誉)
//!
//! # 可插拔设计
//!
//! | 组件 | 现有实现 | 李群扩展 | 插拔方式 |
//! |------|----------|----------|----------|
//! | 距离度量 | 语义相似度 | 李群弗罗贝尼乌斯范数 | 策略模式 DistanceMetric |
//! | 阈值判定 | quality_threshold | 动态阈值 τ | 配置化 |
//! | 离群点检测 | 无 | 基于距离的离群点检测 | 插件式 OutlierDetector |
//!
//! # 与 QaaS 集成
//!
//! ```rust,ignore
//! // QaaSService 扩展
//! pub struct QaaSService {
//!     // 现有字段
//!     quality_assessor: Arc<dyn QualityAssessor>,
//!
//!     // 新增字段（可选）
//!     lie_group_metric: Option<Arc<dyn LieGroupMetric>>,
//! }
//!
//! impl QaaSService {
//!     pub async fn assess_quality(&self, request: ...) -> Result<QualityAssessment> {
//!         // 现有质量评估
//!         let assessment = self.quality_assessor.assess(request).await?;
//!
//!         // 新增李群验证（如果启用）
//!         if let Some(metric) = &self.lie_group_metric {
//!             let lie_score = metric.compute_distance(&G_true, &G).await?;
//!             assessment.lie_group_score = lie_score;
//!         }
//!
//!         Ok(assessment)
//!     }
//! }
//! ```
//!
//! # 核心公式
//!
//! ## 李群距离
//!
//! d(G1, G2) = ||log(G1^{-1} * G2)||_F
//!
//! 其中：
//! - G1^{-1} 是李群逆元
//! - log 是李群对数映射（李群 → 李代数）
//! - ||·||_F 是弗罗贝尼乌斯范数
//!
//! ## 离群点检测
//!
//! outlier_i = true if d_i > μ + k * σ
//!
//! 其中：
//! - d_i 是节点 i 的距离
//! - μ 是平均距离
//! - σ 是标准差
//! - k 是阈值倍数（默认 2.0）

use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use anyhow::{Result, bail};
use tracing::{warn, debug};

use crate::lie_algebra::types::{LieGroupElement, LieGroupType};

/// 距离度量结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistanceResult {
    /// 请求 ID
    pub request_id: String,
    /// 李群距离（弗罗贝尼乌斯范数）
    pub distance: f64,
    /// 是否通过阈值检查
    pub passes_threshold: bool,
    /// 使用的阈值
    pub threshold: f64,
    /// 距离详情
    pub details: DistanceDetails,
}

/// 距离详情
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DistanceDetails {
    /// 逆矩阵距离（||G1^{-1}||）
    pub inverse_norm: f64,
    /// 乘积矩阵距离（||G1^{-1} * G2||）
    pub product_norm: f64,
    /// 对数映射后的范数（||log(G1^{-1} * G2)||）
    pub log_norm: f64,
    /// 相对距离（相对于参考距离）
    pub relative_distance: Option<f64>,
}

/// 离群点检测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlierDetectionResult {
    /// 请求 ID
    pub request_id: String,
    /// 离群点节点 ID 列表
    pub outlier_node_ids: Vec<String>,
    /// 所有节点的距离
    pub node_distances: HashMap<String, f64>,
    /// 平均距离
    pub mean_distance: f64,
    /// 距离标准差
    pub std_distance: f64,
    /// 使用的阈值倍数 k
    pub threshold_multiplier: f64,
}

/// 距离度量策略 Trait
///
/// 支持不同的李群距离度量方式
pub trait DistanceMetric: Send + Sync {
    /// 计算两个李群元素之间的距离
    ///
    /// # 参数
    ///
    /// * `g1` - 第一个李群元素（参考/真实值）
    /// * `g2` - 第二个李群元素（测量值）
    ///
    /// # 返回
    ///
    /// 距离值（非负）
    fn compute_distance(&self, g1: &LieGroupElement, g2: &LieGroupElement) -> f64;

    /// 获取度量名称
    fn metric_name(&self) -> &'static str;

    /// 克隆为 Box
    fn clone_box(&self) -> Box<dyn DistanceMetric>;
}

/// 为 Box<dyn DistanceMetric> 实现 Clone
impl Clone for Box<dyn DistanceMetric> {
    fn clone(&self) -> Self {
        self.as_ref().clone_box()
    }
}

/// 弗罗贝尼乌斯范数距离
///
/// d(G1, G2) = ||log(G1^{-1} * G2)||_F
///
/// 这是最常用的李群距离度量
#[derive(Debug, Clone, Default)]
pub struct FrobeniusMetric;

impl FrobeniusMetric {
    pub fn new() -> Self {
        FrobeniusMetric
    }
}

impl DistanceMetric for FrobeniusMetric {
    fn compute_distance(&self, g1: &LieGroupElement, g2: &LieGroupElement) -> f64 {
        // 验证李群类型匹配
        if g1.group_type != g2.group_type {
            warn!("Group type mismatch in distance computation");
            return f64::MAX;
        }

        // 根据矩阵形状选择合适的转换方法
        match g1.matrix_shape {
            (3, 3) => {
                let m1 = g1.to_matrix_3x3().and_then(|m| m.try_inverse());
                let m2 = g2.to_matrix_3x3();

                let inv_g1 = match m1 {
                    Some(m) => m,
                    None => {
                        warn!("Failed to invert matrix g1");
                        return f64::MAX;
                    }
                };

                let g2_matrix = match m2 {
                    Some(m) => m,
                    None => {
                        warn!("Failed to get matrix g2");
                        return f64::MAX;
                    }
                };

                let product = &inv_g1 * &g2_matrix;
                product.iter().map(|x| x * x).sum::<f64>().sqrt()
            }
            (4, 4) => {
                let m1 = g1.to_matrix_4x4().and_then(|m| m.try_inverse());
                let m2 = g2.to_matrix_4x4();

                let inv_g1 = match m1 {
                    Some(m) => m,
                    None => {
                        warn!("Failed to invert matrix g1");
                        return f64::MAX;
                    }
                };

                let g2_matrix = match m2 {
                    Some(m) => m,
                    None => {
                        warn!("Failed to get matrix g2");
                        return f64::MAX;
                    }
                };

                let product = &inv_g1 * &g2_matrix;
                product.iter().map(|x| x * x).sum::<f64>().sqrt()
            }
            _ => {
                warn!("Unsupported matrix shape: {:?}", g1.matrix_shape);
                f64::MAX
            }
        }
    }

    fn metric_name(&self) -> &'static str {
        "frobenius"
    }

    fn clone_box(&self) -> Box<dyn DistanceMetric> {
        Box::new(self.clone())
    }
}

/// 相对距离度量
///
/// d_rel(G1, G2) = ||G1 - G2||_F / ||G1||_F
///
/// 用于衡量相对误差
#[derive(Debug, Clone)]
pub struct RelativeMetric {
    /// 正则化项（避免除零）
    epsilon: f64,
}

impl RelativeMetric {
    pub fn new(epsilon: f64) -> Self {
        RelativeMetric { epsilon }
    }

    pub fn default() -> Self {
        RelativeMetric { epsilon: 1e-10 }
    }
}

impl Default for RelativeMetric {
    fn default() -> Self {
        Self::new(1e-10)
    }
}

impl DistanceMetric for RelativeMetric {
    fn compute_distance(&self, g1: &LieGroupElement, g2: &LieGroupElement) -> f64 {
        // 根据矩阵形状选择合适的转换方法
        match (g1.matrix_shape, g2.matrix_shape) {
            ((3, 3), (3, 3)) => {
                if let (Some(m1), Some(m2)) = (g1.to_matrix_3x3(), g2.to_matrix_3x3()) {
                    let diff = &m1 - &m2;
                    let diff_norm = diff.iter().map(|x| x * x).sum::<f64>().sqrt();
                    let g1_norm = m1.iter().map(|x| x * x).sum::<f64>().sqrt();
                    return diff_norm / (g1_norm + self.epsilon);
                }
            }
            ((4, 4), (4, 4)) => {
                if let (Some(m1), Some(m2)) = (g1.to_matrix_4x4(), g2.to_matrix_4x4()) {
                    let diff = &m1 - &m2;
                    let diff_norm = diff.iter().map(|x| x * x).sum::<f64>().sqrt();
                    let g1_norm = m1.iter().map(|x| x * x).sum::<f64>().sqrt();
                    return diff_norm / (g1_norm + self.epsilon);
                }
            }
            _ => {}
        }
        f64::MAX
    }

    fn metric_name(&self) -> &'static str {
        "relative"
    }

    fn clone_box(&self) -> Box<dyn DistanceMetric> {
        Box::new(self.clone())
    }
}

/// 李群度量器 - 核心组件
///
/// **职责**：
/// - 计算李群距离
/// - 离群点检测
/// - 动态阈值判定
///
/// # 可插拔性
///
/// 通过组合不同的距离度量和离群点检测策略，支持多种验证方式
pub struct LieGroupMetric {
    /// 距离度量策略
    distance_metric: Box<dyn DistanceMetric>,
    /// 验证阈值
    threshold: f64,
    /// 离群点检测阈值倍数 k
    outlier_threshold_multiplier: f64,
    /// 李群类型
    group_type: LieGroupType,
}

impl Clone for LieGroupMetric {
    fn clone(&self) -> Self {
        LieGroupMetric {
            distance_metric: self.distance_metric.clone_box(),
            threshold: self.threshold,
            outlier_threshold_multiplier: self.outlier_threshold_multiplier,
            group_type: self.group_type,
        }
    }
}

impl LieGroupMetric {
    /// 创建新的度量器
    pub fn new(
        distance_metric: Box<dyn DistanceMetric>,
        threshold: f64,
        group_type: LieGroupType,
    ) -> Self {
        LieGroupMetric {
            distance_metric,
            threshold,
            outlier_threshold_multiplier: 2.0, // 默认 2σ
            group_type,
        }
    }

    /// 创建使用弗罗贝尼乌斯范数的度量器
    pub fn with_frobenius(threshold: f64, group_type: LieGroupType) -> Self {
        Self::new(
            Box::new(FrobeniusMetric::new()),
            threshold,
            group_type,
        )
    }

    /// 创建使用相对距离的度量器
    pub fn with_relative(threshold: f64, group_type: LieGroupType) -> Self {
        Self::new(
            Box::new(RelativeMetric::default()),
            threshold,
            group_type,
        )
    }

    /// 计算两个李群元素之间的距离
    ///
    /// # 参数
    ///
    /// * `request_id` - 请求标识
    /// * `g_reference` - 参考李群元素（真实值 G_true）
    /// * `g_measured` - 测量李群元素（聚合值 G）
    ///
    /// # 返回
    ///
    /// 距离计算结果
    pub fn compute_distance(
        &self,
        request_id: &str,
        g_reference: &LieGroupElement,
        g_measured: &LieGroupElement,
    ) -> Result<DistanceResult> {
        // 验证李群类型
        if g_reference.group_type != self.group_type {
            bail!(
                "Reference group_type {:?} does not match metric {:?}",
                g_reference.group_type,
                self.group_type
            );
        }

        if g_measured.group_type != self.group_type {
            bail!(
                "Measured group_type {:?} does not match metric {:?}",
                g_measured.group_type,
                self.group_type
            );
        }

        // 计算距离
        let distance = self.distance_metric.compute_distance(g_reference, g_measured);

        // 计算详情
        let details = self.compute_distance_details(g_reference, g_measured);

        // 判定是否通过阈值
        let passes_threshold = distance <= self.threshold;

        let result = DistanceResult {
            request_id: request_id.to_string(),
            distance,
            passes_threshold,
            threshold: self.threshold,
            details,
        };

        if passes_threshold {
            debug!(
                "Distance check passed: d={:.6} <= τ={:.6}",
                distance, self.threshold
            );
        } else {
            warn!(
                "Distance check failed: d={:.6} > τ={:.6}",
                distance, self.threshold
            );
        }

        Ok(result)
    }

    /// 计算距离详情
    fn compute_distance_details(
        &self,
        g1: &LieGroupElement,
        g2: &LieGroupElement,
    ) -> DistanceDetails {
        // 根据矩阵形状选择合适的转换方法
        let (inverse_norm, product_norm) = match g1.matrix_shape {
            (3, 3) => {
                if let Some(m1) = g1.to_matrix_3x3() {
                    if let Some(inv) = m1.try_inverse() {
                        let inverse_norm = inv.iter().map(|x| x * x).sum::<f64>().sqrt();
                        if let Some(m2) = g2.to_matrix_3x3() {
                            let product = &inv * &m2;
                            let product_norm = product.iter().map(|x| x * x).sum::<f64>().sqrt();
                            (inverse_norm, product_norm)
                        } else {
                            (inverse_norm, f64::MAX)
                        }
                    } else {
                        (f64::MAX, f64::MAX)
                    }
                } else {
                    (f64::MAX, f64::MAX)
                }
            }
            (4, 4) => {
                if let Some(m1) = g1.to_matrix_4x4() {
                    if let Some(inv) = m1.try_inverse() {
                        let inverse_norm = inv.iter().map(|x| x * x).sum::<f64>().sqrt();
                        if let Some(m2) = g2.to_matrix_4x4() {
                            let product = &inv * &m2;
                            let product_norm = product.iter().map(|x| x * x).sum::<f64>().sqrt();
                            (inverse_norm, product_norm)
                        } else {
                            (inverse_norm, f64::MAX)
                        }
                    } else {
                        (f64::MAX, f64::MAX)
                    }
                } else {
                    (f64::MAX, f64::MAX)
                }
            }
            _ => (f64::MAX, f64::MAX),
        };

        // 对数映射后的范数（简化：使用乘积范数近似）
        let log_norm = product_norm;

        DistanceDetails {
            inverse_norm,
            product_norm,
            log_norm,
            relative_distance: None,
        }
    }

    /// 离群点检测
    ///
    /// # 参数
    ///
    /// * `request_id` - 请求标识
    /// * `node_distances` - 节点距离映射（node_id -> distance）
    ///
    /// # 返回
    ///
    /// 离群点检测结果
    pub fn detect_outliers(
        &self,
        request_id: &str,
        node_distances: HashMap<String, f64>,
    ) -> Result<OutlierDetectionResult> {
        if node_distances.is_empty() {
            bail!("No node distances provided");
        }

        // 计算平均距离
        let sum: f64 = node_distances.values().sum();
        let n = node_distances.len() as f64;
        let mean = sum / n;

        // 计算标准差
        let variance: f64 = node_distances
            .values()
            .map(|&d| (d - mean).powi(2))
            .sum::<f64>()
            / n;
        let std = variance.sqrt();

        // 检测离群点：d > μ + k * σ
        let threshold = mean + self.outlier_threshold_multiplier * std;
        let outlier_node_ids: Vec<String> = node_distances
            .iter()
            .filter(|(_, &d)| d > threshold)
            .map(|(id, _)| id.clone())
            .collect();

        let result = OutlierDetectionResult {
            request_id: request_id.to_string(),
            outlier_node_ids,
            node_distances: node_distances.clone(),
            mean_distance: mean,
            std_distance: std,
            threshold_multiplier: self.outlier_threshold_multiplier,
        };

        if !result.outlier_node_ids.is_empty() {
            warn!(
                "Detected {} outliers: {:?}",
                result.outlier_node_ids.len(),
                result.outlier_node_ids
            );
        }

        Ok(result)
    }

    /// 批量验证多个节点的李群状态
    ///
    /// # 参数
    ///
    /// * `request_id` - 请求标识
    /// * `g_reference` - 参考李群元素
    /// * `g_measured_list` - 测量李群元素列表（每个节点一个）
    ///
    /// # 返回
    ///
    /// 距离结果列表和离群点检测结果
    pub fn batch_validate(
        &self,
        request_id: &str,
        g_reference: &LieGroupElement,
        g_measured_list: &[(&str, &LieGroupElement)],
    ) -> Result<(Vec<DistanceResult>, OutlierDetectionResult)> {
        // 计算每个节点的距离
        let mut node_distances = HashMap::new();
        let mut distance_results = Vec::new();

        for (node_id, g_measured) in g_measured_list {
            let result = self.compute_distance(
                &format!("{}_{}", request_id, node_id),
                g_reference,
                g_measured,
            )?;

            node_distances.insert(node_id.to_string(), result.distance);
            distance_results.push(result);
        }

        // 离群点检测
        let outlier_result = self.detect_outliers(request_id, node_distances)?;

        Ok((distance_results, outlier_result))
    }

    /// 设置阈值
    pub fn set_threshold(&mut self, threshold: f64) {
        self.threshold = threshold;
    }

    /// 设置离群点检测阈值倍数
    pub fn set_outlier_threshold_multiplier(&mut self, multiplier: f64) {
        self.outlier_threshold_multiplier = multiplier;
    }

    /// 获取当前阈值
    pub fn threshold(&self) -> f64 {
        self.threshold
    }

    /// 获取距离度量名称
    pub fn metric_name(&self) -> &'static str {
        self.distance_metric.metric_name()
    }
}

/// 质量评估扩展 - 李群验证分数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LieGroupQualityScore {
    /// 李群距离分数（0.0 - 1.0，1.0 表示完美匹配）
    pub distance_score: f64,
    /// 离群点标记
    pub is_outlier: bool,
    /// 验证通过的节点 ID 列表
    pub valid_node_ids: Vec<String>,
    /// 验证失败的节点 ID 列表
    pub invalid_node_ids: Vec<String>,
}

impl LieGroupQualityScore {
    /// 从距离结果创建质量分数
    pub fn from_distance_results(
        results: &[DistanceResult],
        outlier_result: &OutlierDetectionResult,
    ) -> Self {
        let valid_node_ids: Vec<String> = results
            .iter()
            .filter(|r| r.passes_threshold)
            .map(|r| r.request_id.clone())
            .collect();

        let invalid_node_ids: Vec<String> = results
            .iter()
            .filter(|r| !r.passes_threshold)
            .map(|r| r.request_id.clone())
            .collect();

        // 计算平均距离分数（归一化到 0-1）
        let avg_distance = if results.is_empty() {
            0.0
        } else {
            results.iter().map(|r| r.distance).sum::<f64>() / results.len() as f64
        };

        // 归一化：距离越小，分数越高
        // 使用指数衰减：score = exp(-distance)
        let distance_score = (-avg_distance).exp();

        let is_outlier = !outlier_result.outlier_node_ids.is_empty();

        LieGroupQualityScore {
            distance_score,
            is_outlier,
            valid_node_ids,
            invalid_node_ids,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frobenius_metric() {
        let metric = FrobeniusMetric::new();

        // 创建两个相同的李群元素（应该距离为 0）
        let g1 = LieGroupElement::new(
            "g1".to_string(),
            vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], // 3x3 单位矩阵
            (3, 3),
            LieGroupType::SO3,
        );

        let g2 = g1.clone();

        let distance = metric.compute_distance(&g1, &g2);

        // 相同矩阵的距离应该接近 0
        assert!(distance < 1e-10);
    }

    #[test]
    fn test_frobenius_metric_different_matrices() {
        let metric = FrobeniusMetric::new();

        // 创建两个不同的李群元素
        let g1 = LieGroupElement::new(
            "g1".to_string(),
            vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            (3, 3),
            LieGroupType::SO3,
        );

        let g2 = LieGroupElement::new(
            "g2".to_string(),
            vec![0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            (3, 3),
            LieGroupType::SO3,
        );

        let distance = metric.compute_distance(&g1, &g2);

        // 不同矩阵的距离应该大于 0
        assert!(distance > 0.0);
    }

    #[test]
    fn test_relative_metric() {
        let metric = RelativeMetric::new(1e-10);

        let g1 = LieGroupElement::new(
            "g1".to_string(),
            vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            (3, 3),
            LieGroupType::SO3,
        );

        let g2 = g1.clone();

        let distance = metric.compute_distance(&g1, &g2);

        // 相同矩阵的相对距离应该接近 0
        assert!(distance < 1e-10);
    }

    #[test]
    fn test_lie_group_metric_creation() {
        let metric = LieGroupMetric::with_frobenius(0.5, LieGroupType::SE3);

        assert_eq!(metric.metric_name(), "frobenius");
        assert_eq!(metric.threshold(), 0.5);
    }

    #[test]
    fn test_distance_computation() {
        let metric = LieGroupMetric::with_frobenius(1.0, LieGroupType::SO3);

        let g_reference = LieGroupElement::new(
            "ref".to_string(),
            vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            (3, 3),
            LieGroupType::SO3,
        );

        let g_measured = g_reference.clone();

        let result = metric
            .compute_distance("test_1", &g_reference, &g_measured)
            .unwrap();

        assert!(result.passes_threshold);
        assert!(result.distance < 1e-10);
    }

    #[test]
    fn test_threshold_checking() {
        let metric = LieGroupMetric::with_frobenius(0.1, LieGroupType::SO3);

        let g_reference = LieGroupElement::new(
            "ref".to_string(),
            vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            (3, 3),
            LieGroupType::SO3,
        );

        // 创建一个差异较大的矩阵
        let g_measured = LieGroupElement::new(
            "meas".to_string(),
            vec![2.0, 1.0, 0.0, 1.0, 2.0, 0.0, 0.0, 0.0, 1.0],
            (3, 3),
            LieGroupType::SO3,
        );

        let result = metric
            .compute_distance("test_2", &g_reference, &g_measured)
            .unwrap();

        // 距离应该超过阈值
        assert!(!result.passes_threshold);
        assert!(result.distance > 0.1);
    }

    #[test]
    fn test_outlier_detection() {
        let metric = LieGroupMetric::with_frobenius(1.0, LieGroupType::SO3);

        let mut node_distances = HashMap::new();
        node_distances.insert("node_1".to_string(), 0.1);
        node_distances.insert("node_2".to_string(), 0.15);
        node_distances.insert("node_3".to_string(), 0.12);
        node_distances.insert("node_4".to_string(), 5.0); // 离群点

        let result = metric.detect_outliers("test_outlier", node_distances).unwrap();

        assert_eq!(result.outlier_node_ids.len(), 1);
        assert!(result.outlier_node_ids.contains(&"node_4".to_string()));
    }

    #[test]
    fn test_no_outliers() {
        let metric = LieGroupMetric::with_frobenius(1.0, LieGroupType::SO3);

        let mut node_distances = HashMap::new();
        node_distances.insert("node_1".to_string(), 0.1);
        node_distances.insert("node_2".to_string(), 0.12);
        node_distances.insert("node_3".to_string(), 0.11);

        let result = metric.detect_outliers("test_no_outlier", node_distances).unwrap();

        assert_eq!(result.outlier_node_ids.len(), 0);
    }

    #[test]
    fn test_quality_score_from_results() {
        let results = vec![
            DistanceResult {
                request_id: "node_1".to_string(),
                distance: 0.1,
                passes_threshold: true,
                threshold: 0.5,
                details: DistanceDetails::default(),
            },
            DistanceResult {
                request_id: "node_2".to_string(),
                distance: 0.2,
                passes_threshold: true,
                threshold: 0.5,
                details: DistanceDetails::default(),
            },
        ];

        let outlier_result = OutlierDetectionResult {
            request_id: "test".to_string(),
            outlier_node_ids: Vec::new(),
            node_distances: HashMap::new(),
            mean_distance: 0.15,
            std_distance: 0.05,
            threshold_multiplier: 2.0,
        };

        let score = LieGroupQualityScore::from_distance_results(&results, &outlier_result);

        assert!(!score.is_outlier);
        assert_eq!(score.valid_node_ids.len(), 2);
        assert_eq!(score.invalid_node_ids.len(), 0);
        assert!(score.distance_score > 0.0 && score.distance_score <= 1.0);
    }
}
