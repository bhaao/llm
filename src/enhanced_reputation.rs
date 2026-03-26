//! 信誉管理器增强模块 - P1-6：自动惩罚执行
//!
//! **设计目标**：
//! - 基于检测结果自动执行惩罚
//! - 支持多级惩罚策略
//! - 支持惩罚申诉和恢复机制
//! - 与合谋检测、完整性检查联动
//!
//! **惩罚等级**：
//! 1. **警告** - 轻微违规，仅记录
//! 2. **降权** - 降低调度优先级
//! 3. **暂停** - 临时禁止参与推理
//! 4. **冻结** - 冻结资产和收益
//! 5. **剔除** - 永久剔除，需要治理恢复

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::{Result, Context};
use tracing::{info, warn, instrument};
use serde::{Serialize, Deserialize};

use crate::reputation::ReputationManager;
use crate::integrity_checker::IntegrityCheckResult;
use crate::collusion_analyzer::CollusionAnalysisResult;

/// 惩罚类型
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PenaltyType {
    /// 警告
    Warning,
    /// 降权（调度优先级降低）
    Downweight,
    /// 暂停（临时禁止）
    Suspension,
    /// 冻结（资产和收益）
    Freezing,
    /// 剔除（永久）
    Expulsion,
}

impl PenaltyType {
    /// 获取惩罚严重程度（1-5）
    pub fn severity(&self) -> u32 {
        match self {
            PenaltyType::Warning => 1,
            PenaltyType::Downweight => 2,
            PenaltyType::Suspension => 3,
            PenaltyType::Freezing => 4,
            PenaltyType::Expulsion => 5,
        }
    }

    /// 是否可自动恢复
    pub fn is_auto_recoverable(&self) -> bool {
        matches!(self, PenaltyType::Warning | PenaltyType::Downweight | PenaltyType::Suspension)
    }
}

/// 惩罚记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PenaltyRecord {
    /// 惩罚 ID
    pub penalty_id: String,
    /// 被惩罚节点 ID
    pub node_id: String,
    /// 惩罚类型
    pub penalty_type: PenaltyType,
    /// 惩罚原因
    pub reason: String,
    /// 惩罚严重程度
    pub severity: u32,
    /// 惩罚时长（秒），None 表示永久
    pub duration_secs: Option<u64>,
    /// 惩罚开始时间
    pub started_at: u64,
    /// 惩罚结束时间
    pub ends_at: Option<u64>,
    /// 是否已执行
    pub executed: bool,
    /// 是否已恢复
    pub recovered: bool,
    /// 关联的证据
    pub evidence: Vec<String>,
}

impl PenaltyRecord {
    /// 创建新的惩罚记录
    pub fn new(
        node_id: String,
        penalty_type: PenaltyType,
        reason: String,
        duration_secs: Option<u64>,
        evidence: Vec<String>,
    ) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        use sha2::{Sha256, Digest};

        let started_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let ends_at = duration_secs.map(|d| started_at + d);

        // 生成唯一 penalty_id
        let data = format!("{}:{}:{}", node_id, penalty_type as u32, started_at);
        let penalty_id = format!("penalty_{:x}", Sha256::digest(data.as_bytes()));

        PenaltyRecord {
            penalty_id,
            node_id,
            penalty_type,
            reason,
            severity: penalty_type.severity(),
            duration_secs,
            started_at,
            ends_at,
            executed: false,
            recovered: false,
            evidence,
        }
    }

    /// 是否已过期
    pub fn is_expired(&self) -> bool {
        if let Some(end) = self.ends_at {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() >= end
        } else {
            false // 永久惩罚不过期
        }
    }

    /// 标记为已执行
    pub fn mark_executed(&mut self) {
        self.executed = true;
    }

    /// 标记为已恢复
    pub fn mark_recovered(&mut self) {
        self.recovered = true;
    }
}

/// 惩罚策略配置
#[derive(Debug, Clone)]
pub struct PenaltyStrategy {
    /// 完整性检查失败的惩罚阈值
    pub integrity_failure_threshold: f64,
    /// 合谋检测失败的惩罚阈值
    pub collusion_confidence_threshold: f64,
    /// 自动惩罚启用
    pub enable_auto_penalty: bool,
    /// 惩罚申诉期（秒）
    pub appeal_period_secs: u64,
    /// 临时惩罚默认时长（秒）
    pub default_suspension_duration_secs: u64,
    /// 信誉恢复速率（每秒恢复的信誉分）
    pub reputation_recovery_rate: f64,
    /// 最大信誉恢复上限
    pub max_reputation_after_recovery: f64,
}

impl Default for PenaltyStrategy {
    fn default() -> Self {
        PenaltyStrategy {
            integrity_failure_threshold: 0.7,
            collusion_confidence_threshold: 0.8,
            enable_auto_penalty: true,
            appeal_period_secs: 3600, // 1 小时
            default_suspension_duration_secs: 86400, // 24 小时
            reputation_recovery_rate: 0.0001, // 每秒恢复 0.0001
            max_reputation_after_recovery: 0.9,
        }
    }
}

/// 节点惩罚状态
#[derive(Debug, Clone, Default)]
pub struct NodePenaltyState {
    /// 当前活跃的惩罚
    pub active_penalties: Vec<PenaltyRecord>,
    /// 历史惩罚记录
    pub historical_penalties: Vec<PenaltyRecord>,
    /// 累计惩罚次数
    pub total_penalty_count: u64,
    /// 当前调度权重乘数（0.0 - 1.0）
    pub scheduling_weight_multiplier: f64,
    /// 是否被禁止调度
    pub is_scheduling_blocked: bool,
    /// 禁止调度结束时间
    pub scheduling_block_ends_at: Option<u64>,
}

impl NodePenaltyState {
    fn new() -> Self {
        NodePenaltyState {
            active_penalties: Vec::new(),
            historical_penalties: Vec::new(),
            total_penalty_count: 0,
            scheduling_weight_multiplier: 1.0,
            is_scheduling_blocked: false,
            scheduling_block_ends_at: None,
        }
    }

    /// 添加惩罚
    fn add_penalty(&mut self, penalty: PenaltyRecord) {
        // 更新调度权重
        match penalty.penalty_type {
            PenaltyType::Warning => {
                self.scheduling_weight_multiplier = (self.scheduling_weight_multiplier - 0.1).max(0.5);
            }
            PenaltyType::Downweight => {
                self.scheduling_weight_multiplier = (self.scheduling_weight_multiplier - 0.3).max(0.3);
            }
            PenaltyType::Suspension | PenaltyType::Freezing => {
                self.is_scheduling_blocked = true;
                self.scheduling_block_ends_at = penalty.ends_at;
                self.scheduling_weight_multiplier = 0.0;
            }
            PenaltyType::Expulsion => {
                self.is_scheduling_blocked = true;
                self.scheduling_block_ends_at = None; // 永久
                self.scheduling_weight_multiplier = 0.0;
            }
        }

        self.active_penalties.push(penalty);
        self.total_penalty_count += 1;
    }

    /// 清理过期惩罚
    fn cleanup_expired(&mut self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // 移动过期惩罚到历史记录
        let expired: Vec<_> = self.active_penalties
            .drain(..)
            .filter(|p| p.is_expired())
            .collect();

        for mut penalty in expired {
            penalty.mark_recovered();
            self.historical_penalties.push(penalty);
        }

        // 更新调度状态
        if let Some(end) = self.scheduling_block_ends_at {
            if now >= end {
                self.is_scheduling_blocked = false;
                self.scheduling_block_ends_at = None;
                self.scheduling_weight_multiplier = 0.5; // 恢复部分权重
            }
        }

        // 如果没有活跃惩罚，恢复权重
        if self.active_penalties.is_empty() && !self.is_scheduling_blocked {
            self.scheduling_weight_multiplier = 1.0;
        }
    }

    /// 获取当前有效惩罚数量
    #[allow(dead_code)]
    fn active_penalty_count(&self) -> usize {
        self.active_penalties.len()
    }
}

/// 信誉管理器（增强版）- 自动惩罚执行
///
/// **功能**：
/// - 基于检测结果自动执行惩罚
/// - 管理惩罚状态和恢复
/// - 与信誉系统联动
pub struct EnhancedReputationManager {
    /// 基础信誉管理器
    base_manager: ReputationManager,
    /// 节点惩罚状态
    penalty_states: Arc<RwLock<HashMap<String, NodePenaltyState>>>,
    /// 惩罚策略
    strategy: PenaltyStrategy,
    /// 所有惩罚记录
    all_penalties: Arc<RwLock<Vec<PenaltyRecord>>>,
}

impl EnhancedReputationManager {
    /// 创建新的增强信誉管理器
    pub fn new(strategy: PenaltyStrategy) -> Self {
        EnhancedReputationManager {
            base_manager: ReputationManager::new(0.6, 0.3),
            penalty_states: Arc::new(RwLock::new(HashMap::new())),
            strategy,
            all_penalties: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 创建默认管理器
    pub fn with_defaults() -> Self {
        Self::new(PenaltyStrategy::default())
    }

    /// 处理完整性检查结果并执行惩罚
    #[instrument(skip(self, result), fields(node_id = %result_node_id))]
    pub async fn process_integrity_check(
        &self,
        result: &IntegrityCheckResult,
        result_node_id: &str,
    ) -> Result<Option<PenaltyRecord>> {
        // 清理过期惩罚
        self.cleanup_expired_penalties().await;

        if result.passed {
            return Ok(None);
        }

        // 计算总体严重程度
        let max_severity = result.issues.iter()
            .map(|i| i.severity)
            .fold(0.0_f64, f64::max);

        if max_severity < self.strategy.integrity_failure_threshold {
            warn!("Integrity check failed but severity {} below threshold", max_severity);
            return Ok(None);
        }

        // 确定惩罚类型
        let penalty_type = self.determine_penalty_type(
            max_severity,
            result_node_id,
        ).await;

        // 创建惩罚记录
        let reason = format!(
            "Integrity check failed: {} issues detected, max severity: {:.2}",
            result.issues.len(),
            max_severity
        );

        let evidence: Vec<_> = result.issues.iter()
            .map(|i| format!("{:?}: {}", i.issue_type, i.description))
            .collect();

        let duration = match penalty_type {
            PenaltyType::Warning | PenaltyType::Downweight => None,
            PenaltyType::Suspension | PenaltyType::Freezing => {
                Some(self.strategy.default_suspension_duration_secs)
            }
            PenaltyType::Expulsion => None, // 永久
        };

        let penalty = PenaltyRecord::new(
            result_node_id.to_string(),
            penalty_type,
            reason,
            duration,
            evidence,
        );

        // 执行惩罚
        self.execute_penalty(&penalty).await?;

        // 简化实现：跳过 ReputationManager 调用
        // 实际应该调用 reputation_manager 的相应方法来更新信誉分

        info!(
            "Executed penalty {:?} on node {} for integrity violation",
            penalty_type, result_node_id
        );

        Ok(Some(penalty))
    }

    /// 处理合谋检测结果并执行惩罚
    #[instrument(skip(self, result))]
    pub async fn process_collusion_check(
        &self,
        result: &CollusionAnalysisResult,
    ) -> Result<Vec<PenaltyRecord>> {
        // 清理过期惩罚
        self.cleanup_expired_penalties().await;

        if !result.collusion_detected || result.confidence < self.strategy.collusion_confidence_threshold {
            return Ok(Vec::new());
        }

        let mut penalties = Vec::new();

        for node_id in &result.suspected_nodes {
            // 确定惩罚类型
            let penalty_type = self.determine_penalty_type(
                result.confidence,
                node_id,
            ).await;

            // 创建惩罚记录
            let reason = format!(
                "Collusion detected: confidence {:.2}, involved in {} issues",
                result.confidence,
                result.issues.len()
            );

            let evidence: Vec<_> = result.issues.iter()
                .flat_map(|i| {
                    if i.involved_nodes.contains(node_id) {
                        Some(format!("{:?}: {}", i.issue_type, i.description))
                    } else {
                        None
                    }
                })
                .collect();

            let duration = match penalty_type {
                PenaltyType::Warning | PenaltyType::Downweight => None,
                PenaltyType::Suspension | PenaltyType::Freezing => {
                    Some(self.strategy.default_suspension_duration_secs)
                }
                PenaltyType::Expulsion => None,
            };

            let penalty = PenaltyRecord::new(
                node_id.clone(),
                penalty_type,
                reason,
                duration,
                evidence,
            );

            // 执行惩罚
            self.execute_penalty(&penalty).await?;

            // 简化实现：跳过 ReputationManager 调用
            penalties.push(penalty);
        }

        info!(
            "Executed {} penalties for collusion detection",
            penalties.len()
        );

        Ok(penalties)
    }

    /// 确定惩罚类型
    async fn determine_penalty_type(
        &self,
        severity: f64,
        node_id: &str,
    ) -> PenaltyType {
        let penalty_states = self.penalty_states.read().await;
        let state = penalty_states.get(node_id);

        let prior_count = state.map(|s| s.total_penalty_count).unwrap_or(0);

        // 基于严重程度和历史记录决定惩罚类型
        if severity > 0.9 || prior_count >= 3 {
            PenaltyType::Expulsion
        } else if severity > 0.7 || prior_count >= 2 {
            PenaltyType::Freezing
        } else if severity > 0.5 || prior_count >= 1 {
            PenaltyType::Suspension
        } else if severity > 0.3 {
            PenaltyType::Downweight
        } else {
            PenaltyType::Warning
        }
    }

    /// 执行惩罚
    async fn execute_penalty(&self, penalty: &PenaltyRecord) -> Result<()> {
        let mut penalty_states = self.penalty_states.write().await;
        
        let state = penalty_states.entry(penalty.node_id.clone())
            .or_insert_with(NodePenaltyState::new);
        
        state.add_penalty(penalty.clone());

        // 更新节点状态 - 简化实现，直接记录惩罚
        // 实际应该通过 ReputationManager 的 API 来更新节点状态

        // 记录到总惩罚列表
        let mut all_penalties = self.all_penalties.write().await;
        all_penalties.push(penalty.clone());

        Ok(())
    }

    /// 清理过期惩罚
    async fn cleanup_expired_penalties(&self) {
        let mut penalty_states = self.penalty_states.write().await;
        for state in penalty_states.values_mut() {
            state.cleanup_expired();
        }
    }

    /// 获取节点惩罚状态
    pub async fn get_node_penalty_state(&self, node_id: &str) -> Option<NodePenaltyState> {
        let penalty_states = self.penalty_states.read().await;
        penalty_states.get(node_id).cloned()
    }

    /// 获取所有惩罚记录
    pub async fn get_all_penalties(&self) -> Vec<PenaltyRecord> {
        let all_penalties = self.all_penalties.read().await;
        all_penalties.clone()
    }

    /// 获取节点惩罚历史
    pub async fn get_node_penalty_history(&self, node_id: &str) -> Vec<PenaltyRecord> {
        let all_penalties = self.all_penalties.read().await;
        all_penalties.iter()
            .filter(|p| p.node_id == node_id)
            .cloned()
            .collect()
    }

    /// 检查节点是否被禁止调度
    pub async fn is_scheduling_blocked(&self, node_id: &str) -> bool {
        let penalty_states = self.penalty_states.read().await;
        penalty_states.get(node_id)
            .map(|s| s.is_scheduling_blocked)
            .unwrap_or(false)
    }

    /// 获取节点调度权重
    pub async fn get_scheduling_weight(&self, node_id: &str) -> f64 {
        let penalty_states = self.penalty_states.read().await;
        penalty_states.get(node_id)
            .map(|s| s.scheduling_weight_multiplier)
            .unwrap_or(1.0)
    }

    /// 恢复节点（申诉成功或惩罚期满）
    pub async fn restore_node(&self, node_id: &str) -> Result<()> {
        let mut penalty_states = self.penalty_states.write().await;
        
        let state = penalty_states.get_mut(node_id)
            .context(format!("Node {} not found", node_id))?;

        // 标记所有活跃惩罚为已恢复
        for penalty in &mut state.active_penalties {
            penalty.mark_recovered();
        }

        // 恢复节点状态
        state.active_penalties.clear();
        state.is_scheduling_blocked = false;
        state.scheduling_block_ends_at = None;
        state.scheduling_weight_multiplier = 0.5; // 部分恢复

        // 恢复信誉分 - 简化实现
        // 实际应该调用 reputation_manager 的相应方法

        info!("Restored node {} after penalty recovery", node_id);

        Ok(())
    }

    /// 获取基础信誉管理器
    pub fn base_manager(&self) -> &ReputationManager {
        &self.base_manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_enhanced_reputation_manager_penalty() {
        let manager = EnhancedReputationManager::with_defaults("test_node".to_string());

        // 注册节点
        manager.base_manager.register_node("node_1".to_string());

        // 模拟完整性检查失败
        let result = IntegrityCheckResult {
            passed: false,
            confidence_score: 0.3,
            issues: vec![
                IntegrityIssue {
                    issue_type: IssueType::TooFastComputation,
                    description: "Computation too fast".to_string(),
                    severity: 0.8,
                    evidence: "evidence".to_string(),
                }
            ],
            details: Default::default(),
        };

        let penalty = manager.process_integrity_check(&result, "node_1").await.unwrap();
        
        assert!(penalty.is_some());
        let penalty = penalty.unwrap();
        assert_eq!(penalty.node_id, "node_1");
        assert!(penalty.severity >= 2);

        // 检查节点是否被禁止调度
        let blocked = manager.is_scheduling_blocked("node_1").await;
        assert!(blocked);
    }

    #[tokio::test]
    async fn test_penalty_expiration() {
        let mut strategy = PenaltyStrategy::default();
        strategy.default_suspension_duration_secs = 1; // 1 秒用于测试

        let manager = EnhancedReputationManager::new("test_node".to_string(), strategy);
        manager.base_manager.register_node("node_1".to_string());

        // 执行惩罚
        let result = IntegrityCheckResult {
            passed: false,
            confidence_score: 0.3,
            issues: vec![
                IntegrityIssue {
                    issue_type: IssueType::TooFastComputation,
                    description: "Test".to_string(),
                    severity: 0.6,
                    evidence: "evidence".to_string(),
                }
            ],
            details: Default::default(),
        };

        manager.process_integrity_check(&result, "node_1").await.unwrap();

        // 等待惩罚过期
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // 清理过期惩罚
        manager.cleanup_expired_penalties().await;

        // 检查节点状态
        let state = manager.get_node_penalty_state("node_1").await;
        assert!(state.is_some());
        let state = state.unwrap();
        assert_eq!(state.active_penalties.len(), 0);
    }

    #[test]
    fn test_penalty_record_serialization() {
        let penalty = PenaltyRecord::new(
            "node_1".to_string(),
            PenaltyType::Suspension,
            "Test penalty".to_string(),
            Some(3600),
            vec!["evidence".to_string()],
        );

        let json = serde_json::to_string(&penalty).unwrap();
        let restored: PenaltyRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(penalty.node_id, restored.node_id);
        assert_eq!(penalty.penalty_type, restored.penalty_type);
    }
}
