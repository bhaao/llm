//! 断路器模式实现 - 故障恢复和自动切换
//!
//! **核心功能**：
//! - 跟踪连续失败次数
//! - 自动切换提供商（连续 3 次失败后）
//! - 指数退避重试（1s, 2s, 4s, 8s...）
//! - 半开状态测试恢复
//!
//! # 状态机
//!
//! ```text
//! Closed ──(失败达阈值)──> Open
//!   ↑                         │
//!   │                         │ (等待超时)
//!   │                         ↓
//!   └────(成功)──────── Half-Open
//! ```

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use dashmap::DashMap;

/// 断路器状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// 闭合状态 - 正常操作
    Closed,
    /// 断开状态 - 拒绝所有请求
    Open,
    /// 半开状态 - 允许少量请求测试恢复
    HalfOpen,
}

/// 断路器配置
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// 连续失败阈值（触发断路器打开）
    pub failure_threshold: u32,
    /// 成功阈值（半开状态下重置断路器）
    pub success_threshold: u32,
    /// 断开状态超时（秒）
    pub timeout_secs: u64,
    /// 指数退避基数（毫秒）
    pub exponential_backoff_base_ms: u64,
    /// 最大退避时间（毫秒）
    pub max_backoff_ms: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout_secs: 30,
            exponential_backoff_base_ms: 1000,
            max_backoff_ms: 60000, // 1 分钟
        }
    }
}

/// 断路器统计信息
#[derive(Debug, Clone, Default)]
pub struct CircuitStats {
    /// 总请求数
    pub total_requests: u64,
    /// 成功请求数
    pub successes: u64,
    /// 失败请求数
    pub failures: u64,
    /// 断路器打开次数
    pub circuit_opens: u64,
    /// 最后错误时间戳
    pub last_failure_time: Option<u64>,
}

/// 断路器错误
#[derive(Debug, thiserror::Error)]
pub enum CircuitBreakerError {
    #[error("断路器已打开，请稍后重试")]
    CircuitOpen,
    #[error("断路器在 {0} 次尝试后已打开")]
    CircuitOpenAfterAttempts(u32),
    #[error("操作失败：{0}")]
    OperationFailed(String),
}

/// 断路器实现
pub struct CircuitBreaker {
    /// 当前状态
    state: std::sync::RwLock<CircuitState>,
    /// 连续失败计数
    failure_count: AtomicU32,
    /// 连续成功计数（半开状态）
    success_count: AtomicU32,
    /// 最后失败时间戳（Unix 秒）
    last_failure_time: AtomicU64,
    /// 断路器打开时间戳
    opened_at: AtomicU64,
    /// 配置
    config: CircuitBreakerConfig,
    /// 统计信息
    stats: DashMap<String, CircuitStats>,
}

impl CircuitBreaker {
    /// 创建新的断路器
    pub fn new(config: CircuitBreakerConfig) -> Self {
        CircuitBreaker {
            state: std::sync::RwLock::new(CircuitState::Closed),
            failure_count: AtomicU32::new(0),
            success_count: AtomicU32::new(0),
            last_failure_time: AtomicU64::new(0),
            opened_at: AtomicU64::new(0),
            config,
            stats: DashMap::new(),
        }
    }

    /// 创建默认配置的断路器
    pub fn with_defaults() -> Self {
        Self::new(CircuitBreakerConfig::default())
    }

    /// 获取当前状态
    pub fn state(&self) -> CircuitState {
        let state = self.state.read().unwrap();
        *state
    }

    /// 检查是否允许请求通过
    pub fn allow_request(&self) -> bool {
        let current_state = self.state();

        match current_state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // 检查是否已超时
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                let opened_at = self.opened_at.load(Ordering::SeqCst);

                if now - opened_at >= self.config.timeout_secs {
                    // 转换为半开状态
                    let mut state = self.state.write().unwrap();
                    *state = CircuitState::HalfOpen;
                    self.success_count.store(0, Ordering::SeqCst);
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// 记录成功
    pub fn record_success(&self, stats_key: &str) {
        self.update_stats(stats_key, true);

        let current_state = self.state();

        if current_state == CircuitState::HalfOpen {
            let success_count = self.success_count.fetch_add(1, Ordering::SeqCst) + 1;
            if success_count >= self.config.success_threshold {
                // 重置断路器
                let mut state = self.state.write().unwrap();
                *state = CircuitState::Closed;
                self.failure_count.store(0, Ordering::SeqCst);
                self.success_count.store(0, Ordering::SeqCst);
            }
        } else {
            // 闭合状态下成功，重置失败计数
            self.failure_count.store(0, Ordering::SeqCst);
        }
    }

    /// 记录失败
    pub fn record_failure(&self, stats_key: &str) {
        self.update_stats(stats_key, false);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.last_failure_time.store(now, Ordering::SeqCst);

        let current_state = self.state();

        if current_state == CircuitState::HalfOpen {
            // 半开状态下失败，立即打开断路器
            let mut state = self.state.write().unwrap();
            *state = CircuitState::Open;
            self.opened_at.store(now, Ordering::SeqCst);
            self.update_circuit_opens(stats_key);
        } else if current_state == CircuitState::Closed {
            let failure_count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
            if failure_count >= self.config.failure_threshold {
                // 打开断路器
                let mut state = self.state.write().unwrap();
                *state = CircuitState::Open;
                self.opened_at.store(now, Ordering::SeqCst);
                self.update_circuit_opens(stats_key);
            }
        }
    }

    /// 计算退避延迟（指数退避）
    pub fn calculate_backoff(&self, attempt: u32) -> Duration {
        let base = self.config.exponential_backoff_base_ms;
        let max = self.config.max_backoff_ms;

        // 指数退避：base * 2^attempt
        let delay = base.saturating_mul(2u64.saturating_pow(attempt));
        let delay = delay.min(max);

        // 添加抖动（±10%）
        let jitter = (delay as f64 * 0.1 * (rand::random::<f64>() - 0.5) * 2.0) as u64;
        let delay = delay.saturating_add(jitter);

        Duration::from_millis(delay)
    }

    /// 带断路器保护的异步执行
    pub async fn execute_with_protection<F, T, E>(
        &self,
        stats_key: &str,
        operation: F,
    ) -> Result<T, E>
    where
        F: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display + From<CircuitBreakerError>,
    {
        if !self.allow_request() {
            // 断路器打开，返回错误
            return Err(CircuitBreakerError::CircuitOpen.into());
        }

        match operation.await {
            Ok(result) => {
                self.record_success(stats_key);
                Ok(result)
            }
            Err(e) => {
                self.record_failure(stats_key);
                Err(e)
            }
        }
    }

    /// 带重试和断路器保护的执行
    pub async fn execute_with_retry<F, T, E>(
        &self,
        stats_key: &str,
        max_retries: u32,
        operation: F,
    ) -> Result<T, E>
    where
        F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, E>> + Send>>,
        E: std::fmt::Display + From<CircuitBreakerError> + 'static,
    {
        let mut last_error: Option<E> = None;

        for attempt in 0..=max_retries {
            if !self.allow_request() {
                return Err(CircuitBreakerError::CircuitOpenAfterAttempts(attempt).into());
            }

            // 如果不是第一次尝试，等待退避时间
            if attempt > 0 {
                let backoff = self.calculate_backoff(attempt - 1);
                sleep(backoff).await;
            }

            match operation().await {
                Ok(result) => {
                    self.record_success(stats_key);
                    return Ok(result);
                }
                Err(e) => {
                    self.record_failure(stats_key);
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap())
    }

    /// 获取统计信息
    pub fn get_stats(&self, key: &str) -> CircuitStats {
        self.stats
            .get(key)
            .map(|entry| entry.clone())
            .unwrap_or_default()
    }

    /// 重置断路器
    pub fn reset(&self) {
        let mut state = self.state.write().unwrap();
        *state = CircuitState::Closed;
        self.failure_count.store(0, Ordering::SeqCst);
        self.success_count.store(0, Ordering::SeqCst);
    }

    // ========== 内部方法 ==========

    fn update_stats(&self, key: &str, success: bool) {
        let mut stats_entry = self.stats.entry(key.to_string()).or_insert_with(CircuitStats::default);
        stats_entry.total_requests += 1;
        if success {
            stats_entry.successes += 1;
        } else {
            stats_entry.failures += 1;
            stats_entry.last_failure_time = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            );
        }
    }

    fn update_circuit_opens(&self, key: &str) {
        if let Some(mut stats) = self.stats.get_mut(key) {
            stats.circuit_opens += 1;
        }
    }
}

/// 多断路器管理器 - 为每个提供商维护独立的断路器
pub struct CircuitBreakerManager {
    breakers: DashMap<String, Arc<CircuitBreaker>>,
    config: CircuitBreakerConfig,
}

impl CircuitBreakerManager {
    /// 创建新的管理器
    pub fn new(config: CircuitBreakerConfig) -> Self {
        CircuitBreakerManager {
            breakers: DashMap::new(),
            config,
        }
    }

    /// 创建默认配置的管理器
    pub fn with_defaults() -> Self {
        Self::new(CircuitBreakerConfig::default())
    }

    /// 获取或创建提供商的断路器
    pub fn get_or_create(&self, provider_id: &str) -> Arc<CircuitBreaker> {
        self.breakers
            .entry(provider_id.to_string())
            .or_insert_with(|| Arc::new(CircuitBreaker::new(self.config.clone())))
            .clone()
    }

    /// 获取所有健康（断路器闭合）的提供商列表
    pub fn get_healthy_providers(&self, provider_ids: &[String]) -> Vec<String> {
        provider_ids
            .iter()
            .filter(|id| {
                if let Some(breaker) = self.breakers.get(*id) {
                    breaker.state() == CircuitState::Closed
                        || breaker.state() == CircuitState::HalfOpen
                } else {
                    true // 没有断路器，默认健康
                }
            })
            .cloned()
            .collect()
    }

    /// 获取所有断路器状态
    pub fn get_all_states(&self) -> Vec<(String, CircuitState)> {
        self.breakers
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().state()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_basic() {
        let cb = CircuitBreaker::with_defaults();

        // 初始状态应为闭合
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.allow_request());
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_after_failures() {
        let cb = CircuitBreaker::with_defaults();

        // 记录 3 次失败
        cb.record_failure("test");
        cb.record_failure("test");
        cb.record_failure("test");

        // 断路器应打开
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.allow_request());
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open() {
        let cb = CircuitBreaker::with_defaults();

        // 打开断路器
        cb.record_failure("test");
        cb.record_failure("test");
        cb.record_failure("test");

        // 等待超时
        sleep(Duration::from_secs(31)).await;

        // 应转换为半开状态
        assert!(cb.allow_request());
        assert_eq!(cb.state(), CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn test_circuit_breaker_resets_on_success() {
        let cb = CircuitBreaker::with_defaults();

        // 打开断路器
        cb.record_failure("test");
        cb.record_failure("test");
        cb.record_failure("test");

        // 等待超时并进入半开状态
        sleep(Duration::from_secs(31)).await;
        cb.allow_request(); // 触发状态转换

        // 记录成功
        cb.record_success("test");
        cb.record_success("test");

        // 应重置为闭合
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_exponential_backoff() {
        let cb = CircuitBreaker::with_defaults();

        let delay_0 = cb.calculate_backoff(0);
        let delay_1 = cb.calculate_backoff(1);
        let delay_2 = cb.calculate_backoff(2);
        let delay_3 = cb.calculate_backoff(3);

        // 验证指数增长
        assert!(delay_1 > delay_0);
        assert!(delay_2 > delay_1);
        assert!(delay_3 > delay_2);

        // 验证不超过最大值
        assert!(delay_3 <= Duration::from_millis(60000));
    }

    #[tokio::test]
    async fn test_execute_with_protection() {
        let cb = CircuitBreaker::with_defaults();

        // 成功执行
        let result = cb
            .execute_with_protection("test", async { Ok::<_, anyhow::Error>("success".to_string()) })
            .await;
        assert!(result.is_ok());

        // 失败执行
        let result = cb
            .execute_with_protection("test", async { Err::<String, _>(anyhow::anyhow!("error")) })
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_manager_get_healthy_providers() {
        let manager = CircuitBreakerManager::with_defaults();
        let providers = vec![
            "provider_1".to_string(),
            "provider_2".to_string(),
            "provider_3".to_string(),
        ];

        // 初始所有提供商都健康
        let healthy = manager.get_healthy_providers(&providers);
        assert_eq!(healthy.len(), 3);

        // 打开 provider_2 的断路器
        let breaker = manager.get_or_create("provider_2");
        breaker.record_failure("test");
        breaker.record_failure("test");
        breaker.record_failure("test");

        // provider_2 应不健康
        let healthy = manager.get_healthy_providers(&providers);
        assert_eq!(healthy.len(), 2);
        assert!(!healthy.contains(&"provider_2".to_string()));
    }
}
