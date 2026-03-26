//! 质量感知共识模块 - P2-2：双阈值判定
//!
//! **设计目标**：
//! - 基于质量分数的共识决策
//! - 双阈值判定机制（接受阈值 / 拒绝阈值）
//! - 支持多节点质量投票
//! - 与 PBFT 共识集成
//!
//! **核心概念**：
//! - **质量投票** - 验证节点对推理结果的质量评分
//! - **双阈值判定** - 高于接受阈值则通过，低于拒绝阈值则拒绝，中间区域需要更多投票
//! - **加权聚合** - 基于节点信誉的加权质量分数聚合
//!
//! **共识流程**：
//! 1. 验证节点提交质量投票（包含质量分数和证据）
//! 2. 聚合所有投票，计算加权质量分数
//! 3. 双阈值判定：
//!    - 加权分数 >= 接受阈值 → 共识通过
//!    - 加权分数 <= 拒绝阈值 → 共识拒绝
//!    - 中间区域 → 等待更多投票
//! 4. 达成共识后提交上链

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::{Result, Context};
use tracing::{info, warn, debug};
use serde::{Serialize, Deserialize};

/// 质量感知共识配置
#[derive(Debug, Clone)]
pub struct QualityConsensusConfig {
    /// 接受阈值（高于此值则通过）
    pub accept_threshold: f64,
    /// 拒绝阈值（低于此值则拒绝）
    pub reject_threshold: f64,
    /// 最小投票数（达到此数量才进行判定）
    pub min_votes: usize,
    /// 超时时间（毫秒）
    pub timeout_ms: u64,
    /// 是否启用信誉加权
    pub enable_reputation_weighting: bool,
    /// 默认信誉权重（当启用信誉加权时使用）
    pub default_reputation_weight: f64,
}

impl Default for QualityConsensusConfig {
    fn default() -> Self {
        QualityConsensusConfig {
            accept_threshold: 0.8,
            reject_threshold: 0.5,
            min_votes: 3,
            timeout_ms: 10000, // 10 秒
            enable_reputation_weighting: true,
            default_reputation_weight: 1.0,
        }
    }
}

/// 质量投票
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityVote {
    /// 投票 ID
    pub vote_id: String,
    /// 请求 ID
    pub request_id: String,
    /// 验证节点 ID
    pub validator_id: String,
    /// 质量分数（0.0 - 1.0）
    pub quality_score: f64,
    /// 节点信誉分（用于加权）
    pub reputation_score: f64,
    /// 投票时间戳
    pub timestamp: u64,
    /// 验证证据哈希
    pub evidence_hash: String,
    /// 节点签名
    pub signature: String,
    /// 投票状态
    pub vote_status: VoteStatus,
}

/// 投票状态
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum VoteStatus {
    /// 待处理
    Pending,
    /// 已接受
    Accepted,
    /// 已拒绝
    Rejected,
    /// 需要更多投票
    NeedsMoreVotes,
}

/// 加权质量投票结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightedVoteResult {
    /// 请求 ID
    pub request_id: String,
    /// 加权质量分数
    pub weighted_score: f64,
    /// 投票总数
    pub vote_count: usize,
    /// 接受票数
    pub accept_votes: usize,
    /// 拒绝票数
    pub reject_votes: usize,
    /// 判定结果
    pub decision: ConsensusDecision,
    /// 判定置信度
    pub confidence: f64,
}

/// 共识判定结果
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ConsensusDecision {
    /// 共识通过
    Accepted,
    /// 共识拒绝
    Rejected,
    /// 需要更多投票
    Pending,
    /// 超时
    Timeout,
}

/// 质量投票记录（用于追踪）
#[derive(Debug, Clone)]
pub struct VoteRecord {
    /// 投票
    pub vote: QualityVote,
    /// 接收时间戳
    pub received_at: u64,
    /// 是否已验证
    pub verified: bool,
}

/// 质量感知共识状态
#[derive(Debug, Clone)]
pub struct QualityConsensusState {
    /// 请求 ID
    pub request_id: String,
    /// 所有投票
    pub votes: Vec<VoteRecord>,
    /// 共识开始时间
    pub started_at: u64,
    /// 当前状态
    pub state: ConsensusState,
    /// 判定结果（如果有）
    pub decision: Option<WeightedVoteResult>,
}

/// 共识状态枚举
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConsensusState {
    /// 等待投票
    Collecting,
    /// 判定中
    Deciding,
    /// 已完成
    Completed,
    /// 已超时
    Timeout,
}

/// 质量感知共识管理器
///
/// **核心职责**：
/// - 收集和管理质量投票
/// - 执行双阈值判定
/// - 计算加权质量分数
/// - 与 PBFT 共识集成
pub struct QualityAwareConsensusManager {
    /// 共识配置
    config: QualityConsensusConfig,
    /// 共识状态（按请求 ID 索引）
    consensus_states: Arc<RwLock<HashMap<String, QualityConsensusState>>>,
    /// 节点信誉映射（节点 ID -> 信誉分）
    node_reputation: Arc<RwLock<HashMap<String, f64>>>,
}

impl QualityAwareConsensusManager {
    /// 创建新的质量感知共识管理器
    pub fn new(config: QualityConsensusConfig) -> Self {
        QualityAwareConsensusManager {
            config,
            consensus_states: Arc::new(RwLock::new(HashMap::new())),
            node_reputation: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 初始化共识状态
    pub async fn init_consensus(&self, request_id: &str) -> Result<()> {
        let mut states = self.consensus_states.write().await;
        
        let state = QualityConsensusState {
            request_id: request_id.to_string(),
            votes: Vec::new(),
            started_at: current_timestamp(),
            state: ConsensusState::Collecting,
            decision: None,
        };

        states.insert(request_id.to_string(), state);
        info!("Initialized consensus state for request: {}", request_id);

        Ok(())
    }

    /// 提交质量投票
    pub async fn submit_vote(&self, vote: QualityVote) -> Result<ConsensusDecision> {
        let request_id = vote.request_id.clone();
        
        // 验证投票
        if !self.verify_vote(&vote).await? {
            warn!("Invalid vote from validator: {}", vote.validator_id);
            return Ok(ConsensusDecision::Rejected);
        }

        // 创建投票记录
        let record = VoteRecord {
            vote: vote.clone(),
            received_at: current_timestamp(),
            verified: true,
        };

        // 更新共识状态
        let mut states = self.consensus_states.write().await;
        let state = states
            .get_mut(&request_id)
            .context("Consensus state not found")?;

        state.votes.push(record);

        // 检查是否达到最小投票数
        if state.votes.len() >= self.config.min_votes {
            // 执行双阈值判定
            let result = self.make_decision(&request_id).await?;
            state.decision = Some(result.clone());
            state.state = ConsensusState::Completed;

            info!(
                "Consensus decision made for request {}: {:?} (weighted_score={:.2}, confidence={:.2})",
                request_id, result.decision, result.weighted_score, result.confidence
            );

            Ok(result.decision)
        } else {
            // 还需要更多投票
            state.state = ConsensusState::Collecting;
            Ok(ConsensusDecision::Pending)
        }
    }

    /// 执行双阈值判定
    async fn make_decision(&self, request_id: &str) -> Result<WeightedVoteResult> {
        let states = self.consensus_states.read().await;
        let state = states
            .get(request_id)
            .context("Consensus state not found")?;

        let votes: Vec<&QualityVote> = state.votes.iter().map(|r| &r.vote).collect();

        // 计算加权质量分数
        let weighted_score = self.calculate_weighted_score(&votes).await;

        // 统计接受/拒绝票数
        let accept_votes = votes
            .iter()
            .filter(|v| v.quality_score >= self.config.accept_threshold)
            .count();
        let reject_votes = votes
            .iter()
            .filter(|v| v.quality_score <= self.config.reject_threshold)
            .count();

        // 双阈值判定
        let decision = if weighted_score >= self.config.accept_threshold {
            ConsensusDecision::Accepted
        } else if weighted_score <= self.config.reject_threshold {
            ConsensusDecision::Rejected
        } else {
            // 中间区域：基于多数票判定
            if accept_votes > reject_votes {
                ConsensusDecision::Accepted
            } else if reject_votes > accept_votes {
                ConsensusDecision::Rejected
            } else {
                // 平票：需要更多投票
                ConsensusDecision::Pending
            }
        };

        // 计算置信度
        let confidence = self.calculate_confidence(&votes, weighted_score);

        Ok(WeightedVoteResult {
            request_id: request_id.to_string(),
            weighted_score,
            vote_count: votes.len(),
            accept_votes,
            reject_votes,
            decision,
            confidence,
        })
    }

    /// 计算加权质量分数
    async fn calculate_weighted_score(&self, votes: &[&QualityVote]) -> f64 {
        if votes.is_empty() {
            return 0.0;
        }

        if !self.config.enable_reputation_weighting {
            // 简单平均
            return votes.iter().map(|v| v.quality_score).sum::<f64>() / votes.len() as f64;
        }

        // 加权平均
        let mut total_weight = 0.0;
        let mut weighted_sum = 0.0;

        for vote in votes {
            let weight = vote.reputation_score.max(self.config.default_reputation_weight);
            weighted_sum += vote.quality_score * weight;
            total_weight += weight;
        }

        if total_weight > 0.0 {
            weighted_sum / total_weight
        } else {
            0.0
        }
    }

    /// 计算判定置信度
    fn calculate_confidence(&self, votes: &[&QualityVote], weighted_score: f64) -> f64 {
        if votes.is_empty() {
            return 0.0;
        }

        // 基于投票数量和分数离散度计算置信度
        let vote_count_factor = (votes.len() as f64 / self.config.min_votes as f64).min(1.0);

        // 计算分数标准差
        let mean = weighted_score;
        let variance = votes
            .iter()
            .map(|v| (v.quality_score - mean).powi(2))
            .sum::<f64>()
            / votes.len() as f64;
        let std_dev = variance.sqrt();

        // 标准差越小，置信度越高
        let consistency_factor = 1.0 - std_dev.min(1.0);

        // 综合置信度
        (vote_count_factor * 0.5 + consistency_factor * 0.5).min(1.0)
    }

    /// 验证投票有效性
    async fn verify_vote(&self, vote: &QualityVote) -> Result<bool> {
        // 验证签名（简化实现）
        // 实际实现应该验证加密签名

        // 验证质量分数范围
        if vote.quality_score < 0.0 || vote.quality_score > 1.0 {
            return Ok(false);
        }

        // 验证时间戳（防止重放攻击）
        let now = current_timestamp();
        if vote.timestamp > now + 60000 || vote.timestamp < now - 60000 {
            // 允许 1 分钟的时钟偏差
            return Ok(false);
        }

        // 验证节点信誉（可选）
        let reputation = self.get_node_reputation(&vote.validator_id).await;
        if reputation < 0.3 {
            // 信誉过低的节点投票无效
            return Ok(false);
        }

        Ok(true)
    }

    /// 获取节点信誉分
    async fn get_node_reputation(&self, node_id: &str) -> f64 {
        let reputation = self.node_reputation.read().await;
        *reputation
            .get(node_id)
            .unwrap_or(&self.config.default_reputation_weight)
    }

    /// 更新节点信誉
    pub async fn update_node_reputation(&self, node_id: &str, score: f64) {
        let mut reputation = self.node_reputation.write().await;
        reputation.insert(node_id.to_string(), score.clamp(0.0, 1.0));
        debug!("Updated reputation for node {}: {}", node_id, score);
    }

    /// 获取共识状态
    pub async fn get_consensus_state(&self, request_id: &str) -> Option<QualityConsensusState> {
        let states = self.consensus_states.read().await;
        states.get(request_id).cloned()
    }

    /// 获取判定结果
    pub async fn get_decision(&self, request_id: &str) -> Option<WeightedVoteResult> {
        let states = self.consensus_states.read().await;
        states.get(request_id).and_then(|s| s.decision.clone())
    }

    /// 检查是否超时
    pub async fn check_timeout(&self, request_id: &str) -> bool {
        let states = self.consensus_states.read().await;
        if let Some(state) = states.get(request_id) {
            let elapsed = current_timestamp() - state.started_at;
            elapsed > self.config.timeout_ms
        } else {
            false
        }
    }

    /// 清理已完成的共识状态（垃圾回收）
    pub async fn cleanup_completed(&self, max_age_ms: u64) -> usize {
        let mut states = self.consensus_states.write().await;
        let now = current_timestamp();
        let mut removed = 0;

        states.retain(|_, state| {
            if state.state == ConsensusState::Completed {
                let age = now - state.started_at;
                if age > max_age_ms {
                    removed += 1;
                    return false;
                }
            }
            true
        });

        if removed > 0 {
            debug!("Cleaned up {} completed consensus states", removed);
        }

        removed
    }
}

/// 质量投票构建器
pub struct QualityVoteBuilder {
    vote_id: String,
    request_id: String,
    validator_id: String,
    quality_score: f64,
    reputation_score: f64,
    evidence_hash: String,
}

impl QualityVoteBuilder {
    /// 创建新的投票构建器
    pub fn new(request_id: &str, validator_id: &str) -> Self {
        QualityVoteBuilder {
            vote_id: generate_vote_id(),
            request_id: request_id.to_string(),
            validator_id: validator_id.to_string(),
            quality_score: 0.0,
            reputation_score: 1.0,
            evidence_hash: String::new(),
        }
    }

    /// 设置质量分数
    pub fn quality_score(mut self, score: f64) -> Self {
        self.quality_score = score.clamp(0.0, 1.0);
        self
    }

    /// 设置信誉分数
    pub fn reputation_score(mut self, score: f64) -> Self {
        self.reputation_score = score.clamp(0.0, 1.0);
        self
    }

    /// 设置证据哈希
    pub fn evidence_hash(mut self, hash: &str) -> Self {
        self.evidence_hash = hash.to_string();
        self
    }

    /// 构建投票
    pub fn build(self, signature: &str) -> QualityVote {
        QualityVote {
            vote_id: self.vote_id,
            request_id: self.request_id,
            validator_id: self.validator_id,
            quality_score: self.quality_score,
            reputation_score: self.reputation_score,
            timestamp: current_timestamp(),
            evidence_hash: self.evidence_hash,
            signature: signature.to_string(),
            vote_status: VoteStatus::Pending,
        }
    }
}

// 辅助函数

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn generate_vote_id() -> String {
    use sha2::{Sha256, Digest};
    let timestamp = current_timestamp();
    let random = rand::random::<u64>();
    let data = format!("{}:{}", timestamp, random);
    let hash = Sha256::digest(data.as_bytes());
    format!("vote_{:x}", hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_consensus_accept() {
        let config = QualityConsensusConfig {
            accept_threshold: 0.8,
            reject_threshold: 0.5,
            min_votes: 3,
            ..Default::default()
        };

        let manager = QualityAwareConsensusManager::new(config);
        let request_id = "test_request_1";

        // 初始化共识
        manager.init_consensus(request_id).await.unwrap();

        // 提交 3 个高质量投票
        for i in 0..3 {
            let vote = QualityVoteBuilder::new(request_id, &format!("validator_{}", i))
                .quality_score(0.9)
                .reputation_score(0.95)
                .evidence_hash("evidence_hash")
                .build("signature");

            let decision = manager.submit_vote(vote).await.unwrap();
            
            if i < 2 {
                assert_eq!(decision, ConsensusDecision::Pending);
            } else {
                assert_eq!(decision, ConsensusDecision::Accepted);
            }
        }
    }

    #[tokio::test]
    async fn test_consensus_reject() {
        let config = QualityConsensusConfig {
            accept_threshold: 0.8,
            reject_threshold: 0.5,
            min_votes: 3,
            ..Default::default()
        };

        let manager = QualityAwareConsensusManager::new(config);
        let request_id = "test_request_2";

        manager.init_consensus(request_id).await.unwrap();

        // 提交 3 个低质量投票
        for i in 0..3 {
            let vote = QualityVoteBuilder::new(request_id, &format!("validator_{}", i))
                .quality_score(0.3)
                .reputation_score(0.9)
                .evidence_hash("evidence_hash")
                .build("signature");

            let decision = manager.submit_vote(vote).await.unwrap();
            
            if i < 2 {
                assert_eq!(decision, ConsensusDecision::Pending);
            } else {
                assert_eq!(decision, ConsensusDecision::Rejected);
            }
        }
    }

    #[tokio::test]
    async fn test_weighted_score() {
        let manager = QualityAwareConsensusManager::new(QualityConsensusConfig::default());

        let votes = vec![
            QualityVote {
                vote_id: "vote1".to_string(),
                request_id: "test".to_string(),
                validator_id: "v1".to_string(),
                quality_score: 0.9,
                reputation_score: 1.0,
                timestamp: 0,
                evidence_hash: String::new(),
                signature: String::new(),
                vote_status: VoteStatus::Pending,
            },
            QualityVote {
                vote_id: "vote2".to_string(),
                request_id: "test".to_string(),
                validator_id: "v2".to_string(),
                quality_score: 0.7,
                reputation_score: 0.5,
                timestamp: 0,
                evidence_hash: String::new(),
                signature: String::new(),
                vote_status: VoteStatus::Pending,
            },
        ];

        let vote_refs: Vec<&QualityVote> = votes.iter().collect();
        let weighted_score = manager.calculate_weighted_score(&vote_refs).await;

        // 加权平均：(0.9*1.0 + 0.7*0.5) / (1.0 + 0.5) = (0.9 + 0.35) / 1.5 = 0.833...
        assert!(weighted_score > 0.8 && weighted_score < 0.85);
    }
}
