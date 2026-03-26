//! 计算完整性检查器 - P1-4：偷工减料检测
//!
//! **设计目标**：
//! - 检测推理节点是否跳过计算步骤
//! - 验证 KV Cache 生成是否完整
//! - 检查 token 生成是否符合预期
//! - 识别异常的计算加速行为
//!
//! **检测维度**：
//! 1. **时间异常检测** - 计算时间短于理论最小值
//! 2. **Token 计数检测** - 输出 token 数量与报告不符
//! 3. **KV Cache 完整性** - KV Cache 条目数量与计算量不匹配
//! 4. **计算模式分析** - 识别跳步、截断等行为

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;
use tracing::{info, instrument};
use serde::{Serialize, Deserialize};

use crate::provider_layer::{InferenceRequest, InferenceResponse};
use crate::memory_layer::KvShard;

/// 计算完整性检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityCheckResult {
    /// 检查是否通过
    pub passed: bool,
    /// 总体可信度得分（0.0 - 1.0）
    pub confidence_score: f64,
    /// 检测到的问题列表
    pub issues: Vec<IntegrityIssue>,
    /// 检查详情
    pub details: CheckDetails,
}

/// 完整性问题
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityIssue {
    /// 问题类型
    pub issue_type: IssueType,
    /// 问题描述
    pub description: String,
    /// 严重程度（0.0 - 1.0）
    pub severity: f64,
    /// 证据
    pub evidence: String,
}

/// 问题类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum IssueType {
    /// 计算时间过短
    TooFastComputation,
    /// Token 数量不符
    TokenCountMismatch,
    /// KV Cache 不完整
    IncompleteKvCache,
    /// 跳步检测
    StepSkipping,
    /// 异常截断
    AbnormalTruncation,
    /// 重复计算（可能作弊）
    DuplicateComputation,
    /// 其他异常
    OtherAnomaly,
}

/// 检查详情
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CheckDetails {
    /// 预期计算时间（毫秒）
    pub expected_computation_time_ms: Option<f64>,
    /// 实际计算时间（毫秒）
    pub actual_computation_time_ms: Option<f64>,
    /// 预期 token 数量
    pub expected_token_count: Option<usize>,
    /// 实际 token 数量
    pub actual_token_count: Option<usize>,
    /// KV Cache 条目数量
    pub kv_cache_entries: Option<usize>,
    /// 时间异常得分（0.0 - 1.0，越高越异常）
    pub time_anomaly_score: f64,
    /// Token 异常得分
    pub token_anomaly_score: f64,
    /// KV 异常得分
    pub kv_anomaly_score: f64,
}

/// 计算完整性检查器配置
#[derive(Debug, Clone)]
pub struct IntegrityCheckerConfig {
    /// 最小 token 生成时间（毫秒/token）
    pub min_token_generation_time_ms: f64,
    /// 时间异常阈值（低于此比例认为异常）
    pub time_anomaly_threshold: f64,
    /// Token 数量容差比例
    pub token_count_tolerance: f64,
    /// KV Cache 最小填充率
    pub min_kv_cache_fill_rate: f64,
    /// 启用时间检测
    pub enable_time_check: bool,
    /// 启用 Token 检测
    pub enable_token_check: bool,
    /// 启用 KV Cache 检测
    pub enable_kv_cache_check: bool,
    /// 启用历史模式分析
    pub enable_pattern_analysis: bool,
}

impl Default for IntegrityCheckerConfig {
    fn default() -> Self {
        IntegrityCheckerConfig {
            min_token_generation_time_ms: 10.0, // 10ms/token
            time_anomaly_threshold: 0.5,        // 低于 50% 认为异常
            token_count_tolerance: 0.1,         // 10% 容差
            min_kv_cache_fill_rate: 0.8,        // 80% 最小填充率
            enable_time_check: true,
            enable_token_check: true,
            enable_kv_cache_check: true,
            enable_pattern_analysis: true,
        }
    }
}

/// 节点计算历史记录
#[derive(Debug, Clone, Default)]
pub struct NodeComputationHistory {
    /// 历史计算时间（毫秒）
    pub computation_times: VecDeque<f64>,
    /// 历史 token 数量
    pub token_counts: VecDeque<usize>,
    /// 历史 KV Cache 大小
    pub kv_cache_sizes: VecDeque<usize>,
    /// 历史问题记录
    pub issues: Vec<IntegrityIssue>,
}

impl NodeComputationHistory {
    fn new(max_size: usize) -> Self {
        NodeComputationHistory {
            computation_times: VecDeque::with_capacity(max_size),
            token_counts: VecDeque::with_capacity(max_size),
            kv_cache_sizes: VecDeque::with_capacity(max_size),
            issues: Vec::new(),
        }
    }

    fn record(&mut self, time_ms: f64, token_count: usize, kv_cache_size: usize) {
        self.computation_times.push_back(time_ms);
        self.token_counts.push_back(token_count);
        self.kv_cache_sizes.push_back(kv_cache_size);

        // 限制历史记录大小
        let max_size = self.computation_times.capacity();
        while self.computation_times.len() > max_size {
            self.computation_times.pop_front();
            self.token_counts.pop_front();
            self.kv_cache_sizes.pop_front();
        }
    }

    fn avg_computation_time(&self) -> Option<f64> {
        if self.computation_times.is_empty() {
            return None;
        }
        let sum: f64 = self.computation_times.iter().sum();
        Some(sum / self.computation_times.len() as f64)
    }

    fn avg_token_count(&self) -> Option<f64> {
        if self.token_counts.is_empty() {
            return None;
        }
        let sum: usize = self.token_counts.iter().sum();
        Some(sum as f64 / self.token_counts.len() as f64)
    }
}

/// 计算完整性检查器
///
/// **功能**：
/// - 实时检测计算完整性
/// - 基于历史数据识别异常模式
/// - 生成详细的问题报告
pub struct ComputationIntegrityChecker {
    /// 配置
    config: IntegrityCheckerConfig,
    /// 节点计算历史
    node_history: Arc<RwLock<HashMap<String, NodeComputationHistory>>>,
    /// 最大历史记录数
    max_history_size: usize,
}

impl ComputationIntegrityChecker {
    /// 创建新的检查器
    pub fn new(config: IntegrityCheckerConfig) -> Self {
        ComputationIntegrityChecker {
            config,
            node_history: Arc::new(RwLock::new(HashMap::new())),
            max_history_size: 100,
        }
    }

    /// 创建默认检查器
    pub fn with_defaults() -> Self {
        Self::new(IntegrityCheckerConfig::default())
    }

    /// 执行完整性检查
    #[instrument(skip(self, request, response), fields(request_id = %request.request_id))]
    pub async fn check(
        &self,
        request: &InferenceRequest,
        response: &InferenceResponse,
        kv_shards: Option<&[KvShard]>,
    ) -> Result<IntegrityCheckResult> {
        info!("Performing computation integrity check");

        let mut issues = Vec::new();
        let mut details = CheckDetails::default();

        // 计算实际时间
        let actual_time_ms = response.latency_ms as f64;
        
        // 预期时间：基于 token 数量的理论最小值
        let expected_tokens = response.completion_tokens as f64;
        let expected_time_ms = expected_tokens * self.config.min_token_generation_time_ms;

        details.expected_computation_time_ms = Some(expected_time_ms);
        details.actual_computation_time_ms = Some(actual_time_ms);
        details.expected_token_count = Some(response.completion_tokens as usize);

        // 1. 时间异常检测
        if self.config.enable_time_check {
            if let Some(issue) = self.check_time_anomaly(
                "unknown_provider",
                actual_time_ms,
                expected_time_ms,
            ).await {
                issues.push(issue);
            }
        }

        // 2. Token 计数检测
        if self.config.enable_token_check {
            if let Some(issue) = self.check_token_count(
                request,
                response,
            ) {
                issues.push(issue);
            }
        }

        // 3. KV Cache 完整性检测
        if self.config.enable_kv_cache_check {
            if let Some(kv_shards) = kv_shards {
                if let Some(issue) = self.check_kv_cache_integrity(
                    request,
                    kv_shards,
                ) {
                    issues.push(issue);
                }
                details.kv_cache_entries = Some(kv_shards.len());
            }
        }

        // 4. 历史模式分析
        if self.config.enable_pattern_analysis {
            if let Some(issue) = self.analyze_pattern("unknown_provider").await {
                issues.push(issue);
            }
        }

        // 记录历史
        self.record_history(
            "unknown_provider",
            actual_time_ms,
            response.completion_tokens as usize,
            kv_shards.map(|s| s.len()).unwrap_or(0),
        ).await;

        // 计算总体得分
        let confidence_score = self.calculate_confidence_score(&issues, &details);
        let passed = issues.is_empty();

        Ok(IntegrityCheckResult {
            passed,
            confidence_score,
            issues,
            details,
        })
    }

    /// 检查时间异常
    async fn check_time_anomaly(
        &self,
        provider_id: &str,
        actual_time_ms: f64,
        expected_time_ms: f64,
    ) -> Option<IntegrityIssue> {
        let ratio = actual_time_ms / expected_time_ms.max(1.0);

        // 计算时间异常得分
        let anomaly_score = if ratio < self.config.time_anomaly_threshold {
            1.0 - ratio
        } else {
            0.0
        };

        if anomaly_score > 0.3 {
            // 获取历史平均时间
            let history = self.node_history.read().await;
            let avg_time = history.get(provider_id)
                .and_then(|h| h.avg_computation_time());

            let description = if let Some(avg) = avg_time {
                format!(
                    "Computation time {:.2}ms is {:.1}% faster than expected {:.2}ms, \
                     historical average: {:.2}ms",
                    actual_time_ms,
                    (1.0 - ratio) * 100.0,
                    expected_time_ms,
                    avg
                )
            } else {
                format!(
                    "Computation time {:.2}ms is {:.1}% faster than expected {:.2}ms",
                    actual_time_ms,
                    (1.0 - ratio) * 100.0,
                    expected_time_ms
                )
            };

            Some(IntegrityIssue {
                issue_type: IssueType::TooFastComputation,
                description,
                severity: anomaly_score,
                evidence: format!("ratio={:.3}, threshold={:.3}", ratio, self.config.time_anomaly_threshold),
            })
        } else {
            None
        }
    }

    /// 检查 Token 计数
    fn check_token_count(
        &self,
        request: &InferenceRequest,
        response: &InferenceResponse,
    ) -> Option<IntegrityIssue> {
        let expected_max = request.max_tokens as usize;
        let actual = response.completion_tokens as usize;

        // 检查是否超过最大 token 数
        if actual > expected_max {
            return Some(IntegrityIssue {
                issue_type: IssueType::TokenCountMismatch,
                description: format!(
                    "Generated {} tokens exceeds maximum allowed {}",
                    actual, expected_max
                ),
                severity: (actual - expected_max) as f64 / expected_max as f64,
                evidence: format!("actual={}, max={}", actual, expected_max),
            });
        }

        // 检查是否异常少（可能提前截断）
        let min_expected = (expected_max as f64 * (1.0 - self.config.token_count_tolerance)) as usize;
        if actual < min_expected && expected_max > 10 {
            let severity = (min_expected - actual) as f64 / min_expected as f64;
            
            return Some(IntegrityIssue {
                issue_type: IssueType::AbnormalTruncation,
                description: format!(
                    "Generated {} tokens is significantly less than expected minimum {}",
                    actual, min_expected
                ),
                severity,
                evidence: format!("actual={}, min_expected={}", actual, min_expected),
            });
        }

        None
    }

    /// 检查 KV Cache 完整性
    fn check_kv_cache_integrity(
        &self,
        request: &InferenceRequest,
        kv_shards: &[KvShard],
    ) -> Option<IntegrityIssue> {
        // 简化实现：检查 KV Cache 条目数量是否合理
        let expected_entries = (request.max_tokens as f64 * 0.8) as usize;
        let actual_entries = kv_shards.len();

        let fill_rate = actual_entries as f64 / expected_entries as f64;

        if fill_rate < self.config.min_kv_cache_fill_rate {
            Some(IntegrityIssue {
                issue_type: IssueType::IncompleteKvCache,
                description: format!(
                    "KV Cache has {} entries, expected at least {} (fill rate: {:.1}%)",
                    actual_entries,
                    expected_entries,
                    fill_rate * 100.0
                ),
                severity: 1.0 - fill_rate,
                evidence: format!("actual={}, expected={}, fill_rate={:.3}", 
                    actual_entries, expected_entries, fill_rate),
            })
        } else {
            None
        }
    }

    /// 分析历史模式
    async fn analyze_pattern(&self, provider_id: &str) -> Option<IntegrityIssue> {
        let history = self.node_history.read().await;
        
        if let Some(hist) = history.get(provider_id) {
            // 检查是否有连续快速计算
            let times: Vec<_> = hist.computation_times.iter().collect();
            if times.len() >= 5 {
                let recent_avg: f64 = times[times.len() - 5..].iter().map(|&&t| t).sum::<f64>() / 5.0;
                let overall_avg = hist.avg_computation_time().unwrap_or(recent_avg);

                if recent_avg < overall_avg * 0.7 {
                    return Some(IntegrityIssue {
                        issue_type: IssueType::StepSkipping,
                        description: format!(
                            "Recent computations are {:.1}% faster than historical average, \
                             possible step skipping detected",
                            (1.0 - recent_avg / overall_avg) * 100.0
                        ),
                        severity: 0.7,
                        evidence: format!("recent_avg={:.2}ms, overall_avg={:.2}ms", 
                            recent_avg, overall_avg),
                    });
                }
            }
        }

        None
    }

    /// 记录计算历史
    async fn record_history(
        &self,
        provider_id: &str,
        time_ms: f64,
        token_count: usize,
        kv_cache_size: usize,
    ) {
        let mut history = self.node_history.write().await;
        
        let entry = history.entry(provider_id.to_string())
            .or_insert_with(|| NodeComputationHistory::new(self.max_history_size));
        
        entry.record(time_ms, token_count, kv_cache_size);
    }

    /// 计算总体可信度得分
    fn calculate_confidence_score(
        &self,
        issues: &[IntegrityIssue],
        _details: &CheckDetails,
    ) -> f64 {
        if issues.is_empty() {
            return 1.0;
        }

        // 根据问题严重程度扣分
        let total_penalty: f64 = issues.iter()
            .map(|issue| issue.severity * 0.3) // 每个问题最多扣 0.3
            .sum();

        (1.0 - total_penalty).max(0.0)
    }

    /// 获取节点历史统计
    pub async fn get_node_stats(&self, provider_id: &str) -> Option<NodeStats> {
        let history = self.node_history.read().await;
        
        history.get(provider_id).map(|h| NodeStats {
            provider_id: provider_id.to_string(),
            total_checks: h.computation_times.len() as u64,
            avg_computation_time_ms: h.avg_computation_time().unwrap_or(0.0),
            avg_token_count: h.avg_token_count().unwrap_or(0.0) as usize,
            issue_count: h.issues.len() as u64,
        })
    }

    /// 清除节点历史
    pub async fn clear_history(&self, provider_id: Option<&str>) {
        let mut history = self.node_history.write().await;
        
        if let Some(id) = provider_id {
            history.remove(id);
        } else {
            history.clear();
        }
    }
}

/// 节点统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStats {
    /// 节点 ID
    pub provider_id: String,
    /// 总检查次数
    pub total_checks: u64,
    /// 平均计算时间（毫秒）
    pub avg_computation_time_ms: f64,
    /// 平均 token 数量
    pub avg_token_count: usize,
    /// 问题记录数
    pub issue_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_integrity_checker_normal() {
        let checker = ComputationIntegrityChecker::with_defaults();

        let request = InferenceRequest {
            request_id: "test".to_string(),
            prompt: "test prompt".to_string(),
            model_name: "test-model".to_string(),
            max_tokens: 100,
        };

        let response = InferenceResponse {
            request_id: "test".to_string(),
            completion: "test output".to_string(),
            prompt_tokens: 10,
            completion_tokens: 50,
            latency_ms: 600,
            efficiency: 5.0,
            new_kv: HashMap::new(),
            success: true,
            error_message: None,
        };

        let result = checker.check(&request, &response, None).await.unwrap();
        
        assert!(result.passed);
        assert!(result.confidence_score > 0.8);
    }

    #[tokio::test]
    async fn test_integrity_checker_too_fast() {
        let checker = ComputationIntegrityChecker::with_defaults();

        let request = InferenceRequest {
            request_id: "test".to_string(),
            prompt: "test prompt".to_string(),
            model_name: "test-model".to_string(),
            max_tokens: 100,
        };

        let response = InferenceResponse {
            request_id: "test".to_string(),
            completion: "test output".to_string(),
            prompt_tokens: 10,
            completion_tokens: 100,
            latency_ms: 100, // 异常快：100 tokens 只用了 100ms
            efficiency: 60.0,
            new_kv: HashMap::new(),
            success: true,
            error_message: None,
        };

        let result = checker.check(&request, &response, None).await.unwrap();
        
        assert!(!result.passed);
        assert!(result.issues.iter().any(|i| i.issue_type == IssueType::TooFastComputation));
    }

    #[test]
    fn test_integrity_issue_serialization() {
        let issue = IntegrityIssue {
            issue_type: IssueType::TooFastComputation,
            description: "Test issue".to_string(),
            severity: 0.8,
            evidence: "evidence".to_string(),
        };

        let json = serde_json::to_string(&issue).unwrap();
        let restored: IntegrityIssue = serde_json::from_str(&json).unwrap();

        assert_eq!(issue.issue_type, restored.issue_type);
        assert_eq!(issue.description, restored.description);
    }
}
