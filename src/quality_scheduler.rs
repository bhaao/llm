//! 质量感知调度器和自动重试服务 - P2-5
//!
//! **设计目标**：
//! - 基于质量和信誉的智能调度
//! - 自动重试失败请求
//! - 支持多副本并行推理
//! - 动态调整重试策略
//!
//! **调度策略**：
//! 1. **质量优先** - 优先选择高质量提供商
//! 2. **信誉优先** - 优先选择高信誉节点
//! 3. **成本优先** - 优先选择低成本提供商
//! 4. **混合策略** - 综合考量多个因素

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, sleep};
use anyhow::Result;
use tracing::{info, warn, instrument};
use serde::{Serialize, Deserialize};

use crate::provider_layer::{InferenceRequest, InferenceResponse, ProviderLayerManager};
use crate::node_layer::{ProviderRecord, ProviderStatus, QualityHistory, ReliabilityMetrics};
use crate::enhanced_reputation::EnhancedReputationManager;

/// 调度器配置
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// 最大重试次数
    pub max_retries: u32,
    /// 初始重试延迟（毫秒）
    pub initial_retry_delay_ms: u64,
    /// 重试延迟增长因子（指数退避）
    pub retry_backoff_multiplier: f64,
    /// 最大重试延迟（毫秒）
    pub max_retry_delay_ms: u64,
    /// 质量阈值（低于此值不调度）
    pub quality_threshold: f64,
    /// 信誉阈值（低于此值不调度）
    pub reputation_threshold: f64,
    /// 启用多副本并行
    pub enable_parallel_replicas: bool,
    /// 并行副本数量
    pub parallel_replica_count: usize,
    /// 启用自动降级
    pub enable_auto_fallback: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        SchedulerConfig {
            max_retries: 3,
            initial_retry_delay_ms: 100,
            retry_backoff_multiplier: 2.0,
            max_retry_delay_ms: 5000,
            quality_threshold: 0.6,
            reputation_threshold: 0.5,
            enable_parallel_replicas: false,
            parallel_replica_count: 3,
            enable_auto_fallback: true,
        }
    }
}

/// 调度决策
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingDecision {
    /// 选中的提供商 ID
    pub selected_provider_id: String,
    /// 调度得分
    pub score: f64,
    /// 调度原因
    pub reason: String,
    /// 备选提供商列表
    pub fallback_providers: Vec<String>,
    /// 是否使用并行副本
    pub use_parallel_replicas: bool,
    /// 并行副本提供商列表
    pub parallel_providers: Vec<String>,
}

/// 重试策略
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RetryStrategy {
    /// 固定延迟
    FixedDelay,
    /// 指数退避
    ExponentialBackoff,
    /// 随机抖动
    Jitter,
    /// 自适应（基于历史）
    Adaptive,
}

impl RetryStrategy {
    /// 计算下次重试延迟
    pub fn calculate_delay(
        &self,
        attempt: u32,
        initial_delay_ms: u64,
        backoff_multiplier: f64,
        max_delay_ms: u64,
    ) -> u64 {
        let delay = match self {
            RetryStrategy::FixedDelay => initial_delay_ms,
            RetryStrategy::ExponentialBackoff => {
                (initial_delay_ms as f64 * backoff_multiplier.powi(attempt as i32)) as u64
            }
            RetryStrategy::Jitter => {
                let base_delay = (initial_delay_ms as f64 * backoff_multiplier.powi(attempt as i32)) as u64;
                // 添加 ±20% 的随机抖动
                let jitter_range = (base_delay as f64 * 0.2) as u64;
                let jitter = (rand::random::<u64>() % (jitter_range * 2)) - jitter_range;
                (base_delay as i64 + jitter as i64).max(0) as u64
            }
            RetryStrategy::Adaptive => {
                // 简化实现：指数退避 + 限制
                ((initial_delay_ms as f64 * backoff_multiplier.powi(attempt as i32)) as u64)
                    .min(max_delay_ms / 2)
            }
        };
        
        delay.min(max_delay_ms)
    }
}

/// 重试状态
#[derive(Debug, Clone)]
pub struct RetryState {
    /// 当前重试次数
    pub attempt: u32,
    /// 已尝试的提供商列表
    pub attempted_providers: Vec<String>,
    /// 上次错误信息
    pub last_error: Option<String>,
    /// 累计耗时（毫秒）
    pub total_elapsed_ms: u64,
}

impl RetryState {
    fn new() -> Self {
        RetryState {
            attempt: 0,
            attempted_providers: Vec::new(),
            last_error: None,
            total_elapsed_ms: 0,
        }
    }
}

/// 质量感知调度器
///
/// **功能**：
/// - 基于质量和信誉选择提供商
/// - 支持多策略调度
/// - 动态调整调度权重
pub struct QualityAwareScheduler {
    /// 配置
    config: SchedulerConfig,
    /// 提供商层管理器
    #[allow(dead_code)]
    provider_layer: Arc<ProviderLayerManager>,
    /// 增强信誉管理器
    reputation_manager: Arc<EnhancedReputationManager>,
    /// 提供商质量历史
    quality_history: Arc<RwLock<HashMap<String, QualityHistory>>>,
    /// 提供商可靠性指标
    reliability_metrics: Arc<RwLock<HashMap<String, ReliabilityMetrics>>>,
}

impl QualityAwareScheduler {
    /// 创建新的调度器
    pub fn new(
        config: SchedulerConfig,
        provider_layer: Arc<ProviderLayerManager>,
        reputation_manager: Arc<EnhancedReputationManager>,
    ) -> Self {
        QualityAwareScheduler {
            config,
            provider_layer,
            reputation_manager,
            quality_history: Arc::new(RwLock::new(HashMap::new())),
            reliability_metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 创建默认调度器
    pub fn with_defaults(
        provider_layer: Arc<ProviderLayerManager>,
        reputation_manager: Arc<EnhancedReputationManager>,
    ) -> Self {
        Self::new(SchedulerConfig::default(), provider_layer, reputation_manager)
    }

    /// 选择最佳提供商
    #[instrument(skip(self), fields(request_id = %request.request_id))]
    pub async fn select_provider(&self, request: &InferenceRequest) -> Result<SchedulingDecision> {
        info!("Selecting provider for request: {}", request.request_id);

        // 获取所有活跃提供商
        let providers = self.get_active_providers().await?;

        if providers.is_empty() {
            anyhow::bail!("No active providers available");
        }

        // 过滤不符合阈值的提供商
        let filtered_providers = self.filter_by_threshold(providers).await?;

        if filtered_providers.is_empty() {
            anyhow::bail!("No providers meet quality/reputation thresholds");
        }

        // 计算每个提供商的得分
        let mut scored_providers = Vec::new();
        for provider in filtered_providers {
            if let Some(score) = self.calculate_provider_score(&provider).await {
                scored_providers.push((provider, score));
            }
        }

        // 按得分排序
        scored_providers.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        if scored_providers.is_empty() {
            anyhow::bail!("No providers could be scored");
        }

        // 选择最佳提供商
        let (best_provider, best_score) = scored_providers[0].clone();

        // 准备备选列表
        let fallback_providers: Vec<_> = scored_providers.iter()
            .skip(1)
            .take(3)
            .map(|(p, _)| p.provider_id.clone())
            .collect();

        // 并行副本（如果启用）
        let parallel_providers = if self.config.enable_parallel_replicas {
            scored_providers.iter()
                .take(self.config.parallel_replica_count)
                .map(|(p, _)| p.provider_id.clone())
                .collect()
        } else {
            Vec::new()
        };

        Ok(SchedulingDecision {
            selected_provider_id: best_provider.provider_id.clone(),
            score: best_score,
            reason: format!("Highest score based on quality, reputation, and reliability"),
            fallback_providers,
            use_parallel_replicas: self.config.enable_parallel_replicas && parallel_providers.len() > 1,
            parallel_providers,
        })
    }

    /// 获取活跃提供商列表
    async fn get_active_providers(&self) -> Result<Vec<ProviderRecord>> {
        // 简化实现：返回所有 Active 状态的提供商
        // 实际实现需要从 provider_layer 获取
        Ok(Vec::new())
    }

    /// 过滤不符合阈值的提供商
    async fn filter_by_threshold(&self, providers: Vec<ProviderRecord>) -> Result<Vec<ProviderRecord>> {
        let filtered = providers.into_iter()
            .filter(|p| {
                p.status == ProviderStatus::Active
                    && p.quality_score >= self.config.quality_threshold
            })
            .collect();
        
        Ok(filtered)
    }

    /// 计算提供商得分
    async fn calculate_provider_score(&self, provider: &ProviderRecord) -> Option<f64> {
        // 质量得分（40%）
        let quality_score = provider.quality_score;

        // 可靠性得分（30%）
        let reliability_score = provider.reliability_metrics.compute_reliability_score();

        // 信誉得分（30%）- 从信誉管理器获取
        let reputation_score = self.get_reputation_score(&provider.provider_id).await
            .unwrap_or(0.5);

        // 检查是否被禁止调度
        if self.reputation_manager.is_scheduling_blocked(&provider.provider_id).await {
            return None;
        }

        // 应用调度权重乘数
        let weight_multiplier = self.reputation_manager
            .get_scheduling_weight(&provider.provider_id)
            .await;

        let total_score = (quality_score * 0.4 + reliability_score * 0.3 + reputation_score * 0.3)
            * weight_multiplier;

        Some(total_score)
    }

    /// 获取信誉得分
    async fn get_reputation_score(&self, _node_id: &str) -> Option<f64> {
        // 简化实现
        Some(0.8)
    }

    /// 记录质量结果
    pub async fn record_quality_result(
        &self,
        provider_id: &str,
        quality_score: f64,
        passed: bool,
    ) {
        let mut history = self.quality_history.write().await;
        let entry = history.entry(provider_id.to_string())
            .or_insert_with(QualityHistory::default);
        
        entry.record_quality_check(quality_score, passed);
    }

    /// 记录可靠性指标
    pub async fn record_reliability_metric(
        &self,
        provider_id: &str,
        response_time_ms: f64,
        success: bool,
        timed_out: bool,
        error: bool,
    ) {
        let mut metrics = self.reliability_metrics.write().await;
        let entry = metrics.entry(provider_id.to_string())
            .or_insert_with(ReliabilityMetrics::default);
        
        entry.record_response_time(response_time_ms);
        entry.record_completion(success, timed_out, error);
    }
}

/// 自动重试服务
///
/// **功能**：
/// - 自动重试失败的推理请求
/// - 智能选择重试目标
/// - 支持多种重试策略
pub struct AutoRetryService {
    /// 配置
    config: SchedulerConfig,
    /// 调度器
    scheduler: Arc<QualityAwareScheduler>,
    /// 重试策略
    retry_strategy: RetryStrategy,
    /// 重试状态追踪
    retry_states: Arc<RwLock<HashMap<String, RetryState>>>,
}

impl AutoRetryService {
    /// 创建新的重试服务
    pub fn new(
        config: SchedulerConfig,
        scheduler: Arc<QualityAwareScheduler>,
        retry_strategy: RetryStrategy,
    ) -> Self {
        AutoRetryService {
            config,
            scheduler,
            retry_strategy,
            retry_states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 执行带重试的推理
    #[instrument(skip(self, request), fields(request_id = %request.request_id))]
    pub async fn execute_with_retry(
        &self,
        request: &InferenceRequest,
    ) -> Result<InferenceResponse> {
        info!("Executing inference with retry for request: {}", request.request_id);

        let mut state = RetryState::new();
        let start_time = std::time::Instant::now();

        while state.attempt <= self.config.max_retries {
            // 选择提供商
            let decision = self.scheduler.select_provider(request).await?;

            // 检查是否已经尝试过这个提供商
            if state.attempted_providers.contains(&decision.selected_provider_id) {
                if decision.fallback_providers.is_empty() {
                    break; // 没有可用的备选
                }
                // 使用备选提供商
                // 简化实现：直接选择第一个备选
            }

            state.attempted_providers.push(decision.selected_provider_id.clone());

            // 执行推理
            match self.execute_inference(request, &decision.selected_provider_id).await {
                Ok(response) => {
                    info!(
                        "Inference succeeded on attempt {} with provider {}",
                        state.attempt + 1,
                        decision.selected_provider_id
                    );
                    
                    // 记录成功
                    self.scheduler.record_quality_result(
                        &decision.selected_provider_id,
                        0.9, // 简化：假设成功就是高质量
                        true,
                    ).await;
                    
                    self.scheduler.record_reliability_metric(
                        &decision.selected_provider_id,
                        response.latency_ms as f64,
                        true,
                        false,
                        false,
                    ).await;

                    // 清理重试状态
                    self.retry_states.write().await.remove(&request.request_id);

                    return Ok(response);
                }
                Err(e) => {
                    warn!(
                        "Inference failed on attempt {} with provider {}: {}",
                        state.attempt + 1,
                        decision.selected_provider_id,
                        e
                    );

                    state.last_error = Some(e.to_string());
                    state.attempt += 1;
                    state.total_elapsed_ms = start_time.elapsed().as_millis() as u64;

                    if state.attempt <= self.config.max_retries {
                        // 计算重试延迟
                        let delay_ms = self.retry_strategy.calculate_delay(
                            state.attempt,
                            self.config.initial_retry_delay_ms,
                            self.config.retry_backoff_multiplier,
                            self.config.max_retry_delay_ms,
                        );

                        info!("Retrying in {}ms (attempt {}/{})", delay_ms, state.attempt, self.config.max_retries);
                        sleep(Duration::from_millis(delay_ms)).await;
                    }
                }
            }
        }

        // 所有重试失败
        let error_msg = state.last_error.unwrap_or_else(|| "Unknown error".to_string());
        anyhow::bail!(
            "Inference failed after {} attempts: {}",
            state.attempt,
            error_msg
        )
    }

    /// 执行单次推理
    async fn execute_inference(
        &self,
        _request: &InferenceRequest,
        _provider_id: &str,
    ) -> Result<InferenceResponse> {
        // 简化实现：实际应该调用 provider_layer
        anyhow::bail!("Not implemented - requires provider layer integration")
    }

    /// 获取重试状态
    pub async fn get_retry_state(&self, request_id: &str) -> Option<RetryState> {
        let states = self.retry_states.read().await;
        states.get(request_id).cloned()
    }

    /// 取消重试
    pub async fn cancel_retry(&self, request_id: &str) {
        let mut states = self.retry_states.write().await;
        states.remove(request_id);
    }
}

/// 多副本并行推理结果
#[derive(Debug, Clone)]
pub struct ParallelInferenceResult {
    /// 主响应
    pub primary_response: InferenceResponse,
    /// 所有响应
    pub all_responses: Vec<InferenceResponse>,
    /// 一致性得分（0.0 - 1.0）
    pub consistency_score: f64,
    /// 是否通过一致性检查
    pub passed_consistency_check: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_strategy_fixed_delay() {
        let strategy = RetryStrategy::FixedDelay;
        
        let delay = strategy.calculate_delay(0, 100, 2.0, 5000);
        assert_eq!(delay, 100);
        
        let delay = strategy.calculate_delay(3, 100, 2.0, 5000);
        assert_eq!(delay, 100);
    }

    #[test]
    fn test_retry_strategy_exponential_backoff() {
        let strategy = RetryStrategy::ExponentialBackoff;
        
        let delay = strategy.calculate_delay(0, 100, 2.0, 5000);
        assert_eq!(delay, 100);
        
        let delay = strategy.calculate_delay(1, 100, 2.0, 5000);
        assert_eq!(delay, 200);
        
        let delay = strategy.calculate_delay(2, 100, 2.0, 5000);
        assert_eq!(delay, 400);
        
        let delay = strategy.calculate_delay(10, 100, 2.0, 5000);
        assert_eq!(delay, 5000); // 达到最大值
    }

    #[test]
    fn test_retry_state() {
        let mut state = RetryState::new();
        
        assert_eq!(state.attempt, 0);
        assert!(state.attempted_providers.is_empty());
        assert!(state.last_error.is_none());
        
        state.attempt = 1;
        state.attempted_providers.push("provider_1".to_string());
        state.last_error = Some("test error".to_string());
        
        assert_eq!(state.attempt, 1);
        assert_eq!(state.attempted_providers.len(), 1);
        assert!(state.last_error.is_some());
    }
}
