//! 视图切换模块 - 处理 Leader 作恶场景
//!
//! **核心功能**：
//! - 检测 Leader 不作为或作恶
//! - 协调节点切换到新视图
//! - 保证视图切换过程中的安全性
//!
//! **视图切换触发条件**：
//! - Leader 超时未发送 Pre-prepare
//! - 收到 2f+1 个 ViewChange 消息
//! - 检测到 Byzantine 行为
//!
//! **视图切换流程**：
//! 1. 节点发送 ViewChange 消息
//! 2. 新 Leader 收集 2f+1 个 ViewChange
//! 3. 新 Leader 发送 NewView 消息
//! 4. 节点验证并切换到新视图

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer, Verifier};

use crate::consensus::messages::{
    ViewChangeData, PrePrepareData, PreparedMessage,
};

/// 视图切换状态
#[derive(Debug, Clone, PartialEq)]
pub enum ViewChangeState {
    /// 正常状态
    Normal,
    /// 正在视图切换中
    ViewChanging { new_view: u64 },
    /// 视图切换完成
    ViewChanged { new_view: u64 },
}

/// 视图切换管理器
#[derive(Debug, Clone)]
pub struct ViewChangeManager {
    /// 当前视图号
    current_view: u64,
    /// 视图切换状态
    state: ViewChangeState,
    /// 收到的 ViewChange 消息 (new_view -> sender_id -> message)
    view_changes: HashMap<u64, HashMap<String, ViewChangeMessage>>,
    /// 节点列表
    nodes: Vec<String>,
    /// 法定人数大小 (2f+1)
    quorum_size: usize,
    /// 视图切换超时时间（毫秒）
    view_change_timeout_ms: u64,
    /// 最后活动时间戳
    last_activity_timestamp: u64,
}

/// 视图切换消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewChangeMessage {
    /// 新视图号
    pub new_view: u64,
    /// 发送者 ID
    pub sender_id: String,
    /// 最后稳定 checkpoint 序列号
    pub last_stable_checkpoint: u64,
    /// checkpoint 摘要
    pub checkpoint_digest: String,
    /// 已准备但未提交的消息
    pub prepared_log: Vec<PreparedMessage>,
    /// 签名
    pub signature: Vec<u8>,
    /// 时间戳
    pub timestamp: u64,
}

impl ViewChangeMessage {
    /// 创建视图切换消息
    pub fn new(
        new_view: u64,
        sender_id: String,
        last_stable_checkpoint: u64,
        checkpoint_digest: String,
        prepared_log: Vec<PreparedMessage>,
        signing_key: &SigningKey,
    ) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let message_data = format!(
            "ViewChange:{}:{}:{}:{}",
            new_view,
            sender_id,
            last_stable_checkpoint,
            checkpoint_digest
        );
        let signature = signing_key.try_sign(message_data.as_bytes()).unwrap();

        ViewChangeMessage {
            new_view,
            sender_id,
            last_stable_checkpoint,
            checkpoint_digest,
            prepared_log,
            signature: signature.to_bytes().to_vec(),
            timestamp,
        }
    }

    /// 验证签名
    pub fn verify(&self, public_key: &VerifyingKey) -> bool {
        let message_data = format!(
            "ViewChange:{}:{}:{}:{}",
            self.new_view,
            self.sender_id,
            self.last_stable_checkpoint,
            self.checkpoint_digest
        );

        match Signature::try_from(&self.signature[..]) {
            Ok(signature) => public_key.verify(message_data.as_bytes(), &signature).is_ok(),
            Err(_) => false,
        }
    }

    /// 转换为 ViewChangeData
    pub fn to_data(&self) -> ViewChangeData {
        ViewChangeData {
            new_view: self.new_view,
            sender_id: self.sender_id.clone(),
            last_stable_checkpoint: self.last_stable_checkpoint,
            checkpoint_digest: self.checkpoint_digest.clone(),
            signature: self.signature.clone(),
        }
    }
}

/// NewView 消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewViewMessage {
    /// 新视图号
    pub new_view: u64,
    /// 新 Leader ID
    pub new_leader: String,
    /// ViewChange 消息集合
    pub view_changes: Vec<ViewChangeData>,
    /// PrePrepare 消息集合
    pub pre_prepares: Vec<PrePrepareData>,
    /// 签名
    pub signature: Vec<u8>,
}

impl NewViewMessage {
    /// 创建 NewView 消息
    pub fn new(
        new_view: u64,
        new_leader: String,
        view_changes: Vec<ViewChangeData>,
        pre_prepares: Vec<PrePrepareData>,
        signing_key: &SigningKey,
    ) -> Self {
        let message_data = format!("NewView:{}:{}", new_view, new_leader);
        let signature = signing_key.try_sign(message_data.as_bytes()).unwrap();

        NewViewMessage {
            new_view,
            new_leader,
            view_changes,
            pre_prepares,
            signature: signature.to_bytes().to_vec(),
        }
    }

    /// 验证签名
    pub fn verify(&self, public_key: &VerifyingKey) -> bool {
        let message_data = format!("NewView:{}:{}", self.new_view, self.new_leader);

        match Signature::try_from(&self.signature[..]) {
            Ok(signature) => public_key.verify(message_data.as_bytes(), &signature).is_ok(),
            Err(_) => false,
        }
    }
}

impl ViewChangeManager {
    /// 创建新的视图切换管理器
    pub fn new(nodes: Vec<String>, quorum_size: usize, view_change_timeout_ms: u64) -> Self {
        ViewChangeManager {
            current_view: 0,
            state: ViewChangeState::Normal,
            view_changes: HashMap::new(),
            nodes,
            quorum_size,
            view_change_timeout_ms,
            last_activity_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// 获取当前视图号
    pub fn current_view(&self) -> u64 {
        self.current_view
    }

    /// 获取当前状态
    pub fn state(&self) -> &ViewChangeState {
        &self.state
    }

    /// 更新活动时间戳
    pub fn update_activity(&mut self) {
        self.last_activity_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }

    /// 检查是否超时
    pub fn is_timeout(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now - self.last_activity_timestamp > self.view_change_timeout_ms / 1000
    }

    /// 发起视图切换
    pub fn initiate_view_change(&mut self, new_view: u64, message: ViewChangeMessage) {
        self.state = ViewChangeState::ViewChanging { new_view };

        self.view_changes
            .entry(new_view)
            .or_insert_with(HashMap::new)
            .insert(message.sender_id.clone(), message);
    }

    /// 接收 ViewChange 消息
    ///
    /// # Returns
    /// - `Ok(Some(quorum_reached))`: 消息处理成功，返回是否达到法定人数
    /// - `Err`: 消息无效
    pub fn receive_view_change(
        &mut self,
        message: ViewChangeMessage,
    ) -> Result<bool, String> {
        // 验证视图号
        if message.new_view <= self.current_view {
            return Err(format!(
                "Invalid view number: {} (current: {})",
                message.new_view, self.current_view
            ));
        }

        // 添加到收集器
        let view_map = self.view_changes
            .entry(message.new_view)
            .or_insert_with(HashMap::new);

        view_map.insert(message.sender_id.clone(), message);

        // 检查是否达到法定人数
        let count = view_map.len();
        if count >= self.quorum_size {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// 构建 NewView 消息（仅 Leader 调用）
    ///
    /// # Returns
    /// - `Some(NewViewMessage)`: 成功构建
    /// - `None`: 视图切换未准备好
    pub fn build_new_view(
        &mut self,
        new_view: u64,
        new_leader: String,
        signing_key: &SigningKey,
    ) -> Option<NewViewMessage> {
        // 检查是否有足够的 ViewChange 消息
        let view_changes_map = self.view_changes.get(&new_view)?;

        if view_changes_map.len() < self.quorum_size {
            return None;
        }

        // 收集 ViewChange 数据
        let view_changes: Vec<ViewChangeData> = view_changes_map
            .values()
            .take(self.quorum_size)
            .map(|vc| vc.to_data())
            .collect();

        // 构建 PrePrepare 消息
        // 需要重放未提交的操作
        let mut pre_prepares = Vec::new();
        let mut sequence = 0;

        for vc in view_changes_map.values() {
            for prepared in &vc.prepared_log {
                // 跳过已处理的序列号
                if prepared.sequence > sequence {
                    sequence = prepared.sequence;

                    // 创建 PrePrepare 数据
                    pre_prepares.push(PrePrepareData {
                        view: new_view,
                        sequence: prepared.sequence,
                        digest: prepared.digest.clone(),
                        request: vec![], // 实际实现中需要从日志中获取
                    });
                }
            }
        }

        // 更新状态
        self.current_view = new_view;
        self.state = ViewChangeState::ViewChanged { new_view };
        self.update_activity();

        Some(NewViewMessage::new(
            new_view,
            new_leader,
            view_changes,
            pre_prepares,
            signing_key,
        ))
    }

    /// 处理 NewView 消息
    ///
    /// # Returns
    /// - `Ok(true)`: 验证通过，接受新视图
    /// - `Ok(false)`: 验证失败
    /// - `Err`: 处理错误
    pub fn process_new_view(
        &mut self,
        new_view: NewViewMessage,
        public_key: &VerifyingKey,
    ) -> Result<bool, String> {
        // 验证签名
        if !new_view.verify(public_key) {
            return Err("Invalid NewView signature".to_string());
        }

        // 验证视图号
        if new_view.new_view <= self.current_view {
            return Err(format!(
                "NewView has old view number: {} (current: {})",
                new_view.new_view, self.current_view
            ));
        }

        // 验证 ViewChange 数量
        if new_view.view_changes.len() < self.quorum_size {
            return Err(format!(
                "Insufficient ViewChange messages: {} (required: {})",
                new_view.view_changes.len(),
                self.quorum_size
            ));
        }

        // 接受新视图
        self.current_view = new_view.new_view;
        self.state = ViewChangeState::ViewChanged {
            new_view: new_view.new_view,
        };
        self.update_activity();

        // 清理旧的 ViewChange 消息
        self.view_changes.retain(|&view, _| view >= new_view.new_view);

        Ok(true)
    }

    /// 获取当前 Leader
    ///
    /// Leader 选择：按节点列表轮转
    pub fn get_current_leader(&self) -> Option<String> {
        if self.nodes.is_empty() {
            return None;
        }

        let leader_index = (self.current_view as usize) % self.nodes.len();
        Some(self.nodes[leader_index].clone())
    }

    /// 检查是否是当前 Leader
    pub fn is_leader(&self, node_id: &str) -> bool {
        self.get_current_leader().map_or(false, |leader| leader == node_id)
    }

    /// 重置为正常状态
    pub fn reset_to_normal(&mut self, view: u64) {
        self.current_view = view;
        self.state = ViewChangeState::Normal;
        self.update_activity();
    }

    /// 获取视图切换统计信息
    pub fn stats(&self) -> ViewChangeStats {
        let total_view_changes: usize = self.view_changes.values().map(|m| m.len()).sum();

        ViewChangeStats {
            current_view: self.current_view,
            state: format!("{:?}", self.state),
            total_view_changes,
            is_timeout: self.is_timeout(),
            current_leader: self.get_current_leader(),
        }
    }
}

/// 视图切换统计信息
#[derive(Debug, Clone, Default)]
pub struct ViewChangeStats {
    /// 当前视图号
    pub current_view: u64,
    /// 状态描述
    pub state: String,
    /// ViewChange 消息总数
    pub total_view_changes: usize,
    /// 是否超时
    pub is_timeout: bool,
    /// 当前 Leader
    pub current_leader: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    #[test]
    fn test_view_change_manager_creation() {
        let nodes = vec!["node_1".to_string(), "node_2".to_string(), "node_3".to_string()];
        let manager = ViewChangeManager::new(nodes.clone(), 2, 5000);

        assert_eq!(manager.current_view(), 0);
        assert_eq!(manager.state(), &ViewChangeState::Normal);
    }

    #[test]
    fn test_leader_selection() {
        let nodes = vec!["node_1".to_string(), "node_2".to_string(), "node_3".to_string()];
        let mut manager = ViewChangeManager::new(nodes.clone(), 2, 5000);

        // 视图 0: node_1 是 Leader
        assert_eq!(manager.get_current_leader(), Some("node_1".to_string()));
        assert!(manager.is_leader("node_1"));

        // 视图 1: node_2 是 Leader
        manager.current_view = 1;
        assert_eq!(manager.get_current_leader(), Some("node_2".to_string()));
        assert!(manager.is_leader("node_2"));

        // 视图 3: node_1 又是 Leader
        manager.current_view = 3;
        assert_eq!(manager.get_current_leader(), Some("node_1".to_string()));
    }

    #[test]
    fn test_view_change_message_creation() {
        let key_bytes: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&key_bytes);

        let message = ViewChangeMessage::new(
            1,
            "node_1".to_string(),
            100,
            "checkpoint_digest".to_string(),
            vec![],
            &signing_key,
        );

        assert_eq!(message.new_view, 1);
        assert_eq!(message.sender_id, "node_1");
        assert!(!message.signature.is_empty());

        // 验证签名
        assert!(message.verify(&signing_key.verifying_key()));
    }

    #[test]
    fn test_receive_view_change() {
        let nodes = vec!["node_1".to_string(), "node_2".to_string(), "node_3".to_string()];
        let mut manager = ViewChangeManager::new(nodes, 2, 5000);

        let key_bytes: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&key_bytes);

        let message = ViewChangeMessage::new(
            1,
            "node_1".to_string(),
            100,
            "digest".to_string(),
            vec![],
            &signing_key,
        );

        // 接收第一个 ViewChange
        let result = manager.receive_view_change(message);
        assert!(result.is_ok());
        assert!(!result.unwrap()); // 未达到法定人数

        // 接收第二个 ViewChange（达到法定人数）
        let key_bytes2: [u8; 32] = rand::random();
        let signing_key2 = SigningKey::from_bytes(&key_bytes2);
        let message2 = ViewChangeMessage::new(
            1,
            "node_2".to_string(),
            100,
            "digest".to_string(),
            vec![],
            &signing_key2,
        );

        let result = manager.receive_view_change(message2);
        assert!(result.is_ok());
        assert!(result.unwrap()); // 达到法定人数
    }

    #[test]
    fn test_old_view_change_rejected() {
        let nodes = vec!["node_1".to_string()];
        let mut manager = ViewChangeManager::new(nodes, 1, 5000);
        manager.current_view = 5;

        let key_bytes: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&key_bytes);

        let message = ViewChangeMessage::new(
            3, // 旧视图号
            "node_1".to_string(),
            100,
            "digest".to_string(),
            vec![],
            &signing_key,
        );

        let result = manager.receive_view_change(message);
        assert!(result.is_err());
    }

    #[test]
    fn test_new_view_message_creation() {
        let key_bytes: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&key_bytes);

        let new_view = NewViewMessage::new(
            1,
            "node_1".to_string(),
            vec![],
            vec![],
            &signing_key,
        );

        assert_eq!(new_view.new_view, 1);
        assert_eq!(new_view.new_leader, "node_1");
        assert!(!new_view.signature.is_empty());

        // 验证签名
        assert!(new_view.verify(&signing_key.verifying_key()));
    }

    #[test]
    fn test_view_change_timeout() {
        let nodes = vec!["node_1".to_string()];
        let manager = ViewChangeManager::new(nodes, 1, 50); // 50ms 超时

        // 初始不超时
        assert!(!manager.is_timeout());

        // 等待超时（使用更长的等待时间确保可靠）
        std::thread::sleep(std::time::Duration::from_millis(100));

        // 检查超时（重试几次以确保可靠性）
        for _ in 0..3 {
            if manager.is_timeout() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        
        // 如果多次重试后仍然不超时，则失败
        assert!(manager.is_timeout(), "View change should timeout");
    }
}
