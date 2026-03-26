//! 故障转移模块 - 推理超时自动切换备用提供商
//!
//! **核心目标**：
//! - 提供商卡住/超时/宕机时自动发现
//! - 自动切换到备用提供商
//! - 不丢上下文、不中断任务
//!
//! # 关键机制
//!
//! 1. **超时检测**：给每次推理加超时时间
//! 2. **健康状态管理**：维护提供商健康状态（正常/异常/冷却中）
//! 3. **备用提供商选举**：按优先级/信誉/负载选择下一个
//! 4. **上下文不丢失**：切换前将上下文存入记忆层
//! 5. **防抖动机制**：冷却时间 + 最大切换次数限制
//! 6. **断路器模式**：连续失败自动打开断路器，指数退避重试

pub mod circuit_breaker;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(test)]
use std::time::Duration;
use serde::{Serialize, Deserialize};

/// 辅助函数：处理 RwLock 写锁的 poison error
fn write_lock<T>(lock: &RwLock<T>) -> Result<RwLockWriteGuard<'_, T>, String> {
    lock.write().map_err(|e| {
        format!("RwLock write poisoned: {}", e)
    })
}

/// 辅助函数：处理 RwLock 读锁的 poison error
fn read_lock<T>(lock: &RwLock<T>) -> Result<RwLockReadGuard<'_, T>, String> {
    lock.read().map_err(|e| {
        format!("RwLock read poisoned: {}", e)
    })
}

/// 超时配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutConfig {
    /// 推理超时时间（毫秒）
    pub inference_timeout_ms: u64,
    /// 心跳检测间隔（毫秒）
    pub heartbeat_interval_ms: u64,
    /// 提供商冷却时间（毫秒）
    pub cooldown_ms: u64,
    /// 单个任务最大切换次数
    pub max_failover_count: u32,
    /// 连续超时阈值（超过此值标记为不健康）
    pub consecutive_timeout_threshold: u32,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        TimeoutConfig {
            inference_timeout_ms: 30000, // 30 秒
            heartbeat_interval_ms: 5000, // 5 秒
            cooldown_ms: 10000,          // 10 秒
            max_failover_count: 2,       // 最多切换 2 次
            consecutive_timeout_threshold: 3, // 连续 3 次超时标记为不健康
        }
    }
}

/// 提供商健康状态
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderHealthStatus {
    /// 正常
    Healthy,
    /// 异常（连续超时/宕机）
    Unhealthy,
    /// 冷却中（暂时不可用）
    Cooldown,
    /// 未知（尚未探测）
    Unknown,
}

/// 提供商健康记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHealthRecord {
    /// 提供商 ID
    pub provider_id: String,
    /// 健康状态
    pub status: ProviderHealthStatus,
    /// 连续超时次数
    pub consecutive_timeouts: u32,
    /// 总请求次数
    pub total_requests: u64,
    /// 失败次数
    pub failures: u64,
    /// 最后一次成功时间戳
    pub last_success_time: Option<u64>,
    /// 最后一次失败时间戳
    pub last_failure_time: Option<u64>,
    /// 进入冷却的时间戳
    pub cooldown_until: Option<u64>,
    /// 信誉评分
    pub reputation_score: f64,
    /// 平均响应时间（毫秒）
    pub avg_latency_ms: f64,
}

impl ProviderHealthRecord {
    pub fn new(provider_id: String) -> Self {
        ProviderHealthRecord {
            provider_id,
            status: ProviderHealthStatus::Unknown,
            consecutive_timeouts: 0,
            total_requests: 0,
            failures: 0,
            last_success_time: None,
            last_failure_time: None,
            cooldown_until: None,
            reputation_score: 0.5,
            avg_latency_ms: 0.0,
        }
    }

    /// 记录成功
    pub fn record_success(&mut self, latency_ms: u64) {
        self.total_requests = self.total_requests.saturating_add(1);
        self.consecutive_timeouts = 0;
        self.last_success_time = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );
        
        // 更新平均延迟（简单移动平均）
        let count = self.total_requests as f64;
        self.avg_latency_ms = (self.avg_latency_ms * (count - 1.0) + latency_ms as f64) / count;
        
        // 只在状态不是 Unhealthy 或 Cooldown 时才设置为 Healthy
        if self.status == ProviderHealthStatus::Unknown {
            self.status = ProviderHealthStatus::Healthy;
        }
    }

    /// 记录超时
    pub fn record_timeout(&mut self) {
        self.total_requests = self.total_requests.saturating_add(1);
        self.failures = self.failures.saturating_add(1);
        self.consecutive_timeouts = self.consecutive_timeouts.saturating_add(1);
        self.last_failure_time = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );
    }

    /// 记录失败（非超时）
    pub fn record_failure(&mut self) {
        self.total_requests = self.total_requests.saturating_add(1);
        self.failures = self.failures.saturating_add(1);
        self.consecutive_timeouts = 0;
        self.last_failure_time = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );
    }

    /// 检查是否可用
    pub fn is_available(&self) -> bool {
        match self.status {
            ProviderHealthStatus::Healthy | ProviderHealthStatus::Unknown => true,
            ProviderHealthStatus::Cooldown => {
                // 检查冷却是否已过期
                if let Some(until) = self.cooldown_until {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    now >= until
                } else {
                    true
                }
            }
            ProviderHealthStatus::Unhealthy => false,
        }
    }

    /// 标记为冷却
    pub fn enter_cooldown(&mut self, duration_ms: u64) {
        let until = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64 + duration_ms;
        self.cooldown_until = Some(until);
        self.status = ProviderHealthStatus::Cooldown;
    }

    /// 检查是否需要退出冷却
    pub fn check_cooldown_expired(&mut self) -> bool {
        if self.status != ProviderHealthStatus::Cooldown {
            return false;
        }
        
        if let Some(until) = self.cooldown_until {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            if now >= until {
                self.status = ProviderHealthStatus::Healthy;
                self.cooldown_until = None;
                return true;
            }
        }
        false
    }
}

/// 提供商健康监控器 - 线程安全
pub struct ProviderHealthMonitor {
    /// 提供商健康记录
    records: Arc<RwLock<HashMap<String, ProviderHealthRecord>>>,
    /// 超时配置
    config: TimeoutConfig,
    /// 每个任务的切换次数计数
    task_failover_counts: Arc<RwLock<HashMap<String, AtomicU32>>>,
}

impl Clone for ProviderHealthMonitor {
    fn clone(&self) -> Self {
        ProviderHealthMonitor {
            records: Arc::clone(&self.records),
            config: self.config.clone(),
            task_failover_counts: Arc::clone(&self.task_failover_counts),
        }
    }
}

impl ProviderHealthMonitor {
    /// 创建新的健康监控器
    pub fn new(config: TimeoutConfig) -> Self {
        ProviderHealthMonitor {
            records: Arc::new(RwLock::new(HashMap::new())),
            config,
            task_failover_counts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册提供商
    pub fn register_provider(&self, provider_id: String, reputation_score: f64) -> Result<(), String> {
        let mut records = write_lock(&self.records)?;
        let mut record = ProviderHealthRecord::new(provider_id);
        record.reputation_score = reputation_score;
        records.insert(record.provider_id.clone(), record);
        Ok(())
    }

    /// 获取提供商记录
    pub fn get_record(&self, provider_id: &str) -> Option<ProviderHealthRecord> {
        let records = read_lock(&self.records).ok()?;
        records.get(provider_id).cloned()
    }

    /// 记录成功
    pub fn record_success(&self, provider_id: &str, latency_ms: u64) -> Result<(), String> {
        let mut records = write_lock(&self.records)?;
        if let Some(record) = records.get_mut(provider_id) {
            record.record_success(latency_ms);
        }
        Ok(())
    }

    /// 记录超时
    pub fn record_timeout(&self, provider_id: &str) -> Result<(), String> {
        let mut records = write_lock(&self.records)?;
        if let Some(record) = records.get_mut(provider_id) {
            record.record_timeout();

            // 检查是否需要标记为不健康
            if record.consecutive_timeouts >= self.config.consecutive_timeout_threshold {
                record.status = ProviderHealthStatus::Unhealthy;
                record.enter_cooldown(self.config.cooldown_ms);
            }
        }
        Ok(())
    }

    /// 记录失败
    pub fn record_failure(&self, provider_id: &str) -> Result<(), String> {
        let mut records = write_lock(&self.records)?;
        if let Some(record) = records.get_mut(provider_id) {
            record.record_failure();
        }
        Ok(())
    }

    /// 获取所有可用提供商
    pub fn get_available_providers(&self) -> Result<Vec<String>, String> {
        let records = read_lock(&self.records)?;
        Ok(records.iter()
            .filter(|(_, record)| record.is_available())
            .map(|(id, _)| id.clone())
            .collect())
    }

    /// 选择最佳备用提供商
    ///
    /// 选择策略：
    /// 1. 必须可用
    /// 2. 不在冷却期
    /// 3. 按信誉评分排序
    /// 4. 考虑平均响应时间
    pub fn select_best_backup(
        &self,
        exclude_provider_id: Option<&str>,
    ) -> Option<String> {
        let records = read_lock(&self.records).ok()?;

        let mut candidates: Vec<&ProviderHealthRecord> = records.iter()
            .filter(|(id, record)| {
                // 排除指定的提供商
                if let Some(exclude) = exclude_provider_id {
                    if *id == exclude {
                        return false;
                    }
                }
                // 必须可用
                record.is_available()
            })
            .map(|(_, record)| record)
            .collect();
        
        if candidates.is_empty() {
            return None;
        }
        
        // 按信誉评分和延迟排序
        candidates.sort_by(|a, b| {
            // 优先按信誉评分降序
            b.reputation_score.partial_cmp(&a.reputation_score)
                .unwrap_or(std::cmp::Ordering::Equal)
            // 信誉相同时，按延迟升序
            .then_with(|| {
                a.avg_latency_ms.partial_cmp(&b.avg_latency_ms)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        });
        
        candidates.first().map(|r| r.provider_id.clone())
    }

    /// 获取或创建任务的切换计数
    pub fn get_failover_count(&self, task_id: &str) -> Result<u32, String> {
        let counts = read_lock(&self.task_failover_counts)?;
        Ok(counts.get(task_id)
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(0))
    }

    /// 增加任务切换计数
    pub fn increment_failover_count(&self, task_id: &str) -> Result<u32, String> {
        let mut counts = write_lock(&self.task_failover_counts)?;
        let count = counts.entry(task_id.to_string())
            .or_insert(AtomicU32::new(0));
        Ok(count.fetch_add(1, Ordering::SeqCst))
    }

    /// 重置任务切换计数
    pub fn reset_failover_count(&self, task_id: &str) -> Result<(), String> {
        let mut counts = write_lock(&self.task_failover_counts)?;
        counts.remove(task_id);
        Ok(())
    }

    /// 检查是否允许切换
    pub fn can_failover(&self, task_id: &str) -> Result<bool, String> {
        self.get_failover_count(task_id)
            .map(|count| count < self.config.max_failover_count)
    }

    /// 获取所有提供商状态（用于监控）
    pub fn get_all_status(&self) -> Result<HashMap<String, ProviderHealthStatus>, String> {
        let records = read_lock(&self.records)?;
        Ok(records.iter()
            .map(|(id, record)| (id.clone(), record.status.clone()))
            .collect())
    }

    /// 手动标记提供商健康状态
    pub fn set_status(&self, provider_id: &str, status: ProviderHealthStatus) -> Result<(), String> {
        let mut records = write_lock(&self.records)?;
        if let Some(record) = records.get_mut(provider_id) {
            record.status = status;
        }
        Ok(())
    }
}

/// 推理超时错误
#[derive(Debug, Clone, PartialEq)]
pub struct TimeoutError {
    /// 提供商 ID
    pub provider_id: String,
    /// 超时时间（毫秒）
    pub timeout_ms: u64,
    /// 实际经过时间（毫秒）
    pub elapsed_ms: u64,
    /// 任务 ID
    pub task_id: String,
}

impl std::fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "推理超时：provider={}, timeout={}ms, elapsed={}ms, task={}",
            self.provider_id, self.timeout_ms, self.elapsed_ms, self.task_id
        )
    }
}

impl std::error::Error for TimeoutError {}

/// 故障转移事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverEvent {
    /// 任务 ID
    pub task_id: String,
    /// 原提供商 ID
    pub from_provider: String,
    /// 新提供商 ID
    pub to_provider: String,
    /// 切换原因
    pub reason: FailoverReason,
    /// 时间戳
    pub timestamp: u64,
    /// 切换次数
    pub failover_count: u32,
}

/// 故障转移原因
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FailoverReason {
    /// 推理超时
    Timeout,
    /// 提供商宕机
    ProviderDown,
    /// 连续失败
    ConsecutiveFailures,
    /// 手动切换
    Manual,
    /// 健康检查失败
    HealthCheckFailed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_config_default() {
        let config = TimeoutConfig::default();
        assert_eq!(config.inference_timeout_ms, 30000);
        assert_eq!(config.cooldown_ms, 10000);
        assert_eq!(config.max_failover_count, 2);
    }

    #[test]
    fn test_provider_health_record_success() {
        let mut record = ProviderHealthRecord::new("provider_1".to_string());
        
        record.record_success(100);
        // 初始状态是 Unknown，record_success 会将其改为 Healthy
        assert_eq!(record.status, ProviderHealthStatus::Healthy);
        assert_eq!(record.total_requests, 1);
        assert_eq!(record.failures, 0);
        assert!(record.last_success_time.is_some());
    }

    #[test]
    fn test_provider_health_record_timeout() {
        // 使用 monitor 来测试，因为状态改变逻辑在 monitor 中
        let config = TimeoutConfig {
            consecutive_timeout_threshold: 3,
            cooldown_ms: 10000,
            ..Default::default()
        };
        let monitor = ProviderHealthMonitor::new(config);

        let _ = monitor.register_provider("provider_1".to_string(), 0.5);

        // 连续超时 3 次
        let _ = monitor.record_timeout("provider_1");
        let _ = monitor.record_timeout("provider_1");
        let _ = monitor.record_timeout("provider_1");

        let record = monitor.get_record("provider_1").unwrap();
        assert_eq!(record.consecutive_timeouts, 3);
        // 状态应该是 Cooldown（因为 enter_cooldown 会设置状态为 Cooldown）
        assert_eq!(record.status, ProviderHealthStatus::Cooldown);
        assert!(record.cooldown_until.is_some());
    }

    #[test]
    fn test_provider_health_monitor_selection() {
        let config = TimeoutConfig::default();
        let monitor = ProviderHealthMonitor::new(config);

        // 注册三个提供商
        let _ = monitor.register_provider("provider_1".to_string(), 0.9);
        let _ = monitor.register_provider("provider_2".to_string(), 0.7);
        let _ = monitor.register_provider("provider_3".to_string(), 0.95);

        // 选择最佳备用（排除 provider_3）
        let best = monitor.select_best_backup(Some("provider_3"));
        assert_eq!(best, Some("provider_1".to_string()));

        // 标记 provider_1 为不健康
        let _ = monitor.set_status("provider_1", ProviderHealthStatus::Unhealthy);

        // 再次选择，应该选择 provider_2
        let best = monitor.select_best_backup(Some("provider_3"));
        assert_eq!(best, Some("provider_2".to_string()));
    }

    #[test]
    fn test_failover_count_limit() {
        let config = TimeoutConfig::default();
        let monitor = ProviderHealthMonitor::new(config);

        let task_id = "task_1";

        // 初始应该允许切换
        assert!(monitor.can_failover(task_id).unwrap());

        // 达到最大切换次数
        monitor.increment_failover_count(task_id).unwrap();
        monitor.increment_failover_count(task_id).unwrap();

        // 不应该允许切换
        assert!(!monitor.can_failover(task_id).unwrap());

        // 重置后应该允许切换
        monitor.reset_failover_count(task_id).unwrap();
        assert!(monitor.can_failover(task_id).unwrap());
    }

    #[test]
    fn test_cooldown_expiry() {
        let mut record = ProviderHealthRecord::new("provider_1".to_string());
        
        // 进入冷却
        record.enter_cooldown(100); // 100ms 冷却
        
        // 冷却中不可用
        assert!(!record.is_available());
        
        // 等待冷却过期
        std::thread::sleep(Duration::from_millis(150));
        
        // 应该自动过期
        record.check_cooldown_expired();
        assert_eq!(record.status, ProviderHealthStatus::Healthy);
        assert!(record.is_available());
    }
}
