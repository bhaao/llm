//! 可插拔评估器注册模块 - P1-3
//!
//! **设计目标**：
//! - 支持动态注册/注销质量评估器
//! - 支持多种评估策略并行/串行执行
//! - 支持评估器权重配置
//! - 支持评估器性能指标追踪
//!
//! **核心概念**：
//! - **Assessor** - 评估器 Trait，定义评估接口
//! - **AssessorRegistry** - 注册表，管理所有评估器
//! - **AssessmentStrategy** - 评估策略，决定如何组合多个评估器结果

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::{Result, Context};
use tracing::{info, warn, instrument};
use serde::{Serialize, Deserialize};

use crate::quality_assessment::{
    QualityAssessment, QualityAssessmentRequest, AssessmentDetails,
    SemanticCheckMode,
};

/// 评估器 Trait - 所有评估器必须实现
#[async_trait::async_trait]
pub trait Assessor: Send + Sync {
    /// 获取评估器 ID
    fn id(&self) -> &str;
    
    /// 获取评估器名称
    fn name(&self) -> &str;
    
    /// 获取评估器版本
    fn version(&self) -> &str;
    
    /// 获取评估器权重（0.0 - 1.0）
    fn weight(&self) -> f64;
    
    /// 执行评估
    async fn assess(&self, request: QualityAssessmentRequest) -> Result<QualityAssessment>;
    
    /// 是否启用
    fn is_enabled(&self) -> bool;
    
    /// 获取评估器元数据
    fn metadata(&self) -> AssessorMetadata {
        AssessorMetadata {
            id: self.id().to_string(),
            name: self.name().to_string(),
            version: self.version().to_string(),
            weight: self.weight(),
            is_enabled: self.is_enabled(),
            supported_modes: self.supported_modes(),
        }
    }
    
    /// 获取支持的语义检查模式
    fn supported_modes(&self) -> Vec<SemanticCheckMode>;
}

/// 评估器元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssessorMetadata {
    /// 评估器 ID
    pub id: String,
    /// 评估器名称
    pub name: String,
    /// 版本号
    pub version: String,
    /// 权重
    pub weight: f64,
    /// 是否启用
    pub is_enabled: bool,
    /// 支持的语义检查模式
    pub supported_modes: Vec<SemanticCheckMode>,
}

/// 评估器性能指标
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssessorStats {
    /// 总评估次数
    pub total_assessments: u64,
    /// 成功评估次数
    pub successful_assessments: u64,
    /// 失败评估次数
    pub failed_assessments: u64,
    /// 平均评估耗时（毫秒）
    pub avg_latency_ms: f64,
    /// 平均质量得分
    pub avg_quality_score: f64,
    /// 最后一次评估时间戳
    pub last_assessment_at: Option<u64>,
}

impl AssessorStats {
    /// 记录评估结果
    pub fn record_assessment(&mut self, success: bool, latency_ms: f64, quality_score: f64) {
        self.total_assessments += 1;
        
        if success {
            self.successful_assessments += 1;
        } else {
            self.failed_assessments += 1;
        }
        
        // 更新平均延迟
        let n = self.total_assessments as f64;
        self.avg_latency_ms = ((self.avg_latency_ms * (n - 1.0)) + latency_ms) / n;
        
        // 更新平均质量得分
        self.avg_quality_score = ((self.avg_quality_score * (n - 1.0)) + quality_score) / n;
        
        // 更新时间戳
        self.last_assessment_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );
    }
    
    /// 获取成功率
    pub fn success_rate(&self) -> f64 {
        if self.total_assessments == 0 {
            1.0
        } else {
            self.successful_assessments as f64 / self.total_assessments as f64
        }
    }
}

/// 注册表中的评估器条目
struct AssessorEntry {
    /// 评估器实例
    assessor: Box<dyn Assessor>,
    /// 性能统计
    stats: AssessorStats,
}

/// 评估策略 - 决定如何组合多个评估器结果
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum AssessmentStrategy {
    /// 加权平均（默认）
    /// 按照评估器权重计算加权平均
    #[default]
    WeightedAverage,
    /// 最低分优先
    /// 取所有评估结果中的最低分（保守策略）
    Minimum,
    /// 最高分优先
    /// 取所有评估结果中的最高分（乐观策略）
    Maximum,
    /// 中位数
    /// 取所有评估结果的中位数（抗异常值）
    Median,
    /// 投票制
    /// 超过半数评估器通过则认为通过
    Voting,
}

/// 可插拔评估器注册表
///
/// **功能**：
/// - 动态注册/注销评估器
/// - 并行执行多个评估器
/// - 根据策略组合评估结果
/// - 追踪评估器性能指标
pub struct AssessorRegistry {
    /// 注册的评估器
    assessors: Arc<RwLock<HashMap<String, AssessorEntry>>>,
    /// 评估策略
    strategy: Arc<RwLock<AssessmentStrategy>>,
    /// 质量阈值（低于此值认为不合格）
    quality_threshold: Arc<RwLock<f64>>,
    /// 是否启用并行评估
    enable_parallel: bool,
}

impl AssessorRegistry {
    /// 创建新的注册表
    pub fn new(enable_parallel: bool) -> Self {
        AssessorRegistry {
            assessors: Arc::new(RwLock::new(HashMap::new())),
            strategy: Arc::new(RwLock::new(AssessmentStrategy::default())),
            quality_threshold: Arc::new(RwLock::new(0.7)), // 默认 0.7 阈值
            enable_parallel,
        }
    }

    /// 创建默认注册表（串行评估）
    pub fn with_defaults() -> Self {
        Self::new(false)
    }

    /// 注册评估器
    pub async fn register(&self, assessor: Box<dyn Assessor>) -> Result<()> {
        let id = assessor.id().to_string();
        
        let mut assessors = self.assessors.write().await;
        
        if assessors.contains_key(&id) {
            warn!("Assessor {} already registered, updating", id);
        }
        
        assessors.insert(
            id.clone(),
            AssessorEntry {
                assessor,
                stats: AssessorStats::default(),
            },
        );
        
        info!("Registered assessor: {}", id);
        
        Ok(())
    }

    /// 注销评估器
    pub async fn unregister(&self, assessor_id: &str) -> Result<()> {
        let mut assessors = self.assessors.write().await;
        
        assessors.remove(assessor_id)
            .context(format!("Assessor {} not found", assessor_id))?;
        
        info!("Unregistered assessor: {}", assessor_id);
        
        Ok(())
    }

    /// 获取评估器列表
    pub async fn list_assessors(&self) -> Vec<AssessorMetadata> {
        let assessors = self.assessors.read().await;
        assessors.values().map(|e| e.assessor.metadata()).collect()
    }

    /// 获取评估器统计
    pub async fn get_assessor_stats(&self, assessor_id: &str) -> Result<AssessorStats> {
        let assessors = self.assessors.read().await;
        let entry = assessors.get(assessor_id)
            .context(format!("Assessor {} not found", assessor_id))?;
        
        Ok(entry.stats.clone())
    }

    /// 设置评估策略
    pub async fn set_strategy(&self, strategy: AssessmentStrategy) {
        let mut s = self.strategy.write().await;
        *s = strategy;
        
        info!("Set assessment strategy: {:?}", strategy);
    }

    /// 获取当前策略
    pub async fn get_strategy(&self) -> AssessmentStrategy {
        *self.strategy.read().await
    }

    /// 设置质量阈值
    pub async fn set_quality_threshold(&self, threshold: f64) {
        let mut t = self.quality_threshold.write().await;
        *t = threshold.clamp(0.0, 1.0);
        
        info!("Set quality threshold: {}", *t);
    }

    /// 获取质量阈值
    pub async fn get_quality_threshold(&self) -> f64 {
        *self.quality_threshold.read().await
    }

    /// 执行评估
    ///
    /// 根据配置的策略执行一个或多个评估器
    #[instrument(skip(self, request))]
    pub async fn assess(&self, request: QualityAssessmentRequest) -> Result<QualityAssessment> {
        let assessors = self.assessors.read().await;
        let enabled_assessors: Vec<_> = assessors.values()
            .filter(|e| e.assessor.is_enabled())
            .collect();
        
        if enabled_assessors.is_empty() {
            warn!("No enabled assessors found, returning default assessment");
            return Ok(Self::default_assessment(&request));
        }
        
        info!("Executing {} enabled assessors", enabled_assessors.len());
        
        // 执行评估
        let results = if self.enable_parallel && enabled_assessors.len() > 1 {
            // 并行执行
            self.execute_parallel(enabled_assessors, &request).await
        } else {
            // 串行执行
            self.execute_sequential(enabled_assessors, &request).await
        };
        
        // 更新统计（简化实现，跳过实际更新）
        for (_entry, _result) in assessors.values().zip(results.iter()) {
            // 简化实现：跳过统计更新
        }
        
        // 根据策略组合结果
        self.combine_results(&results)
    }

    /// 并行执行评估器
    async fn execute_parallel(
        &self,
        assessors: Vec<&AssessorEntry>,
        request: &QualityAssessmentRequest,
    ) -> Vec<Result<QualityAssessment>> {
        let futures: Vec<_> = assessors.iter()
            .map(|entry| async move {
                let start = std::time::Instant::now();
                let result = entry.assessor.assess(request.clone()).await;
                let latency = start.elapsed().as_secs_f64() * 1000.0;
                (result, latency, entry.assessor.id())
            })
            .collect();
        
        let results = futures::future::join_all(futures).await;

        // 更新统计并返回结果
        let mut final_results = Vec::new();
        for (result, latency, assessor_id) in results {
            info!("Assessor {} completed in {:.2}ms", assessor_id, latency);

            // 更新统计（需要可变引用，这里简化处理）
            // 克隆成功的结果，或者克隆错误
            final_results.push(result);
        }

        final_results
    }

    /// 串行执行评估器
    async fn execute_sequential(
        &self,
        assessors: Vec<&AssessorEntry>,
        request: &QualityAssessmentRequest,
    ) -> Vec<Result<QualityAssessment>> {
        let mut results = Vec::new();
        
        for entry in assessors {
            let start = std::time::Instant::now();
            let result = entry.assessor.assess(request.clone()).await;
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            
            info!(
                "Assessor {} completed in {:.2}ms: {}",
                entry.assessor.id(),
                latency,
                if result.is_ok() { "success" } else { "failed" }
            );
            
            results.push(result);
        }
        
        results
    }

    /// 根据策略组合多个评估结果
    fn combine_results(
        &self,
        results: &[Result<QualityAssessment>],
    ) -> Result<QualityAssessment> {
        let successful: Vec<_> = results.iter()
            .filter_map(|r| r.as_ref().ok())
            .collect();
        
        if successful.is_empty() {
            anyhow::bail!("All assessors failed");
        }
        
        let strategy = self.strategy.try_read().map(|s| *s).unwrap_or_default();
        
        let combined_score = match strategy {
            AssessmentStrategy::WeightedAverage => {
                self.weighted_average(&successful)
            }
            AssessmentStrategy::Minimum => {
                successful.iter().map(|a| a.overall_score).fold(f64::INFINITY, f64::min)
            }
            AssessmentStrategy::Maximum => {
                successful.iter().map(|a| a.overall_score).fold(f64::NEG_INFINITY, f64::max)
            }
            AssessmentStrategy::Median => {
                self.median(&successful)
            }
            AssessmentStrategy::Voting => {
                self.voting(&successful)
            }
        };
        
        // 创建组合评估结果
        Ok(QualityAssessment {
            overall_score: combined_score,
            kv_cache_valid: successful.iter().all(|a| a.kv_cache_valid),
            semantic_score: successful.iter().map(|a| a.semantic_score).sum::<f64>() / successful.len() as f64,
            integrity_score: successful.iter().map(|a| a.integrity_score).sum::<f64>() / successful.len() as f64,
            is_tampered: successful.iter().any(|a| a.is_tampered),
            details: AssessmentDetails::default(), // 简化处理
        })
    }

    /// 计算加权平均
    fn weighted_average(&self, assessments: &[&QualityAssessment]) -> f64 {
        let assessors = self.assessors.try_read().unwrap();
        
        let total_weight: f64 = assessments.iter()
            .filter_map(|a| assessors.values().find(|e| e.assessor.id() == a.details.kv_hash_match.as_ref().map(|_| "").unwrap_or("")))
            .map(|e| e.assessor.weight())
            .sum();
        
        if total_weight == 0.0 {
            return assessments.iter().map(|a| a.overall_score).sum::<f64>() / assessments.len() as f64;
        }
        
        let weighted_sum: f64 = assessments.iter()
            .zip(assessors.values())
            .map(|(a, e)| a.overall_score * e.assessor.weight())
            .sum();
        
        weighted_sum / total_weight
    }

    /// 计算中位数
    fn median(&self, assessments: &[&QualityAssessment]) -> f64 {
        let mut scores: Vec<_> = assessments.iter().map(|a| a.overall_score).collect();
        scores.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        
        let len = scores.len();
        if len % 2 == 0 {
            (scores[len / 2 - 1] + scores[len / 2]) / 2.0
        } else {
            scores[len / 2]
        }
    }

    /// 投票制
    fn voting(&self, assessments: &[&QualityAssessment]) -> f64 {
        let threshold = self.quality_threshold.try_read().map(|t| *t).unwrap_or(0.7);
        let pass_count = assessments.iter()
            .filter(|a| a.overall_score >= threshold)
            .count();
        
        // 超过半数通过
        if pass_count > assessments.len() / 2 {
            1.0
        } else {
            0.0
        }
    }

    /// 默认评估结果（当没有评估器时）
    fn default_assessment(_request: &QualityAssessmentRequest) -> QualityAssessment {
        QualityAssessment {
            overall_score: 0.5,
            kv_cache_valid: true,
            semantic_score: 0.5,
            integrity_score: 0.5,
            is_tampered: false,
            details: AssessmentDetails::default(),
        }
    }
}

// ==================== 内置评估器实现 ====================

/// 规则基础评估器
pub struct RuleBasedAssessor {
    id: String,
    weight: f64,
    enabled: bool,
}

impl RuleBasedAssessor {
    pub fn new(weight: f64) -> Self {
        RuleBasedAssessor {
            id: "rule_based".to_string(),
            weight,
            enabled: true,
        }
    }
}

#[async_trait::async_trait]
impl Assessor for RuleBasedAssessor {
    fn id(&self) -> &str {
        &self.id
    }
    
    fn name(&self) -> &str {
        "Rule-Based Assessor"
    }
    
    fn version(&self) -> &str {
        "1.0.0"
    }
    
    fn weight(&self) -> f64 {
        self.weight
    }
    
    async fn assess(&self, request: QualityAssessmentRequest) -> Result<QualityAssessment> {
        let output = &request.output;
        
        // 规则 1：检查空输出
        let empty_penalty = if output.trim().is_empty() { 0.3 } else { 0.0 };
        
        // 规则 2：检查过度重复
        let repetition_penalty = self.check_repetition(output);
        
        // 规则 3：检查异常截断
        let truncation_penalty = self.check_truncation(output);
        
        let score = (1.0 - empty_penalty - repetition_penalty - truncation_penalty).max(0.0);
        
        Ok(QualityAssessment {
            overall_score: score,
            kv_cache_valid: true, // 简化处理
            semantic_score: score,
            integrity_score: 1.0 - (empty_penalty + repetition_penalty + truncation_penalty),
            is_tampered: false,
            details: AssessmentDetails::default(),
        })
    }
    
    fn is_enabled(&self) -> bool {
        self.enabled
    }
    
    fn supported_modes(&self) -> Vec<SemanticCheckMode> {
        vec![SemanticCheckMode::Rules]
    }
}

impl RuleBasedAssessor {
    fn check_repetition(&self, output: &str) -> f64 {
        // 简化实现：检查是否有连续重复的短语
        let words: Vec<_> = output.split_whitespace().collect();
        if words.len() < 4 {
            return 0.0;
        }
        
        let mut max_repetition = 0;
        let mut current_repetition = 0;
        
        for i in 0..words.len() - 2 {
            let phrase1 = format!("{} {}", words[i], words[i + 1]);
            let phrase2 = format!("{} {}", words[i + 2], words[i + 3]);
            
            if phrase1 == phrase2 {
                current_repetition += 1;
                max_repetition = max_repetition.max(current_repetition);
            } else {
                current_repetition = 0;
            }
        }
        
        if max_repetition > 3 {
            0.2
        } else if max_repetition > 1 {
            0.1
        } else {
            0.0
        }
    }
    
    fn check_truncation(&self, output: &str) -> f64 {
        // 简化实现：检查是否以标点符号结尾
        let trimmed = output.trim();
        if trimmed.is_empty() {
            return 0.2;
        }
        
        let last_char = trimmed.chars().last().unwrap();
        if !last_char.is_ascii_punctuation() && !last_char.is_whitespace() {
            0.1
        } else {
            0.0
        }
    }
}

/// 语义相似度评估器（简化版）
pub struct SemanticSimilarityAssessor {
    id: String,
    weight: f64,
    enabled: bool,
}

impl SemanticSimilarityAssessor {
    pub fn new(weight: f64) -> Self {
        SemanticSimilarityAssessor {
            id: "semantic_similarity".to_string(),
            weight,
            enabled: true,
        }
    }
}

#[async_trait::async_trait]
impl Assessor for SemanticSimilarityAssessor {
    fn id(&self) -> &str {
        &self.id
    }
    
    fn name(&self) -> &str {
        "Semantic Similarity Assessor"
    }
    
    fn version(&self) -> &str {
        "1.0.0"
    }
    
    fn weight(&self) -> f64 {
        self.weight
    }
    
    async fn assess(&self, request: QualityAssessmentRequest) -> Result<QualityAssessment> {
        // 简化实现：基于输出长度的启发式评分
        let output_len = request.output.len();
        
        let score = if output_len < 10 {
            0.3
        } else if output_len < 50 {
            0.5
        } else if output_len < 200 {
            0.7
        } else if output_len < 1000 {
            0.85
        } else {
            0.9
        };
        
        Ok(QualityAssessment {
            overall_score: score,
            kv_cache_valid: true,
            semantic_score: score,
            integrity_score: score,
            is_tampered: false,
            details: AssessmentDetails::default(),
        })
    }
    
    fn is_enabled(&self) -> bool {
        self.enabled
    }
    
    fn supported_modes(&self) -> Vec<SemanticCheckMode> {
        vec![SemanticCheckMode::Rules, SemanticCheckMode::SmallModel]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_assessor_registry_registration() {
        let registry = AssessorRegistry::with_defaults();
        
        let assessor = Box::new(RuleBasedAssessor::new(0.5));
        registry.register(assessor).await.unwrap();
        
        let assessors = registry.list_assessors().await;
        assert_eq!(assessors.len(), 1);
        assert_eq!(assessors[0].id, "rule_based");
    }

    #[tokio::test]
    async fn test_assessor_assessment() {
        let registry = AssessorRegistry::with_defaults();
        
        let assessor = Box::new(RuleBasedAssessor::new(1.0));
        registry.register(assessor).await.unwrap();
        
        let request = QualityAssessmentRequest {
            request_id: "test".to_string(),
            output: "This is a test output.".to_string(),
            metadata: Default::default(),
        };
        
        let result = registry.assess(request).await.unwrap();
        assert!(result.overall_score > 0.5);
    }

    #[test]
    fn test_assessor_stats() {
        let mut stats = AssessorStats::default();
        
        stats.record_assessment(true, 100.0, 0.8);
        stats.record_assessment(true, 150.0, 0.9);
        stats.record_assessment(false, 200.0, 0.0);
        
        assert_eq!(stats.total_assessments, 3);
        assert_eq!(stats.successful_assessments, 2);
        assert_eq!(stats.failed_assessments, 1);
        assert!((stats.success_rate() - 0.666).abs() < 0.01);
    }
}
