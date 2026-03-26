//! 故障切换服务 - 健康监控和故障转移
//!
//! **职责**：
//! - 监控推理提供商健康状态
//! - 检测超时和失败
//! - 执行故障切换（切换到备用提供商）
//! - 记录故障切换历史
//!
//! **不依赖**：
//! - 不直接执行推理（由 InferenceOrchestrator 负责）
//! - 不直接处理上链（由 CommitmentService 负责）

use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::Result;
use async_trait::async_trait;

use crate::failover::{
    ProviderHealthMonitor, ProviderHealthStatus,
    FailoverEvent, FailoverReason, TimeoutConfig,
};
use crate::provider_layer::ProviderLayerManager;
use crate::services::FailoverServiceTrait;

/// 故障切换服务
pub struct FailoverService {
    /// 提供商健康监控器
    monitor: Arc<RwLock<ProviderHealthMonitor>>,
    /// 提供商层管理器（使用 RwLock 包装，支持内部可变性）
    provider_layer: Arc<RwLock<ProviderLayerManager>>,
    /// 故障切换历史
    history: Arc<RwLock<Vec<FailoverEvent>>>,
}

impl FailoverService {
    /// 创建新的故障切换服务
    pub fn new(
        provider_layer: Arc<RwLock<ProviderLayerManager>>,
        timeout_config: TimeoutConfig,
    ) -> Self {
        let monitor = ProviderHealthMonitor::new(timeout_config);

        FailoverService {
            monitor: Arc::new(RwLock::new(monitor)),
            provider_layer,
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 获取监控器引用
    pub fn monitor(&self) -> Arc<RwLock<ProviderHealthMonitor>> {
        self.monitor.clone()
    }

    /// 标记提供商为健康
    pub fn mark_healthy(&self, provider_id: &str) -> Result<()> {
        let monitor = self.monitor
            .read()
            .map_err(|e| anyhow::anyhow!("Monitor lock poisoned: {}", e))?;

        monitor.set_status(provider_id, ProviderHealthStatus::Healthy)
            .map_err(|e| anyhow::anyhow!("Failed to mark provider healthy: {}", e))?;
        Ok(())
    }

    /// 标记提供商为不健康
    pub fn mark_unhealthy(&self, provider_id: &str, reason: &str) -> Result<()> {
        let monitor = self.monitor
            .read()
            .map_err(|e| anyhow::anyhow!("Monitor lock poisoned: {}", e))?;

        let status = match reason {
            "timeout" | "consecutive_timeouts" => ProviderHealthStatus::Unhealthy,
            "cooldown" => ProviderHealthStatus::Cooldown,
            _ => ProviderHealthStatus::Unhealthy,
        };

        monitor.set_status(provider_id, status)
            .map_err(|e| anyhow::anyhow!("Failed to mark provider unhealthy: {}", e))?;
        Ok(())
    }

    /// 记录故障切换事件
    fn record_failover(
        &self,
        from_provider: &str,
        to_provider: &str,
        reason: FailoverReason,
        task_id: &str,
    ) -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let failover_count = {
            let monitor = self.monitor
                .read()
                .map_err(|e| anyhow::anyhow!("Monitor lock poisoned: {}", e))?;
            monitor.get_failover_count(task_id).unwrap_or(0)
        };

        let event = FailoverEvent {
            task_id: task_id.to_string(),
            timestamp,
            from_provider: from_provider.to_string(),
            to_provider: to_provider.to_string(),
            reason,
            failover_count,
        };

        let mut history = self.history
            .write()
            .map_err(|e| anyhow::anyhow!("History lock poisoned: {}", e))?;

        history.push(event);
        Ok(())
    }

    /// 执行故障切换
    ///
    /// # 参数
    /// - `failed_provider_id`: 失败的提供商 ID
    /// - `new_provider_id`: 新的提供商 ID
    /// - `reason`: 故障原因
    /// - `task_id`: 任务 ID
    ///
    /// # 返回
    /// - `Ok(())`: 成功
    /// - `Err(anyhow::Error)`: 错误上下文
    pub fn failover(
        &self,
        failed_provider_id: &str,
        new_provider_id: &str,
        reason: FailoverReason,
        task_id: &str,
    ) -> Result<()> {
        // 标记旧提供商为不健康
        self.mark_unhealthy(failed_provider_id, &format!("{:?}", reason))?;

        // 增加切换计数
        let monitor = self.monitor
            .read()
            .map_err(|e| anyhow::anyhow!("Monitor lock poisoned: {}", e))?;
        monitor.increment_failover_count(task_id)
            .map_err(|e| anyhow::anyhow!("Failed to increment failover count: {}", e))?;

        // 切换提供商
        {
            let mut pl = self.provider_layer
                .write()
                .map_err(|e| anyhow::anyhow!("Provider layer lock poisoned: {}", e))?;
            pl.set_current_provider(new_provider_id)
                .map_err(|e| anyhow::anyhow!("Failed to switch provider: {}", e))?;
        }

        // 记录故障切换事件
        self.record_failover(failed_provider_id, new_provider_id, reason, task_id)?;

        Ok(())
    }

    /// 获取故障切换历史
    pub fn get_history(&self) -> Result<Vec<FailoverEvent>> {
        let history = self.history
            .read()
            .map_err(|e| anyhow::anyhow!("History lock poisoned: {}", e))?;

        Ok(history.clone())
    }

    /// 获取故障切换次数
    pub fn failover_count(&self) -> Result<usize> {
        let history = self.history
            .read()
            .map_err(|e| anyhow::anyhow!("History lock poisoned: {}", e))?;

        Ok(history.len())
    }

    /// 检查提供商是否健康
    pub fn is_healthy(&self, provider_id: &str) -> bool {
        let monitor = self.monitor
            .read()
            .expect("Monitor lock poisoned");

        monitor.get_record(provider_id)
            .map(|r| r.status == ProviderHealthStatus::Healthy)
            .unwrap_or(false)
    }

    /// 获取所有健康提供商列表
    pub fn get_healthy_providers(&self) -> Vec<String> {
        let monitor = self.monitor
            .read()
            .expect("Monitor lock poisoned");

        monitor.get_available_providers().unwrap_or_default()
    }
}

#[async_trait]
impl FailoverServiceTrait for FailoverService {
    /// 执行故障切换
    async fn failover(&self, from: &str, to: &str, reason: FailoverReason) -> Result<()> {
        // 标记旧提供商为不健康
        self.mark_unhealthy(from, &format!("{:?}", reason))?;

        // 切换提供商
        {
            let mut pl = self.provider_layer
                .write()
                .map_err(|e| anyhow::anyhow!("Provider layer lock poisoned: {}", e))?;
            pl.set_current_provider(to)
                .map_err(|e| anyhow::anyhow!("Failed to switch provider: {}", e))?;
        }

        // 记录故障切换事件（使用空任务 ID，因为 trait 没有提供）
        self.record_failover(from, to, reason, "unknown_task")?;

        Ok(())
    }

    /// 获取健康提供商列表
    fn get_healthy_providers(&self) -> Vec<String> {
        self.get_healthy_providers()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_failover_service_creation() {
        let provider_layer = Arc::new(RwLock::new(ProviderLayerManager::new()));
        let service = FailoverService::new(provider_layer, TimeoutConfig::default());

        assert_eq!(service.failover_count().unwrap(), 0);
    }

    #[test]
    fn test_mark_healthy_unhealthy() {
        let provider_layer = Arc::new(RwLock::new(ProviderLayerManager::new()));
        let service = FailoverService::new(provider_layer, TimeoutConfig::default());

        let provider_id = "provider_1";

        // 初始状态应该是未知（如果没有注册）
        // 先注册提供商
        let monitor = service.monitor();
        let m = monitor.read().unwrap();
        let _ = m.register_provider(provider_id.to_string(), 0.5);
        drop(m);

        // 标记为不健康
        assert!(service.mark_unhealthy(provider_id, "test reason").is_ok());

        // 再次标记为健康
        assert!(service.mark_healthy(provider_id).is_ok());
    }
}
