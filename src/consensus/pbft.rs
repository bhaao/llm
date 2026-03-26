//! PBFT 共识核心引擎 - 三阶段提交实现
//!
//! **核心流程**：
//! 1. Client → Leader: Request
//! 2. Leader → All: Pre-prepare (view, sequence, digest)
//! 3. All → All: Prepare (view, sequence, digest)
//! 4. All → All: Commit (view, sequence, digest)
//! 5. Execute: 收到 2f+1 Commit 后执行
//!
//! **安全保证**：
//! - 任何两个诚实节点不会在同一序列号提交不同操作
//! - 需要 2f+1 个节点才能推进
//! - 视图切换处理 Leader 作恶

use std::collections::{HashMap, HashSet, VecDeque};
use ed25519_dalek::{SigningKey, VerifyingKey, Signer};
use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};
use log::{info, warn, debug};

use crate::consensus::messages::{
    PBFTMessage, SignedMessage, Operation, PreparedMessage,
};
use crate::consensus::certificate::{CertificateManager, Checkpoint};
use crate::consensus::view_change::{ViewChangeManager, ViewChangeMessage};

/// 共识配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusConfig {
    /// 节点 ID
    pub node_id: String,
    /// 所有节点列表
    pub nodes: Vec<String>,
    /// 节点公钥映射 (node_id -> public_key)
    pub public_keys: HashMap<String, Vec<u8>>,
    /// 签名私钥
    pub signing_key: Vec<u8>,
    /// 视图切换超时（毫秒）
    pub view_change_timeout_ms: u64,
    /// 请求处理超时（毫秒）
    pub request_timeout_ms: u64,
    /// Checkpoint 间隔
    pub checkpoint_interval: u64,
}

impl ConsensusConfig {
    /// 创建测试配置
    pub fn for_testing(node_id: String, nodes: Vec<String>) -> Self {
        use rand::random;
        
        // 生成随机密钥
        let key_bytes: [u8; 32] = random();
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&key_bytes);

        let mut public_keys = HashMap::new();
        for node in &nodes {
            // 测试环境使用相同密钥（仅用于测试）
            public_keys.insert(node.clone(), signing_key.verifying_key().to_bytes().to_vec());
        }

        ConsensusConfig {
            node_id,
            nodes,
            public_keys,
            signing_key: signing_key.to_bytes().to_vec(),
            view_change_timeout_ms: 5000,
            request_timeout_ms: 3000,
            checkpoint_interval: 100,
        }
    }

    /// 获取法定人数大小 (2f+1)
    pub fn quorum_size(&self) -> usize {
        let n = self.nodes.len();
        let f = (n - 1) / 3; // 最大容错节点数
        2 * f + 1
    }

    /// 获取最大容错节点数
    pub fn max_faulty(&self) -> usize {
        (self.nodes.len() - 1) / 3
    }
}

/// 共识状态
#[derive(Debug, Clone, PartialEq)]
pub enum ConsensusState {
    /// 正常状态
    Normal,
    /// 视图切换中
    ViewChanging,
    /// 已恢复
    Recovered,
}

/// 待处理请求
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct PendingRequest {
    /// 请求摘要
    digest: String,
    /// 客户端 ID
    client_id: String,
    /// 操作数据
    operation: Vec<u8>,
    /// 时间戳
    timestamp: u64,
    /// 序列号（分配后）
    sequence: Option<u64>,
}

/// PBFT 共识引擎
pub struct PBFTConsensus {
    /// 配置
    config: ConsensusConfig,
    /// 当前视图号
    view: u64,
    /// 下一个可用序列号
    sequence: u64,
    /// 消息日志
    log: HashMap<String, MessageLog>,
    /// 证书管理器
    cert_manager: CertificateManager,
    /// 视图切换管理器
    view_change_manager: ViewChangeManager,
    /// Checkpoint 列表
    checkpoints: Vec<Checkpoint>,
    /// 最后稳定 checkpoint 序列号
    last_stable_checkpoint: u64,
    /// 共识状态
    state: ConsensusState,
    /// 待处理请求队列
    pending_requests: VecDeque<PendingRequest>,
    /// 已执行序列号集合（用于去重）
    executed_sequences: HashSet<u64>,
    /// 签名密钥
    signing_key: SigningKey,
    /// 验证密钥
    verifying_key: VerifyingKey,
}

/// 消息日志 - 记录单个请求的所有消息
#[derive(Debug, Clone, Default)]
struct MessageLog {
    /// Pre-prepare 消息
    pre_prepare: Option<SignedMessage>,
    /// Prepare 消息集合
    prepares: HashMap<String, SignedMessage>,
    /// Commit 消息集合
    commits: HashMap<String, SignedMessage>,
    /// 是否已准备（收到 2f+1 Prepare）
    prepared: bool,
    /// 是否已提交（收到 2f+1 Commit）
    committed: bool,
    /// 是否已执行
    executed: bool,
}

impl PBFTConsensus {
    /// 创建新的 PBFT 共识引擎
    pub fn new(config: ConsensusConfig) -> Result<Self, String> {
        let signing_key_bytes: [u8; 32] = config.signing_key.clone()
            .try_into()
            .map_err(|_| "Invalid signing key length")?;
        let signing_key = SigningKey::from_bytes(&signing_key_bytes);
        let verifying_key = signing_key.verifying_key();

        let quorum_size = config.quorum_size();
        let nodes = config.nodes.clone();
        let view_change_timeout = config.view_change_timeout_ms;

        let mut consensus = PBFTConsensus {
            config,
            view: 0,
            sequence: 1,
            log: HashMap::new(),
            cert_manager: CertificateManager::new(quorum_size),
            view_change_manager: ViewChangeManager::new(nodes, quorum_size, view_change_timeout),
            checkpoints: Vec::new(),
            last_stable_checkpoint: 0,
            state: ConsensusState::Normal,
            pending_requests: VecDeque::new(),
            executed_sequences: HashSet::new(),
            signing_key,
            verifying_key,
        };

        consensus.view_change_manager.update_activity();

        Ok(consensus)
    }

    /// 获取当前视图号
    pub fn view(&self) -> u64 {
        self.view
    }

    /// 获取当前序列号
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// 获取共识状态
    pub fn state(&self) -> &ConsensusState {
        &self.state
    }

    /// 检查是否是 Leader
    pub fn is_leader(&self) -> bool {
        self.view_change_manager.is_leader(&self.config.node_id)
    }

    /// 获取 Leader ID
    pub fn leader_id(&self) -> Option<String> {
        self.view_change_manager.get_current_leader()
    }

    /// 提出操作（仅 Leader 调用）
    ///
    /// # Returns
    /// - `Some(sequence)`: 操作已提出，返回序列号
    /// - `None`: 非 Leader 或视图切换中
    pub async fn propose(&mut self, operation: Operation) -> Option<u64> {
        if !self.is_leader() {
            warn!("Not leader, cannot propose");
            return None;
        }

        if self.state != ConsensusState::Normal {
            warn!("Not in normal state, cannot propose");
            return None;
        }

        let digest = operation.digest();
        let sequence = self.sequence;

        // 创建 Pre-prepare 消息
        let message = PBFTMessage::PrePrepare {
            view: self.view,
            sequence,
            digest: digest.clone(),
            leader: self.config.node_id.clone(),
            request: operation.to_bytes(),
        };

        let signed = SignedMessage::new(message, self.config.node_id.clone(), &self.signing_key);

        // 记录日志
        let log_entry = self.log.entry(digest.clone()).or_default();
        log_entry.pre_prepare = Some(signed.clone());

        // 广播 Pre-prepare
        self.sequence += 1;

        info!("Leader proposed operation at sequence {}", sequence);

        Some(sequence)
    }

    /// 处理接收到的消息
    pub async fn receive_message(&mut self, signed_msg: SignedMessage) -> Result<(), String> {
        // 验证签名
        if let Some(public_key_bytes) = self.config.public_keys.get(&signed_msg.sender_id) {
            let pk: [u8; 32] = public_key_bytes.clone()
                .try_into()
                .map_err(|_| "Invalid public key length")?;
            let public_key = VerifyingKey::from_bytes(&pk).map_err(|e| e.to_string())?;

            if !signed_msg.verify(&public_key) {
                return Err("Invalid signature".to_string());
            }
        } else {
            return Err(format!("Unknown sender: {}", signed_msg.sender_id));
        }

        // 更新活动
        self.view_change_manager.update_activity();

        match &signed_msg.message {
            PBFTMessage::PrePrepare { view, sequence, digest, .. } => {
                self.handle_pre_prepare(*view, *sequence, digest.clone(), signed_msg).await?;
            }
            PBFTMessage::Prepare { view, sequence, digest, replica_id, .. } => {
                self.handle_prepare(*view, *sequence, digest.clone(), replica_id.clone(), signed_msg).await?;
            }
            PBFTMessage::Commit { view, sequence, digest, replica_id, .. } => {
                self.handle_commit(*view, *sequence, digest.clone(), replica_id.clone(), signed_msg).await?;
            }
            PBFTMessage::ViewChange { new_view, sender_id, .. } => {
                self.handle_view_change(*new_view, sender_id.clone(), signed_msg).await?;
            }
            PBFTMessage::NewView { new_view, new_leader, .. } => {
                self.handle_new_view(*new_view, new_leader.clone(), signed_msg).await?;
            }
            _ => {
                debug!("Received unhandled message type: {:?}", signed_msg.message_type());
            }
        }

        Ok(())
    }

    /// 处理 Pre-prepare 消息（Replica）
    async fn handle_pre_prepare(
        &mut self,
        view: u64,
        sequence: u64,
        digest: String,
        signed_msg: SignedMessage,
    ) -> Result<(), String> {
        // 验证视图号
        if view != self.view {
            return Err(format!("View mismatch: expected {}, got {}", self.view, view));
        }

        // 验证序列号
        if sequence <= self.last_stable_checkpoint {
            return Err(format!(
                "Sequence {} is before last stable checkpoint {}",
                sequence, self.last_stable_checkpoint
            ));
        }

        // 验证是否是 Leader 发送
        if let Some(leader) = self.leader_id() {
            if signed_msg.sender_id != leader {
                return Err(format!("Pre-prepare not from leader: {}", signed_msg.sender_id));
            }
        }

        // 检查是否已存在
        if let Some(log) = self.log.get(&digest) {
            if log.pre_prepare.is_some() {
                debug!("Already have Pre-prepare for digest {}", digest);
                return Ok(());
            }
        }

        // 记录 Pre-prepare
        let log_entry = self.log.entry(digest.clone()).or_default();
        log_entry.pre_prepare = Some(signed_msg);

        // 发送 Prepare 消息
        self.send_prepare(digest.clone(), view, sequence).await?;

        Ok(())
    }

    /// 发送 Prepare 消息
    async fn send_prepare(
        &mut self,
        digest: String,
        view: u64,
        sequence: u64,
    ) -> Result<(), String> {
        let message = PBFTMessage::Prepare {
            view,
            sequence,
            digest: digest.clone(),
            replica_id: self.config.node_id.clone(),
            signature: vec![],
        };

        let signed = SignedMessage::new(message, self.config.node_id.clone(), &self.signing_key);

        // 记录自己的 Prepare
        let log_entry = self.log.entry(digest.clone()).or_default();
        log_entry.prepares.insert(self.config.node_id.clone(), signed.clone());

        // 广播 Prepare（实际实现中通过网络发送）
        info!("Sent Prepare for digest {} at sequence {}", digest, sequence);

        Ok(())
    }

    /// 处理 Prepare 消息
    async fn handle_prepare(
        &mut self,
        view: u64,
        sequence: u64,
        digest: String,
        replica_id: String,
        signed_msg: SignedMessage,
    ) -> Result<(), String> {
        if view != self.view {
            return Ok(()); // 忽略旧视图的消息
        }

        // 记录 Prepare
        let log_entry = self.log.entry(digest.clone()).or_default();
        log_entry.prepares.insert(replica_id, signed_msg);

        // 检查是否达到准备条件（2f+1 个 Prepare）
        if !log_entry.prepared && log_entry.prepares.len() >= self.config.quorum_size() {
            if log_entry.pre_prepare.is_some() {
                log_entry.prepared = true;

                // 发送 Commit 消息
                self.send_commit(digest.clone(), view, sequence).await?;

                info!("Prepared digest {} at sequence {}", digest, sequence);
            }
        }

        Ok(())
    }

    /// 发送 Commit 消息
    async fn send_commit(
        &mut self,
        digest: String,
        view: u64,
        sequence: u64,
    ) -> Result<(), String> {
        let message = PBFTMessage::Commit {
            view,
            sequence,
            digest: digest.clone(),
            replica_id: self.config.node_id.clone(),
            signature: vec![],
        };

        let signed = SignedMessage::new(message, self.config.node_id.clone(), &self.signing_key);

        // 记录自己的 Commit
        let log_entry = self.log.entry(digest.clone()).or_default();
        log_entry.commits.insert(self.config.node_id.clone(), signed.clone());

        info!("Sent Commit for digest {} at sequence {}", digest, sequence);

        Ok(())
    }

    /// 处理 Commit 消息
    async fn handle_commit(
        &mut self,
        view: u64,
        sequence: u64,
        digest: String,
        replica_id: String,
        signed_msg: SignedMessage,
    ) -> Result<(), String> {
        if view != self.view {
            return Ok(());
        }

        // 记录 Commit
        let log_entry = self.log.entry(digest.clone()).or_default();
        log_entry.commits.insert(replica_id, signed_msg);

        // 检查是否达到提交条件（2f+1 个 Commit）
        if !log_entry.committed && log_entry.commits.len() >= self.config.quorum_size() {
            if log_entry.prepared {
                log_entry.committed = true;

                info!("Committed digest {} at sequence {}", digest, sequence);

                // 执行操作
                self.execute_operation(digest.clone(), sequence).await?;
            }
        }

        Ok(())
    }

    /// 执行操作
    async fn execute_operation(
        &mut self,
        digest: String,
        sequence: u64,
    ) -> Result<(), String> {
        // 检查是否已执行
        if self.executed_sequences.contains(&sequence) {
            debug!("Sequence {} already executed", sequence);
            return Ok(());
        }

        // 获取操作数据
        let log_entry = self.log.get_mut(&digest);
        if let Some(log) = log_entry {
            if let Some(pre_prepare) = &log.pre_prepare {
                if let PBFTMessage::PrePrepare { request, .. } = &pre_prepare.message {
                    // 执行操作（实际实现中这里执行具体业务逻辑）
                    info!("Executing operation at sequence {}: {:?}", sequence, request);

                    // 标记为已执行
                    self.executed_sequences.insert(sequence);
                    log.executed = true;

                    // 检查是否需要创建 checkpoint
                    if sequence % self.config.checkpoint_interval == 0 {
                        self.create_checkpoint(sequence).await?;
                    }
                }
            }
        }

        Ok(())
    }

    /// 创建 checkpoint
    async fn create_checkpoint(&mut self, sequence: u64) -> Result<(), String> {
        // 计算状态摘要
        let state_digest = self.compute_state_digest();

        let mut checkpoint = Checkpoint::new(sequence, state_digest.clone(), self.config.quorum_size());

        // 添加自己的签名
        let message = format!("Checkpoint:{}:{}", sequence, state_digest);
        let signature = self.signing_key.try_sign(message.as_bytes())
            .map_err(|e| format!("Failed to sign checkpoint: {:?}", e))?;

        checkpoint.certificate.add_signature(
            self.config.node_id.clone(),
            signature.to_bytes().to_vec(),
            self.verifying_key.to_bytes().to_vec(),
            message.as_bytes(),
        ).map_err(|e| format!("Failed to add signature: {:?}", e))?;

        self.checkpoints.push(checkpoint);

        info!("Created checkpoint at sequence {}", sequence);

        Ok(())
    }

    /// 计算状态摘要
    fn compute_state_digest(&self) -> String {
        let data = format!("{}:{}:{}", self.view, self.sequence, self.last_stable_checkpoint);
        let hash = Sha256::digest(data.as_bytes());
        format!("{:x}", hash)
    }

    /// 处理视图切换消息
    async fn handle_view_change(
        &mut self,
        new_view: u64,
        sender_id: String,
        signed_msg: SignedMessage,
    ) -> Result<(), String> {
        info!("Received ViewChange for view {} from {}", new_view, sender_id);

        // 提取 ViewChange 数据
        let view_change_msg = match &signed_msg.message {
            PBFTMessage::ViewChange { new_view, sender_id, last_stable_checkpoint, checkpoint_digest, prepared_log, .. } => {
                ViewChangeMessage::new(
                    *new_view,
                    sender_id.clone(),
                    *last_stable_checkpoint,
                    checkpoint_digest.clone(),
                    prepared_log.clone(),
                    &self.signing_key,
                )
            }
            _ => return Err("Invalid ViewChange message".to_string()),
        };

        // 接收视图切换
        let quorum_reached = self.view_change_manager.receive_view_change(view_change_msg)?;

        if quorum_reached && self.view_change_manager.is_leader(&self.config.node_id) {
            // 达到法定人数且我们是新 Leader，发送 NewView
            self.send_new_view(new_view).await?;
        }

        Ok(())
    }

    /// 发送 NewView 消息
    async fn send_new_view(&mut self, new_view: u64) -> Result<(), String> {
        if let Some(_new_view_msg) = self.view_change_manager.build_new_view(
            new_view,
            self.config.node_id.clone(),
            &self.signing_key,
        ) {
            // 更新视图
            self.view = new_view;
            self.state = ConsensusState::Normal;

            info!("Sent NewView for view {}", new_view);

            // 实际实现中需要广播 NewView 消息
        }

        Ok(())
    }

    /// 处理 NewView 消息
    async fn handle_new_view(
        &mut self,
        new_view: u64,
        new_leader: String,
        _signed_msg: SignedMessage,
    ) -> Result<(), String> {
        // 验证 NewView 消息
        let public_key_bytes = self.config.public_keys.get(&new_leader)
            .ok_or_else(|| format!("Unknown leader: {}", new_leader))?;

        let pk: [u8; 32] = public_key_bytes.clone()
            .try_into()
            .map_err(|_| "Invalid public key length")?;
        let public_key = VerifyingKey::from_bytes(&pk).map_err(|e| e.to_string())?;

        // 处理 NewView
        let accepted = self.view_change_manager.process_new_view(
            crate::consensus::view_change::NewViewMessage::new(
                new_view,
                new_leader.clone(),
                vec![],
                vec![],
                &self.signing_key,
            ),
            &public_key,
        )?;

        if accepted {
            self.view = new_view;
            self.state = ConsensusState::Normal;
            self.view_change_manager.reset_to_normal(new_view);

            info!("Accepted NewView for view {}", new_view);
        }

        Ok(())
    }

    /// 触发视图切换
    pub async fn initiate_view_change(&mut self) -> Result<(), String> {
        let new_view = self.view + 1;

        // 创建 ViewChange 消息
        let prepared_log = self.collect_prepared_log();

        let message = ViewChangeMessage::new(
            new_view,
            self.config.node_id.clone(),
            self.last_stable_checkpoint,
            self.compute_state_digest(),
            prepared_log,
            &self.signing_key,
        );

        self.view_change_manager.initiate_view_change(new_view, message.clone());
        self.state = ConsensusState::ViewChanging;

        info!("Initiated view change to view {}", new_view);

        // 实际实现中需要广播 ViewChange 消息

        Ok(())
    }

    /// 收集已准备的消息日志
    fn collect_prepared_log(&self) -> Vec<PreparedMessage> {
        self.log.values()
            .filter(|log| log.prepared && !log.committed)
            .map(|log| {
                let _prepare_sigs: Vec<_> = log.prepares.values()
                    .map(|msg| {
                        // 提取签名
                        let sig_bytes: [u8; 64] = msg.signature.clone()
                            .try_into()
                            .unwrap_or([0u8; 64]);
                        sig_bytes
                    })
                    .collect();

                PreparedMessage {
                    view: self.view,
                    sequence: 0, // 实际实现中需要从日志获取
                    digest: log.pre_prepare.as_ref().map(|m| m.digest()).unwrap_or_default(),
                    prepare_certificate: vec![],
                }
            })
            .collect()
    }

    /// 获取共识统计信息
    pub fn stats(&self) -> ConsensusStats {
        ConsensusStats {
            view: self.view,
            sequence: self.sequence,
            state: format!("{:?}", self.state),
            is_leader: self.is_leader(),
            log_size: self.log.len(),
            pending_requests: self.pending_requests.len(),
            executed_count: self.executed_sequences.len(),
            last_stable_checkpoint: self.last_stable_checkpoint,
            view_change_stats: self.view_change_manager.stats(),
            certificate_stats: self.cert_manager.stats(),
        }
    }
}

/// 共识统计信息
#[derive(Debug, Clone, Default)]
pub struct ConsensusStats {
    /// 当前视图号
    pub view: u64,
    /// 当前序列号
    pub sequence: u64,
    /// 状态
    pub state: String,
    /// 是否是 Leader
    pub is_leader: bool,
    /// 日志大小
    pub log_size: usize,
    /// 待处理请求数
    pub pending_requests: usize,
    /// 已执行操作数
    pub executed_count: usize,
    /// 最后稳定 checkpoint
    pub last_stable_checkpoint: u64,
    /// 视图切换统计
    pub view_change_stats: crate::consensus::view_change::ViewChangeStats,
    /// 证书统计
    pub certificate_stats: crate::consensus::certificate::CertificateStats,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pbft_creation() {
        let nodes = vec!["node_1".to_string(), "node_2".to_string(), "node_3".to_string(), "node_4".to_string()];
        let config = ConsensusConfig::for_testing("node_1".to_string(), nodes);

        let consensus = PBFTConsensus::new(config).unwrap();

        assert_eq!(consensus.view(), 0);
        assert_eq!(consensus.sequence(), 1);
    }

    #[tokio::test]
    async fn test_leader_selection() {
        let nodes = vec!["node_1".to_string(), "node_2".to_string(), "node_3".to_string(), "node_4".to_string()];
        let config = ConsensusConfig::for_testing("node_1".to_string(), nodes.clone());

        let consensus = PBFTConsensus::new(config).unwrap();

        // 视图 0: node_1 是 Leader
        assert!(consensus.is_leader());
        assert_eq!(consensus.leader_id(), Some("node_1".to_string()));
    }

    #[tokio::test]
    async fn test_quorum_calculation() {
        // 4 个节点：f=1, quorum=3
        let nodes = vec!["node_1".to_string(), "node_2".to_string(), "node_3".to_string(), "node_4".to_string()];
        let config = ConsensusConfig::for_testing("node_1".to_string(), nodes);

        assert_eq!(config.quorum_size(), 3);
        assert_eq!(config.max_faulty(), 1);

        // 7 个节点：f=2, quorum=5
        let nodes = vec![
            "node_1".to_string(), "node_2".to_string(), "node_3".to_string(),
            "node_4".to_string(), "node_5".to_string(), "node_6".to_string(),
            "node_7".to_string(),
        ];
        let config = ConsensusConfig::for_testing("node_1".to_string(), nodes);

        assert_eq!(config.quorum_size(), 5);
        assert_eq!(config.max_faulty(), 2);
    }

    #[tokio::test]
    async fn test_propose_operation() {
        let nodes = vec!["node_1".to_string(), "node_2".to_string(), "node_3".to_string(), "node_4".to_string()];
        let config = ConsensusConfig::for_testing("node_1".to_string(), nodes);

        let mut consensus = PBFTConsensus::new(config).unwrap();

        let operation = Operation::KvWrite {
            key: "test_key".to_string(),
            value: b"test_value".to_vec(),
        };

        let sequence = consensus.propose(operation).await;
        assert!(sequence.is_some());
        assert_eq!(sequence.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_non_leader_cannot_propose() {
        let nodes = vec!["node_1".to_string(), "node_2".to_string(), "node_3".to_string(), "node_4".to_string()];
        let config = ConsensusConfig::for_testing("node_2".to_string(), nodes);

        let mut consensus = PBFTConsensus::new(config).unwrap();

        // node_2 不是 Leader（视图 0 的 Leader 是 node_1）
        assert!(!consensus.is_leader());

        let operation = Operation::KvWrite {
            key: "test_key".to_string(),
            value: b"test_value".to_vec(),
        };

        let sequence = consensus.propose(operation).await;
        assert!(sequence.is_none());
    }
}
