//! 节点信誉模块 - 去中心化信誉管理系统
//!
//! **架构定位**：
//! - 推理负责算得对：分布式推理模块负责高效计算
//! - 评估器负责验得准：质量评估器负责验证结果
//! - 多节点负责保安全：本模块配合多节点并行机制
//! - 区块链负责记可信：信誉记录上链，不可篡改
//!
//! **信誉系统职责**：
//! 1. 记录节点历史表现（成功/失败次数）
//! 2. 动态计算信誉评分
//! 3. 支持信誉查询和排序
//! 4. 恶意节点自动降权和剔除

use serde::{Serialize, Deserialize};
use std::collections::HashMap;

/// 节点状态
///
/// **状态转换规则**：
/// - `Active` → `UnderReview`：信誉分 < 0.6 或出现轻微异常
/// - `UnderReview` → `Frozen`：信誉分 < 0.3 或持续表现不佳
/// - `Frozen` → `Blacklisted`：恶意行为累计 ≥3 次
/// - `Blacklisted` → 永久不可恢复（需人工/链上治理干预）
///
/// **安全说明**：
/// - 一旦节点被拉黑（`Blacklisted`），不会自动恢复
/// - 这是为了防止恶意节点通过"刷信誉"方式重新进入系统
/// - 如需恢复，需通过链上治理投票或管理员手动操作
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NodeStatus {
    /// 活跃状态（可被调度）
    Active,
    /// 观察状态（需要多节点复核）
    UnderReview,
    /// 冻结状态（暂时不可调度）
    Frozen,
    /// 剔除状态（永久不可调度，需人工/治理干预才能恢复）
    Blacklisted,
}

/// 节点信誉信息
///
/// 对应创新点 B：链上可信分布式调度
/// - 记录节点的历史表现
/// - 用于调度决策
/// - 恶意节点会被降权或剔除
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeReputation {
    /// 节点 ID
    pub node_id: String,
    /// 节点地址（可选，用于链上标识）
    pub node_address: Option<String>,
    /// 信誉评分（0.0 - 1.0）
    pub score: f64,
    /// 节点状态
    pub status: NodeStatus,
    /// 成功完成的任务数
    pub completed_tasks: u64,
    /// 失败的任务数
    pub failed_tasks: u64,
    /// 被标记为恶意的次数
    pub malicious_count: u64,
    /// 累计贡献的算力（token 处理量）
    pub total_tokens_processed: u64,
    /// 累计获得的奖励（可选，用于激励机制）
    pub total_rewards: f64,
    /// 历史信誉记录（用于审计）
    pub history: Vec<ReputationRecord>,
}

/// 信誉记录 - 单次事件导致的信誉变化
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationRecord {
    /// 时间戳
    pub timestamp: u64,
    /// 事件类型
    pub event_type: ReputationEvent,
    /// 信誉变化量
    pub score_delta: f64,
    /// 变化后的信誉分
    pub score_after: f64,
    /// 关联的区块高度（可选）
    pub block_height: Option<u64>,
    /// 备注信息
    pub note: Option<String>,
}

/// 信誉事件类型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ReputationEvent {
    /// 任务成功完成
    TaskSuccess,
    /// 任务失败
    TaskFailure,
    /// KV Cache 校验失败（疑似篡改）
    KvCacheMismatch,
    /// 语义检查失败
    SemanticCheckFailed,
    /// 多节点结果不一致（被判定为劣质）
    MultiNodeDisagreement,
    /// 恶意行为确认
    MaliciousBehavior,
    /// 节点主动下线
    NodeOffline,
    /// 节点重新上线
    NodeOnline,
    /// 系统奖励
    SystemReward,
    /// 系统惩罚
    SystemPenalty,
}

impl NodeReputation {
    /// 创建新节点（默认满信誉）
    pub fn new(node_id: String) -> Self {
        NodeReputation {
            node_id,
            node_address: None,
            score: 1.0,
            status: NodeStatus::Active,
            completed_tasks: 0,
            failed_tasks: 0,
            malicious_count: 0,
            total_tokens_processed: 0,
            total_rewards: 0.0,
            history: Vec::new(),
        }
    }

    /// 创建新节点（带初始地址）
    pub fn with_address(node_id: String, node_address: String) -> Self {
        let mut node = Self::new(node_id);
        node.node_address = Some(node_address);
        node
    }

    /// 记录信誉变化
    fn record_event(
        &mut self,
        event_type: ReputationEvent,
        score_delta: f64,
        block_height: Option<u64>,
        note: Option<String>,
    ) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let score_after = self.score;

        self.history.push(ReputationRecord {
            timestamp,
            event_type,
            score_delta,
            score_after,
            block_height,
            note,
        });
    }

    /// 更新状态（基于信誉分）
    fn update_status(&mut self) {
        self.status = if self.malicious_count >= 3 {
            NodeStatus::Blacklisted
        } else if self.score < 0.3 {
            NodeStatus::Frozen
        } else if self.score < 0.6 {
            NodeStatus::UnderReview
        } else {
            NodeStatus::Active
        };
    }

    /// 更新信誉（任务成功后调用）
    pub fn on_task_success(&mut self, tokens_processed: u64, block_height: Option<u64>) {
        self.completed_tasks += 1;
        self.total_tokens_processed += tokens_processed;

        // 轻微提升信誉，上限 1.0
        let old_score = self.score;
        self.score = (self.score + 0.01).min(1.0);
        let delta = self.score - old_score;

        self.record_event(
            ReputationEvent::TaskSuccess,
            delta,
            block_height,
            None,
        );

        self.update_status();
    }

    /// 更新信誉（任务失败后调用）
    pub fn on_task_failed(&mut self, block_height: Option<u64>) {
        self.failed_tasks += 1;

        // 降低信誉
        let old_score = self.score;
        self.score *= 0.9;
        let delta = self.score - old_score;

        self.record_event(
            ReputationEvent::TaskFailure,
            delta,
            block_height,
            None,
        );

        self.update_status();
    }

    /// 标记恶意行为（KV 校验失败、语义检查失败等）
    pub fn on_malicious_behavior(
        &mut self,
        reason: &str,
        block_height: Option<u64>,
    ) {
        self.malicious_count += 1;

        // 大幅降低信誉
        let old_score = self.score;
        self.score *= 0.5;
        let delta = self.score - old_score;

        self.record_event(
            ReputationEvent::MaliciousBehavior,
            delta,
            block_height,
            Some(reason.to_string()),
        );

        self.update_status();
    }

    /// KV Cache 校验失败
    pub fn on_kv_cache_mismatch(&mut self, block_height: Option<u64>) {
        let old_score = self.score;
        self.score *= 0.7; // 中度惩罚
        let delta = self.score - old_score;

        self.record_event(
            ReputationEvent::KvCacheMismatch,
            delta,
            block_height,
            Some("KV Cache 哈希不匹配".to_string()),
        );

        self.update_status();
    }

    /// 语义检查失败
    pub fn on_semantic_check_failed(&mut self, block_height: Option<u64>) {
        let old_score = self.score;
        self.score *= 0.8; // 轻度惩罚
        let delta = self.score - old_score;

        self.record_event(
            ReputationEvent::SemanticCheckFailed,
            delta,
            block_height,
            Some("语义检查失败".to_string()),
        );

        self.update_status();
    }

    /// 多节点结果不一致（被判定为劣质）
    pub fn on_multi_node_disagreement(&mut self, is_winner: bool, block_height: Option<u64>) {
        let old_score = self.score;
        if is_winner {
            // 虽然是赢家但与其他节点不一致，轻微惩罚
            self.score *= 0.95;
        } else {
            // 输家，中度惩罚
            self.score *= 0.85;
        }
        let delta = self.score - old_score;

        self.record_event(
            ReputationEvent::MultiNodeDisagreement,
            delta,
            block_height,
            Some("多节点结果不一致".to_string()),
        );

        self.update_status();
    }

    /// 是否可信（用于调度决策）
    pub fn is_trustworthy(&self, threshold: f64) -> bool {
        self.score >= threshold && self.status == NodeStatus::Active
    }

    /// 是否需要多节点复核
    pub fn needs_multi_node_review(&self) -> bool {
        self.status == NodeStatus::UnderReview || self.score < 0.7
    }

    /// 是否被剔除
    pub fn is_blacklisted(&self) -> bool {
        self.status == NodeStatus::Blacklisted
    }

    /// 获取成功率
    pub fn success_rate(&self) -> f64 {
        let total = self.completed_tasks + self.failed_tasks;
        if total == 0 {
            1.0 // 新节点默认 100% 成功率
        } else {
            self.completed_tasks as f64 / total as f64
        }
    }

    /// 获取节点权重（用于加权调度）
    pub fn get_weight(&self) -> f64 {
        // 权重 = 信誉分 × 成功率 × log(1 + 任务数)
        let task_factor = (1.0 + self.completed_tasks as f64).ln();
        self.score * self.success_rate() * task_factor
    }
}

/// 信誉管理器 - 管理所有节点的信誉
///
/// **治理说明**：
/// - 被拉黑的节点（`Blacklisted`）不会自动恢复
/// - 这是核心安全机制，防止恶意节点"洗白"重新进入系统
/// - 未来扩展：可通过链上治理投票或管理员接口实现人工恢复
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationManager {
    /// 节点信誉表
    nodes: HashMap<String, NodeReputation>,
    /// 可信阈值
    trust_threshold: f64,
    /// 多节点复核阈值
    review_threshold: f64,
}

impl Default for ReputationManager {
    fn default() -> Self {
        Self::new(0.7, 0.6)
    }
}

impl ReputationManager {
    /// 创建新的信誉管理器
    pub fn new(trust_threshold: f64, review_threshold: f64) -> Self {
        ReputationManager {
            nodes: HashMap::new(),
            trust_threshold,
            review_threshold,
        }
    }

    /// 注册节点
    pub fn register_node(&mut self, node_id: String) -> &NodeReputation {
        let node_id_clone = node_id.clone();
        self.nodes
            .entry(node_id)
            .or_insert_with(|| NodeReputation::new(node_id_clone))
    }

    /// 注册节点（带地址）
    pub fn register_node_with_address(
        &mut self,
        node_id: String,
        node_address: String,
    ) -> &NodeReputation {
        let node_id_clone = node_id.clone();
        self.nodes
            .entry(node_id)
            .or_insert_with(|| NodeReputation::with_address(node_id_clone, node_address))
    }

    /// 获取节点信誉
    pub fn get_node(&self, node_id: &str) -> Option<&NodeReputation> {
        self.nodes.get(node_id)
    }

    /// 获取节点信誉（可变引用）
    pub fn get_node_mut(&mut self, node_id: &str) -> Option<&mut NodeReputation> {
        self.nodes.get_mut(node_id)
    }

    /// 获取所有活跃节点
    pub fn get_active_nodes(&self) -> Vec<&NodeReputation> {
        self.nodes
            .values()
            .filter(|n| n.status == NodeStatus::Active)
            .collect()
    }

    /// 获取可信节点列表（用于调度）
    pub fn get_trustworthy_nodes(&self) -> Vec<&NodeReputation> {
        self.nodes
            .values()
            .filter(|n| n.is_trustworthy(self.trust_threshold))
            .collect()
    }

    /// 获取需要多节点复核的节点
    pub fn get_nodes_needing_review(&self) -> Vec<&NodeReputation> {
        self.nodes
            .values()
            .filter(|n| n.needs_multi_node_review())
            .collect()
    }

    /// 获取被剔除的节点
    pub fn get_blacklisted_nodes(&self) -> Vec<&NodeReputation> {
        self.nodes
            .values()
            .filter(|n| n.is_blacklisted())
            .collect()
    }

    /// 获取按权重排序的节点列表（用于加权调度）
    pub fn get_weighted_nodes(&self) -> Vec<&NodeReputation> {
        let mut nodes: Vec<&NodeReputation> = self.nodes
            .values()
            .filter(|n| n.status == NodeStatus::Active)
            .collect();
        
        nodes.sort_by(|a, b| {
            b.get_weight().partial_cmp(&a.get_weight()).unwrap()
        });
        
        nodes
    }

    /// 设置可信阈值
    pub fn set_trust_threshold(&mut self, threshold: f64) {
        self.trust_threshold = threshold;
    }

    /// 设置复核阈值
    pub fn set_review_threshold(&mut self, threshold: f64) {
        self.review_threshold = threshold;
    }

    /// 获取节点总数
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// 获取活跃节点数
    pub fn active_node_count(&self) -> usize {
        self.nodes
            .values()
            .filter(|n| n.status == NodeStatus::Active)
            .count()
    }

    /// 获取被剔除节点数
    pub fn blacklisted_count(&self) -> usize {
        self.nodes
            .values()
            .filter(|n| n.status == NodeStatus::Blacklisted)
            .count()
    }

    /// 移除被剔除的节点（清理操作）
    pub fn prune_blacklisted(&mut self) -> Vec<NodeReputation> {
        let blacklisted: Vec<String> = self.nodes
            .iter()
            .filter(|(_, n)| n.is_blacklisted())
            .map(|(id, _)| id.clone())
            .collect();

        let mut removed = Vec::new();
        for id in blacklisted {
            if let Some(node) = self.nodes.remove(&id) {
                removed.push(node);
            }
        }

        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_reputation_creation() {
        let node = NodeReputation::new("node_1".to_string());

        assert_eq!(node.node_id, "node_1");
        assert_eq!(node.score, 1.0);
        assert_eq!(node.status, NodeStatus::Active);
        assert_eq!(node.completed_tasks, 0);
        assert_eq!(node.failed_tasks, 0);
    }

    #[test]
    fn test_task_success_penalty() {
        let mut node = NodeReputation::new("node_1".to_string());

        // 成功任务
        node.on_task_success(1000, Some(10));
        assert_eq!(node.completed_tasks, 1);
        assert_eq!(node.total_tokens_processed, 1000);
        assert!(node.score > 0.99);

        // 失败任务
        node.on_task_failed(Some(11));
        assert_eq!(node.failed_tasks, 1);
        assert!(node.score < 1.0);
    }

    #[test]
    fn test_malicious_behavior_penalty() {
        let mut node = NodeReputation::new("node_1".to_string());

        // 第一次恶意行为
        node.on_malicious_behavior("KV 校验失败", Some(10));
        assert_eq!(node.malicious_count, 1);
        assert!(node.score < 0.6);
        assert_eq!(node.status, NodeStatus::UnderReview);

        // 第二次
        node.on_malicious_behavior("语义检查失败", Some(11));
        assert_eq!(node.malicious_count, 2);
        assert_eq!(node.status, NodeStatus::Frozen);

        // 第三次 - 应该被剔除
        node.on_malicious_behavior("多节点不一致", Some(12));
        assert_eq!(node.malicious_count, 3);
        assert_eq!(node.status, NodeStatus::Blacklisted);
    }

    #[test]
    fn test_status_transitions() {
        let mut node = NodeReputation::new("node_1".to_string());

        // 初始状态
        assert_eq!(node.status, NodeStatus::Active);

        // 连续失败降低信誉
        // 每次失败乘以 0.9，10 次后约为 0.35
        for _ in 0..10 {
            node.on_task_failed(None);
        }

        assert!(node.score < 0.4);
        // 信誉分 < 0.6 但 >= 0.3，状态为 UnderReview
        assert_eq!(node.status, NodeStatus::UnderReview);

        // 再失败几次，让信誉分低于 0.3
        for _ in 0..5 {
            node.on_task_failed(None);
        }

        assert!(node.score < 0.3);
        assert_eq!(node.status, NodeStatus::Frozen);
    }

    #[test]
    fn test_success_rate() {
        let mut node = NodeReputation::new("node_1".to_string());

        // 新节点默认 100%
        assert_eq!(node.success_rate(), 1.0);

        // 3 成功 1 失败 = 75%
        for _ in 0..3 {
            node.on_task_success(100, None);
        }
        node.on_task_failed(None);

        assert!((node.success_rate() - 0.75).abs() < 0.01);
    }

    #[test]
    fn test_weight_calculation() {
        let mut node1 = NodeReputation::new("node_1".to_string());
        let mut node2 = NodeReputation::new("node_2".to_string());

        // node1: 高信誉，多任务
        for _ in 0..10 {
            node1.on_task_success(100, None);
        }

        // node2: 低信誉，少任务
        node2.on_task_success(100, None);
        node2.on_task_failed(None);
        node2.on_task_failed(None);

        assert!(node1.get_weight() > node2.get_weight());
    }

    #[test]
    fn test_reputation_manager() {
        let mut manager = ReputationManager::new(0.7, 0.6);

        // 注册节点
        manager.register_node("node_1".to_string());
        manager.register_node("node_2".to_string());

        assert_eq!(manager.node_count(), 2);

        // 获取可信节点
        let trustworthy = manager.get_trustworthy_nodes();
        assert_eq!(trustworthy.len(), 2);

        // 降低 node_2 的信誉
        {
            let node_2 = manager.get_node_mut("node_2").unwrap();
            for _ in 0..5 {
                node_2.on_task_failed(None);
            }
        }

        // 现在只有 node_1 可信
        let trustworthy = manager.get_trustworthy_nodes();
        assert_eq!(trustworthy.len(), 1);
        assert_eq!(trustworthy[0].node_id, "node_1");
    }

    #[test]
    fn test_blacklist_pruning() {
        let mut manager = ReputationManager::new(0.7, 0.6);
        manager.register_node("node_1".to_string());
        manager.register_node("node_2".to_string());

        // 让 node_2 被剔除
        {
            let node_2 = manager.get_node_mut("node_2").unwrap();
            for _ in 0..3 {
                node_2.on_malicious_behavior("测试", None);
            }
        }

        assert_eq!(manager.blacklisted_count(), 1);

        // 清理被剔除的节点
        let removed = manager.prune_blacklisted();
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].node_id, "node_2");
        assert_eq!(manager.node_count(), 1);
    }
}
