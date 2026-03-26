//! 验证器信誉管理器 - P2-3：验证器监督
//!
//! **设计目标**：
//! - 管理验证节点的信誉评分
//! - 基于验证质量动态调整信誉
//! - 检测并惩罚恶意验证器
//! - 与质量感知共识集成
//!
//! **核心功能**：
//! - 验证器注册和注销
//! - 信誉分计算和更新
//! - 验证质量追踪
//! - 自动惩罚执行
//!
//! **信誉评分维度**：
//! 1. **验证准确性** - 验证结果与最终判定的一致性
//! 2. **响应及时性** - 验证提交的延迟
//! 3. **历史可靠性** - 长期验证表现
//! 4. **同行评分** - 其他验证器的评价

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::{Result, Context};
use tracing::{info, warn, debug, instrument};
use serde::{Serialize, Deserialize};

/// 验证器信誉配置
#[derive(Debug, Clone)]
pub struct ValidatorReputationConfig {
    /// 初始信誉分
    pub initial_reputation: f64,
    /// 最小信誉分
    pub min_reputation: f64,
    /// 最大信誉分
    pub max_reputation: f64,
    /// 验证准确的奖励
    pub accurate_validation_reward: f64,
    /// 验证错误的惩罚
    pub inaccurate_validation_penalty: f64,
    /// 超时惩罚
    pub timeout_penalty: f64,
    /// 恶意行为惩罚
    pub malicious_behavior_penalty: f64,
    /// 信誉衰减因子（防止长期不活跃）
    pub decay_factor: f64,
    /// 衰减周期（毫秒）
    pub decay_period_ms: u64,
    /// 历史窗口大小
    pub history_window_size: usize,
}

impl Default for ValidatorReputationConfig {
    fn default() -> Self {
        ValidatorReputationConfig {
            initial_reputation: 0.5,
            min_reputation: 0.0,
            max_reputation: 1.0,
            accurate_validation_reward: 0.02,
            inaccurate_validation_penalty: 0.05,
            timeout_penalty: 0.03,
            malicious_behavior_penalty: 0.2,
            decay_factor: 0.99,
            decay_period_ms: 86400000, // 24 小时
            history_window_size: 100,
        }
    }
}

/// 验证器信誉记录
#[derive(Debug, Clone)]
pub struct ValidatorReputationRecord {
    /// 验证器 ID
    pub validator_id: String,
    /// 当前信誉分
    pub reputation_score: f64,
    /// 总验证次数
    pub total_validations: u64,
    /// 准确验证次数
    pub accurate_validations: u64,
    /// 平均响应时间（毫秒）
    pub avg_response_time_ms: f64,
    /// 注册时间戳
    pub registered_at: u64,
    /// 最后活跃时间戳
    pub last_active_at: u64,
    /// 信誉状态
    pub status: ValidatorStatus,
}

/// 验证器状态
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ValidatorStatus {
    /// 活跃
    Active,
    /// 暂停（信誉过低）
    Suspended,
    /// 冻结（恶意行为）
    Frozen,
    /// 注销
    Deregistered,
}

/// 验证历史事件
#[derive(Debug, Clone)]
pub struct ValidationEvent {
    /// 事件 ID
    pub event_id: String,
    /// 请求 ID
    pub request_id: String,
    /// 验证器 ID
    pub validator_id: String,
    /// 事件类型
    pub event_type: ValidationEventType,
    /// 信誉变化
    pub reputation_delta: f64,
    /// 时间戳
    pub timestamp: u64,
    /// 详情
    pub details: String,
}

/// 验证事件类型
#[derive(Debug, Clone)]
pub enum ValidationEventType {
    /// 验证提交
    ValidationSubmitted,
    /// 验证准确
    ValidationAccurate,
    /// 验证错误
    ValidationInaccurate,
    /// 验证超时
    ValidationTimeout,
    /// 恶意行为检测
    MaliciousBehaviorDetected,
    /// 信誉衰减
    ReputationDecay,
}

/// 验证器性能指标
#[derive(Debug, Clone, Default)]
pub struct ValidatorMetrics {
    /// 验证准确率
    pub accuracy_rate: f64,
    /// 平均响应时间
    pub avg_response_time_ms: f64,
    /// 验证总数
    pub total_validations: u64,
    /// 最近 10 次验证准确率
    pub recent_accuracy_rate: f64,
    /// 信誉趋势（正数为上升）
    pub reputation_trend: f64,
}

/// 验证器信誉管理器
///
/// **核心职责**：
/// - 管理验证器注册和信誉
/// - 追踪验证历史
/// - 计算和更新信誉分
/// - 执行自动惩罚
pub struct ValidatorReputationManager {
    /// 配置
    config: ValidatorReputationConfig,
    /// 验证器信誉映射
    validator_reputations: Arc<RwLock<HashMap<String, ValidatorReputationRecord>>>,
    /// 验证历史（按验证器 ID 索引）
    validation_history: Arc<RwLock<HashMap<String, VecDeque<ValidationEvent>>>>,
    /// 系统启动时间
    #[allow(dead_code)]
    started_at: u64,
}

impl ValidatorReputationManager {
    /// 创建新的验证器信誉管理器
    pub fn new(config: ValidatorReputationConfig) -> Self {
        ValidatorReputationManager {
            config,
            validator_reputations: Arc::new(RwLock::new(HashMap::new())),
            validation_history: Arc::new(RwLock::new(HashMap::new())),
            started_at: current_timestamp(),
        }
    }

    /// 注册验证器
    pub async fn register_validator(&self, validator_id: &str) -> Result<()> {
        let mut reputations = self.validator_reputations.write().await;
        
        if reputations.contains_key(validator_id) {
            return Err(anyhow::anyhow!("Validator {} already registered", validator_id));
        }

        let now = current_timestamp();
        let record = ValidatorReputationRecord {
            validator_id: validator_id.to_string(),
            reputation_score: self.config.initial_reputation,
            total_validations: 0,
            accurate_validations: 0,
            avg_response_time_ms: 0.0,
            registered_at: now,
            last_active_at: now,
            status: ValidatorStatus::Active,
        };

        reputations.insert(validator_id.to_string(), record);
        self.validation_history.write().await.insert(
            validator_id.to_string(),
            VecDeque::with_capacity(self.config.history_window_size),
        );

        info!("Registered validator: {} with initial reputation: {}", validator_id, self.config.initial_reputation);

        Ok(())
    }

    /// 注销验证器
    pub async fn deregister_validator(&self, validator_id: &str) -> Result<()> {
        let mut reputations = self.validator_reputations.write().await;
        
        let record = reputations
            .get_mut(validator_id)
            .context("Validator not found")?;

        record.status = ValidatorStatus::Deregistered;
        info!("Deregistered validator: {}", validator_id);

        Ok(())
    }

    /// 更新验证器信誉（验证完成后调用）
    #[instrument(skip(self), fields(validator_id = %validator_id, request_id = %request_id))]
    pub async fn update_reputation(
        &self,
        validator_id: &str,
        request_id: &str,
        is_accurate: bool,
        response_time_ms: f64,
    ) -> Result<f64> {
        let mut reputations = self.validator_reputations.write().await;
        
        let record = reputations
            .get_mut(validator_id)
            .context("Validator not found")?;

        if record.status != ValidatorStatus::Active {
            return Err(anyhow::anyhow!(
                "Validator {} is not active (status: {:?}",
                validator_id,
                record.status
            ));
        }

        // 计算信誉变化
        let delta = if is_accurate {
            self.config.accurate_validation_reward
        } else {
            -self.config.inaccurate_validation_penalty
        };

        // 更新信誉分
        record.reputation_score = (record.reputation_score + delta)
            .clamp(self.config.min_reputation, self.config.max_reputation);

        // 更新统计
        record.total_validations += 1;
        if is_accurate {
            record.accurate_validations += 1;
        }

        // 更新平均响应时间
        record.avg_response_time_ms = (record.avg_response_time_ms
            * (record.total_validations - 1) as f64
            + response_time_ms)
            / record.total_validations as f64;

        record.last_active_at = current_timestamp();

        // 检查是否需要暂停
        if record.reputation_score < 0.2 {
            record.status = ValidatorStatus::Suspended;
            warn!("Validator {} suspended due to low reputation: {}", validator_id, record.reputation_score);
        }

        // 记录历史事件
        self.record_validation_event(
            validator_id,
            request_id,
            if is_accurate {
                ValidationEventType::ValidationAccurate
            } else {
                ValidationEventType::ValidationInaccurate
            },
            delta,
            &format!("Validation {}", if is_accurate { "accurate" } else { "inaccurate" }),
        ).await;

        debug!(
            "Updated reputation for validator {}: {} -> {} (delta: {})",
            validator_id,
            record.reputation_score - delta,
            record.reputation_score,
            delta
        );

        Ok(record.reputation_score)
    }

    /// 报告恶意行为
    pub async fn report_malicious_behavior(
        &self,
        validator_id: &str,
        reason: &str,
    ) -> Result<f64> {
        let mut reputations = self.validator_reputations.write().await;
        
        let record = reputations
            .get_mut(validator_id)
            .context("Validator not found")?;

        // 应用恶意行为惩罚
        record.reputation_score = (record.reputation_score - self.config.malicious_behavior_penalty)
            .clamp(self.config.min_reputation, self.config.max_reputation);

        // 冻结验证器
        record.status = ValidatorStatus::Frozen;
        record.last_active_at = current_timestamp();

        // 记录历史事件
        self.record_validation_event(
            validator_id,
            "malicious_behavior_report",
            ValidationEventType::MaliciousBehaviorDetected,
            -self.config.malicious_behavior_penalty,
            &format!("Malicious behavior: {}", reason),
        ).await;

        warn!(
            "Validator {} frozen due to malicious behavior: {} (reputation: {})",
            validator_id, reason, record.reputation_score
        );

        Ok(record.reputation_score)
    }

    /// 获取验证器信誉分
    pub async fn get_reputation(&self, validator_id: &str) -> Option<f64> {
        let reputations = self.validator_reputations.read().await;
        reputations.get(validator_id).map(|r| r.reputation_score)
    }

    /// 获取验证器状态
    pub async fn get_status(&self, validator_id: &str) -> Option<ValidatorStatus> {
        let reputations = self.validator_reputations.read().await;
        reputations.get(validator_id).map(|r| r.status)
    }

    /// 获取验证器指标
    pub async fn get_metrics(&self, validator_id: &str) -> Option<ValidatorMetrics> {
        let reputations = self.validator_reputations.read().await;
        let history = self.validation_history.read().await;
        
        let record = reputations.get(validator_id)?;
        let events = history.get(validator_id)?;

        // 计算准确率
        let accuracy_rate = if record.total_validations > 0 {
            record.accurate_validations as f64 / record.total_validations as f64
        } else {
            0.0
        };

        // 计算最近 10 次准确率
        let recent_events: Vec<_> = events.iter().take(10).collect();
        let recent_accurate = recent_events
            .iter()
            .filter(|e| matches!(e.event_type, ValidationEventType::ValidationAccurate))
            .count();
        let recent_accuracy_rate = if !recent_events.is_empty() {
            recent_accurate as f64 / recent_events.len() as f64
        } else {
            accuracy_rate
        };

        // 计算信誉趋势
        let reputation_trend = self.calculate_reputation_trend(events).await;

        Some(ValidatorMetrics {
            accuracy_rate,
            avg_response_time_ms: record.avg_response_time_ms,
            total_validations: record.total_validations,
            recent_accuracy_rate,
            reputation_trend,
        })
    }

    /// 获取所有活跃验证器
    pub async fn get_active_validators(&self) -> Vec<String> {
        let reputations = self.validator_reputations.read().await;
        reputations
            .iter()
            .filter(|(_, r)| r.status == ValidatorStatus::Active)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// 获取按信誉排序的验证器列表
    pub async fn get_validators_by_reputation(&self) -> Vec<(String, f64)> {
        let reputations = self.validator_reputations.read().await;
        let mut validators: Vec<_> = reputations
            .iter()
            .filter(|(_, r)| r.status == ValidatorStatus::Active)
            .map(|(id, r)| (id.clone(), r.reputation_score))
            .collect();

        // 按信誉降序排序
        validators.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        validators
    }

    /// 应用信誉衰减（定期调用）
    pub async fn apply_decay(&self) -> Result<usize> {
        let mut reputations = self.validator_reputations.write().await;
        let now = current_timestamp();
        let mut decayed_count = 0;

        for record in reputations.values_mut() {
            if record.status != ValidatorStatus::Active {
                continue;
            }

            let elapsed = now - record.last_active_at;
            if elapsed >= self.config.decay_period_ms {
                record.reputation_score *= self.config.decay_factor;
                record.reputation_score = record.reputation_score.max(self.config.min_reputation);

                self.record_validation_event(
                    &record.validator_id,
                    "decay",
                    ValidationEventType::ReputationDecay,
                    record.reputation_score * (self.config.decay_factor - 1.0),
                    "Periodic reputation decay",
                ).await;

                decayed_count += 1;
            }
        }

        if decayed_count > 0 {
            debug!("Applied reputation decay to {} validators", decayed_count);
        }

        Ok(decayed_count)
    }

    /// 恢复被冻结的验证器（需要治理决策）
    pub async fn restore_validator(&self, validator_id: &str) -> Result<f64> {
        let mut reputations = self.validator_reputations.write().await;
        
        let record = reputations
            .get_mut(validator_id)
            .context("Validator not found")?;

        if record.status != ValidatorStatus::Frozen {
            return Err(anyhow::anyhow!("Validator is not frozen"));
        }

        // 恢复到初始信誉
        record.reputation_score = self.config.initial_reputation;
        record.status = ValidatorStatus::Active;
        record.last_active_at = current_timestamp();

        info!("Restored validator: {} with reputation: {}", validator_id, record.reputation_score);

        Ok(record.reputation_score)
    }

    // ========== 内部方法 ==========

    async fn record_validation_event(
        &self,
        validator_id: &str,
        request_id: &str,
        event_type: ValidationEventType,
        delta: f64,
        details: &str,
    ) {
        let mut history = self.validation_history.write().await;
        let queue = history
            .entry(validator_id.to_string())
            .or_insert_with(|| VecDeque::with_capacity(self.config.history_window_size));

        let event = ValidationEvent {
            event_id: generate_event_id(),
            request_id: request_id.to_string(),
            validator_id: validator_id.to_string(),
            event_type,
            reputation_delta: delta,
            timestamp: current_timestamp(),
            details: details.to_string(),
        };

        queue.push_front(event);

        // 保持窗口大小
        while queue.len() > self.config.history_window_size {
            queue.pop_back();
        }
    }

    async fn calculate_reputation_trend(&self, events: &VecDeque<ValidationEvent>) -> f64 {
        if events.len() < 2 {
            return 0.0;
        }

        let recent: Vec<_> = events.iter().take(10).collect();
        if recent.len() < 2 {
            return 0.0;
        }

        // 简单线性回归斜率
        let n = recent.len() as f64;
        let sum_x = (0..recent.len()).map(|i| i as f64).sum::<f64>();
        let sum_y = recent.iter().map(|e| e.reputation_delta).sum::<f64>();
        let sum_xy = recent.iter().enumerate().map(|(i, e)| i as f64 * e.reputation_delta).sum::<f64>();
        let sum_x2 = (0..recent.len()).map(|i| (i as f64).powi(2)).sum::<f64>();

        let denominator = n * sum_x2 - sum_x.powi(2);
        if denominator.abs() < 1e-10 {
            return 0.0;
        }

        (n * sum_xy - sum_x * sum_y) / denominator
    }
}

// 辅助函数

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn generate_event_id() -> String {
    use sha2::{Sha256, Digest};
    let timestamp = current_timestamp();
    let random = rand::random::<u64>();
    let data = format!("{}:{}", timestamp, random);
    let hash = Sha256::digest(data.as_bytes());
    format!("event_{:x}", hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_validator_registration() {
        let manager = ValidatorReputationManager::new(ValidatorReputationConfig::default());
        
        manager.register_validator("validator_1").await.unwrap();
        
        let reputation = manager.get_reputation("validator_1").await;
        assert_eq!(reputation, Some(0.5));

        let status = manager.get_status("validator_1").await;
        assert_eq!(status, Some(ValidatorStatus::Active));
    }

    #[tokio::test]
    async fn test_reputation_update() {
        let manager = ValidatorReputationManager::new(ValidatorReputationConfig::default());
        
        manager.register_validator("validator_1").await.unwrap();

        // 准确验证
        let new_rep = manager.update_reputation("validator_1", "req_1", true, 100.0).await.unwrap();
        assert_eq!(new_rep, 0.52);

        // 错误验证
        let new_rep = manager.update_reputation("validator_1", "req_2", false, 150.0).await.unwrap();
        assert_eq!(new_rep, 0.47);
    }

    #[tokio::test]
    async fn test_malicious_behavior() {
        let manager = ValidatorReputationManager::new(ValidatorReputationConfig::default());
        
        manager.register_validator("validator_1").await.unwrap();

        let new_rep = manager.report_malicious_behavior("validator_1", "Fake validation").await.unwrap();
        assert_eq!(new_rep, 0.3);

        let status = manager.get_status("validator_1").await;
        assert_eq!(status, Some(ValidatorStatus::Frozen));
    }

    #[tokio::test]
    async fn test_get_active_validators() {
        let manager = ValidatorReputationManager::new(ValidatorReputationConfig::default());
        
        manager.register_validator("validator_1").await.unwrap();
        manager.register_validator("validator_2").await.unwrap();
        manager.register_validator("validator_3").await.unwrap();

        // 冻结一个验证器
        manager.report_malicious_behavior("validator_2", "Test").await.ok();

        let active = manager.get_active_validators().await;
        assert_eq!(active.len(), 2);
        assert!(active.contains(&"validator_1".to_string()));
        assert!(!active.contains(&"validator_2".to_string()));
        assert!(active.contains(&"validator_3".to_string()));
    }
}
