//! 节点层模块 - 可信管控中枢
//!
//! **核心定位**：联盟链可信管控核心，负责节点身份、信誉、授权管理
//!
//! # 核心职责
//!
//! 1. **节点身份/公钥/信誉管理**：节点注册、权限分级、信誉评分
//! 2. **推理提供商准入/调度/切换/惩罚**：提供商全生命周期管理
//! 3. **记忆层哈希校验/存证上链**：验证 KV 数据完整性
//! 4. **跨节点共识/仲裁**：多节点决策协调
//!
//! # 关键约束（企业级落地要求）
//!
//! - **无状态**：不存储任何上下文/KV，仅做管控
//! - **轻量逻辑**：所有操作 <5ms/次
//! - **异步上链**：不阻塞推理主流程
//!
//! # 架构依赖
//!
//! ```text
//! 节点层 → 不依赖 → 推理提供商/记忆层（仅做管控，不做执行）
//! 记忆层 → 依赖 → 节点层（哈希校验/存证上链）
//! 推理提供商 → 依赖 → 节点层（获取访问授权/上报指标）
//! ```

#[cfg(feature = "rpc")]
pub mod rpc_server;

use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::reputation::ReputationManager;

/// 节点角色枚举 - 联盟链标准权限分级
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NodeRole {
    /// 共识节点：可参与记忆层哈希校验/存证上链
    Consensus,
    /// 普通节点：仅可调度推理提供商/管理本地授权
    Regular,
    /// 监管节点：可审计所有操作记录，无执行权限
    Regulatory,
}

impl NodeRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeRole::Consensus => "consensus",
            NodeRole::Regular => "regular",
            NodeRole::Regulatory => "regulatory",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "consensus" => Some(NodeRole::Consensus),
            "regular" => Some(NodeRole::Regular),
            "regulatory" => Some(NodeRole::Regulatory),
            _ => None,
        }
    }
}

/// 节点身份记录 - 联盟链节点核心信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeIdentity {
    /// 节点唯一标识（永久不变）
    pub node_id: String,
    /// 节点地址（公钥/钱包地址）
    pub node_address: String,
    /// 节点角色/权限
    pub role: NodeRole,
    /// 节点公钥（Ed25519）
    pub public_key: String,
    /// 企业资质信息（可选）
    pub enterprise_info: Option<String>,
    /// 注册时间戳
    pub registered_at: u64,
    /// 是否激活
    pub is_active: bool,
}

impl NodeIdentity {
    pub fn new(
        node_id: String,
        node_address: String,
        role: NodeRole,
        public_key: String,
        enterprise_info: Option<String>,
    ) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        NodeIdentity {
            node_id,
            node_address,
            role,
            public_key,
            enterprise_info,
            registered_at: timestamp,
            is_active: true,
        }
    }

    /// 生成节点身份哈希（用于链上存证）
    pub fn hash(&self) -> String {
        let data = format!(
            "{}:{}:{}:{}:{}:{}",
            self.node_id,
            self.node_address,
            self.role.as_str(),
            self.public_key,
            self.registered_at,
            self.is_active
        );
        format!("{:x}", Sha256::digest(data.as_bytes()))
    }
}

/// 访问授权凭证 - 节点层签发给推理提供商的临时访问权限
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessCredential {
    /// 凭证 ID
    pub credential_id: String,
    /// 被授权的推理提供商 ID
    pub provider_id: String,
    /// 授权访问的记忆区块 ID 列表
    pub memory_block_ids: Vec<String>,
    /// 授权类型（只读/写入）
    pub access_type: AccessType,
    /// 有效期（Unix 时间戳）
    pub expires_at: u64,
    /// 签发节点 ID
    pub issuer_node_id: String,
    /// 节点层签名
    pub signature: String,
    /// 是否已撤销
    pub is_revoked: bool,
}

/// 访问类型枚举
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AccessType {
    /// 只读访问
    ReadOnly,
    /// 写入访问（仅限当前推理生成的新 KV）
    WriteOnly,
    /// 读写访问
    ReadWrite,
}

impl AccessCredential {
    pub fn new(
        provider_id: String,
        memory_block_ids: Vec<String>,
        access_type: AccessType,
        expires_in_secs: u64,
        issuer_node_id: String,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let credential_id = format!(
            "cred_{:x}",
            Sha256::digest(format!("{}:{}:{}", provider_id, issuer_node_id, now).as_bytes())
        );

        AccessCredential {
            credential_id,
            provider_id,
            memory_block_ids,
            access_type,
            expires_at: now + expires_in_secs,
            issuer_node_id,
            signature: String::new(), // 需要后续签名
            is_revoked: false,
        }
    }

    /// 检查凭证是否有效
    pub fn is_valid(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        !self.is_revoked && now < self.expires_at
    }

    /// 获取签名消息
    pub fn signing_message(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}:{}",
            self.credential_id,
            self.provider_id,
            self.memory_block_ids.join(","),
            self.access_type.as_str(),
            self.expires_at,
            self.issuer_node_id
        )
    }

    /// 对凭证进行签名（内部方法）
    pub fn sign(&mut self, private_key: &[u8]) -> Result<(), String> {
        use ed25519_dalek::{SigningKey, Signer};

        let signing_key = SigningKey::from_bytes(
            &private_key.try_into()
                .map_err(|_| "Invalid private key length (expected 32 bytes)")?
        );

        let message = self.signing_message();
        let signature = signing_key.sign(message.as_bytes());
        self.signature = hex::encode(signature.to_bytes());
        Ok(())
    }

    /// 验证凭证签名
    pub fn verify_signature(&self, public_key: &str) -> bool {
        use ed25519_dalek::{VerifyingKey, Verifier};

        let public_key_bytes = match hex::decode(public_key) {
            Ok(bytes) => bytes,
            Err(_) => return false,
        };

        let signature_bytes = match hex::decode(&self.signature) {
            Ok(bytes) => bytes,
            Err(_) => return false,
        };

        let verifying_key = match VerifyingKey::from_bytes(&public_key_bytes.try_into().unwrap_or([0u8; 32])) {
            Ok(key) => key,
            Err(_) => return false,
        };

        let signature = match ed25519_dalek::Signature::try_from(&signature_bytes[..]) {
            Ok(sig) => sig,
            Err(_) => return false,
        };

        verifying_key.verify(self.signing_message().as_bytes(), &signature).is_ok()
    }
}

impl AccessType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccessType::ReadOnly => "read_only",
            AccessType::WriteOnly => "write_only",
            AccessType::ReadWrite => "read_write",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "read_only" => Some(AccessType::ReadOnly),
            "write_only" => Some(AccessType::WriteOnly),
            "read_write" => Some(AccessType::ReadWrite),
            _ => None,
        }
    }
}

/// 推理提供商调度策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SchedulingStrategy {
    /// 按推理效率（token/s）优先
    EfficiencyFirst,
    /// 按质量得分（记忆校验通过率）优先
    QualityFirst,
    /// 按奖励回报率优先
    CostFirst,
    /// 综合评分（效率 + 质量 + 成本）
    Balanced,
}

impl Default for SchedulingStrategy {
    fn default() -> Self {
        SchedulingStrategy::Balanced
    }
}

/// 推理提供商记录 - 链上登记的提供商信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRecord {
    /// 提供商唯一标识
    pub provider_id: String,
    /// 提供商接口版本
    pub interface_version: String,
    /// 算力规格（token/s）
    pub compute_capacity: u64,
    /// 奖励分成比例（0.0-1.0）
    pub reward_ratio: f64,
    /// 当前状态
    pub status: ProviderStatus,
    /// 关联的节点 ID（调度方）
    pub scheduler_node_id: Option<String>,
    /// 注册时间
    pub registered_at: u64,
    /// 最后活跃时间
    pub last_active_at: Option<u64>,
    /// 累计推理次数
    pub total_inferences: u64,
    /// 质量得分（记忆校验通过率）
    pub quality_score: f64,
    /// 平均推理效率（token/s）
    pub avg_efficiency: f64,
    // ========================================================================
    // P2-4：质量历史扩展字段
    // ========================================================================
    
    /// 质量历史记录
    pub quality_history: QualityHistory,
    /// 可靠性指标
    pub reliability_metrics: ReliabilityMetrics,
}

/// 质量历史 - 记录提供商的质量表现
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QualityHistory {
    /// 质量验证总次数
    pub total_quality_checks: u64,
    /// 通过质量验证次数
    pub passed_quality_checks: u64,
    /// 平均质量分数（0.0 - 1.0）
    pub avg_quality_score: f64,
    /// 质量分数趋势（最近 10 次）
    pub recent_quality_scores: Vec<f64>,
    /// 质量问题记录
    pub quality_issues: Vec<QualityIssueRecord>,
}

/// 质量问题是记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityIssueRecord {
    /// 时间戳
    pub timestamp: u64,
    /// 问题类型
    pub issue_type: String,
    /// 严重程度（0.0 - 1.0）
    pub severity: f64,
    /// 问题描述
    pub description: String,
}

/// 可靠性指标 - 量化提供商的可靠程度
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReliabilityMetrics {
    /// 可用性（正常运行时间比例，0.0 - 1.0）
    pub availability: f64,
    /// 平均响应时间（毫秒）
    pub avg_response_time_ms: f64,
    /// 响应时间标准差（毫秒）
    pub response_time_std_ms: f64,
    /// 任务完成率（0.0 - 1.0）
    pub completion_rate: f64,
    /// 超时率（0.0 - 1.0）
    pub timeout_rate: f64,
    /// 错误率（0.0 - 1.0）
    pub error_rate: f64,
    /// 连续成功次数
    pub consecutive_successes: u64,
    /// 最大连续成功次数
    pub max_consecutive_successes: u64,
    /// SLA 达成率（0.0 - 1.0）
    pub sla_compliance: f64,
}

impl ReliabilityMetrics {
    /// 计算综合可靠性得分（0.0 - 1.0）
    pub fn compute_reliability_score(&self) -> f64 {
        // 加权平均：可用性 30% + 完成率 25% + SLA 20% + 低错误率 15% + 低超时率 10%
        let availability_score = self.availability;
        let completion_score = self.completion_rate;
        let sla_score = self.sla_compliance;
        let error_score = 1.0 - self.error_rate.min(1.0);
        let timeout_score = 1.0 - self.timeout_rate.min(1.0);
        
        availability_score * 0.30
            + completion_score * 0.25
            + sla_score * 0.20
            + error_score * 0.15
            + timeout_score * 0.10
    }
}

impl QualityHistory {
    /// 记录质量检查结果
    pub fn record_quality_check(&mut self, score: f64, passed: bool) {
        self.total_quality_checks += 1;
        if passed {
            self.passed_quality_checks += 1;
        }
        
        // 更新平均分数
        let n = self.total_quality_checks as f64;
        self.avg_quality_score = ((self.avg_quality_score * (n - 1.0)) + score) / n;
        
        // 更新最近记录（保留最近 10 次）
        self.recent_quality_scores.push(score);
        if self.recent_quality_scores.len() > 10 {
            self.recent_quality_scores.remove(0);
        }
    }
    
    /// 记录质量问题
    pub fn record_quality_issue(&mut self, issue_type: String, severity: f64, description: String) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        self.quality_issues.push(QualityIssueRecord {
            timestamp,
            issue_type,
            severity,
            description,
        });
        
        // 保留最近 50 条记录
        if self.quality_issues.len() > 50 {
            self.quality_issues.remove(0);
        }
    }
    
    /// 获取质量通过率
    pub fn pass_rate(&self) -> f64 {
        if self.total_quality_checks == 0 {
            1.0
        } else {
            self.passed_quality_checks as f64 / self.total_quality_checks as f64
        }
    }
}

impl ReliabilityMetrics {
    /// 记录响应时间
    pub fn record_response_time(&mut self, response_time_ms: f64) {
        // 更新平均响应时间
        let n = self.avg_response_time_ms;
        if n == 0.0 {
            self.avg_response_time_ms = response_time_ms;
            self.response_time_std_ms = 0.0;
        } else {
            let old_mean = self.avg_response_time_ms;
            self.avg_response_time_ms = ((old_mean * (n - 1.0)) + response_time_ms) / n;
            
            // 简化标准差计算
            self.response_time_std_ms = (self.response_time_std_ms + (response_time_ms - old_mean).abs()) / 2.0;
        }
    }
    
    /// 记录任务完成
    pub fn record_completion(&mut self, success: bool, timed_out: bool, error: bool) {
        if success {
            self.consecutive_successes += 1;
            self.max_consecutive_successes = self.max_consecutive_successes.max(self.consecutive_successes);
        } else {
            self.consecutive_successes = 0;
        }
        
        // 更新完成率
        let total = self.completion_rate;
        self.completion_rate = if success {
            (total * (self.completion_rate * 100.0) + 1.0) / (self.completion_rate * 100.0 + 1.0)
        } else {
            total * (self.completion_rate * 100.0) / (self.completion_rate * 100.0 + 1.0)
        };
        
        if timed_out {
            self.timeout_rate = (self.timeout_rate * 100.0 + 1.0) / 101.0;
        }
        
        if error {
            self.error_rate = (self.error_rate * 100.0 + 1.0) / 101.0;
        }
    }
}

/// 提供商状态枚举
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderStatus {
    /// 待审核
    Pending,
    /// 活跃（可被调度）
    Active,
    /// 暂停（冷却中/手动暂停）
    Suspended,
    /// 已剔除（严重违规）
    Blacklisted,
}

impl ProviderRecord {
    pub fn new(
        provider_id: String,
        interface_version: String,
        compute_capacity: u64,
        reward_ratio: f64,
    ) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        ProviderRecord {
            provider_id,
            interface_version,
            compute_capacity,
            reward_ratio,
            status: ProviderStatus::Pending,
            scheduler_node_id: None,
            registered_at: timestamp,
            last_active_at: None,
            total_inferences: 0,
            quality_score: 1.0,
            avg_efficiency: 0.0,
            quality_history: QualityHistory::default(),
            reliability_metrics: ReliabilityMetrics::default(),
        }
    }

    /// 计算综合评分（用于调度排序）
    pub fn compute_score(&self, strategy: &SchedulingStrategy) -> f64 {
        match strategy {
            SchedulingStrategy::EfficiencyFirst => self.avg_efficiency,
            SchedulingStrategy::QualityFirst => self.quality_score,
            SchedulingStrategy::CostFirst => self.reward_ratio,
            SchedulingStrategy::Balanced => {
                // 综合评分：效率 40% + 质量 40% + 成本 20%
                let efficiency_norm = self.avg_efficiency / 100.0; // 归一化
                let cost_norm = self.reward_ratio;
                self.quality_score * 0.4 + efficiency_norm * 0.4 + cost_norm * 0.2
            }
        }
    }
}

/// 节点层管理器 - 统一管理节点身份、授权、调度
#[derive(Debug)]
pub struct NodeLayerManager {
    /// 节点身份映射
    nodes: HashMap<String, NodeIdentity>,
    /// 推理提供商映射
    providers: HashMap<String, ProviderRecord>,
    /// 访问凭证映射
    credentials: HashMap<String, AccessCredential>,
    /// 节点信誉管理器（复用现有模块）
    reputation_manager: ReputationManager,
    /// 调度策略
    scheduling_strategy: SchedulingStrategy,
    /// 节点私钥（用于签名凭证）
    node_private_key: Option<[u8; 32]>,
    /// 节点公钥
    pub node_public_key: String,
}

impl Clone for NodeLayerManager {
    fn clone(&self) -> Self {
        NodeLayerManager {
            nodes: self.nodes.clone(),
            providers: self.providers.clone(),
            credentials: self.credentials.clone(),
            reputation_manager: self.reputation_manager.clone(),
            scheduling_strategy: self.scheduling_strategy.clone(),
            node_private_key: self.node_private_key,
            node_public_key: self.node_public_key.clone(),
        }
    }
}

impl NodeLayerManager {
    /// 创建新的节点层管理器
    pub fn new(_node_id: String, node_address: String) -> Self {
        let reputation_manager = ReputationManager::new(0.7, 0.6);

        NodeLayerManager {
            nodes: HashMap::new(),
            providers: HashMap::new(),
            credentials: HashMap::new(),
            reputation_manager,
            scheduling_strategy: SchedulingStrategy::default(),
            node_private_key: None,
            node_public_key: node_address,
        }
    }

    /// 设置节点密钥（用于签名凭证）
    pub fn set_keys(&mut self, private_key: [u8; 32], public_key: String) {
        self.node_private_key = Some(private_key);
        self.node_public_key = public_key;
    }

    /// 注册节点身份
    pub fn register_node(&mut self, identity: NodeIdentity) -> Result<(), String> {
        if self.nodes.contains_key(&identity.node_id) {
            return Err(format!("Node {} already registered", identity.node_id));
        }

        self.nodes.insert(identity.node_id.clone(), identity);
        Ok(())
    }

    /// 获取节点身份
    pub fn get_node(&self, node_id: &str) -> Option<&NodeIdentity> {
        self.nodes.get(node_id)
    }

    /// 获取所有活跃节点
    pub fn get_active_nodes(&self) -> Vec<&NodeIdentity> {
        self.nodes.values()
            .filter(|n| n.is_active)
            .collect()
    }

    /// 获取共识节点列表
    pub fn get_consensus_nodes(&self) -> Vec<&NodeIdentity> {
        self.nodes.values()
            .filter(|n| n.is_active && n.role == NodeRole::Consensus)
            .collect()
    }

    /// 注册推理提供商
    pub fn register_provider(&mut self, record: ProviderRecord) -> Result<(), String> {
        if self.providers.contains_key(&record.provider_id) {
            return Err(format!("Provider {} already registered", record.provider_id));
        }

        self.providers.insert(record.provider_id.clone(), record);
        Ok(())
    }

    /// 获取提供商记录
    pub fn get_provider(&self, provider_id: &str) -> Option<&ProviderRecord> {
        self.providers.get(provider_id)
    }

    /// 获取提供商记录（可变引用）
    pub fn get_provider_mut(&mut self, provider_id: &str) -> Option<&mut ProviderRecord> {
        self.providers.get_mut(provider_id)
    }

    /// 获取所有活跃提供商
    pub fn get_active_providers(&self) -> Vec<&ProviderRecord> {
        self.providers.values()
            .filter(|p| p.status == ProviderStatus::Active)
            .collect()
    }

    /// 更新提供商状态
    pub fn update_provider_status(
        &mut self,
        provider_id: &str,
        status: ProviderStatus,
    ) -> Result<(), String> {
        let provider = self.providers.get_mut(provider_id)
            .ok_or_else(|| format!("Provider {} not found", provider_id))?;

        provider.status = status;
        Ok(())
    }

    /// 签发访问凭证
    pub fn issue_credential(
        &mut self,
        provider_id: String,
        memory_block_ids: Vec<String>,
        access_type: AccessType,
        expires_in_secs: u64,
    ) -> Result<AccessCredential, String> {
        let issuer_node_id = self.node_public_key.clone(); // 使用公钥作为标识

        let mut credential = AccessCredential::new(
            provider_id,
            memory_block_ids,
            access_type,
            expires_in_secs,
            issuer_node_id,
        );

        // 签名凭证
        if let Some(private_key) = self.node_private_key {
            credential.sign(&private_key)?;
        } else {
            // 无密钥时使用简单签名（测试模式）
            credential.signature = format!("{:x}", Sha256::digest(credential.signing_message().as_bytes()));
        }

        let credential_id = credential.credential_id.clone();
        self.credentials.insert(credential_id, credential.clone());

        Ok(credential)
    }

    /// 验证访问凭证
    pub fn verify_credential(&self, credential: &AccessCredential) -> bool {
        // 检查凭证是否存在
        if !self.credentials.contains_key(&credential.credential_id) {
            return false;
        }

        // 检查凭证是否有效
        if !credential.is_valid() {
            return false;
        }

        // 验证签名
        let stored_credential = self.credentials.get(&credential.credential_id).unwrap();
        if stored_credential.signature != credential.signature {
            return false;
        }

        true
    }

    /// 撤销访问凭证
    pub fn revoke_credential(&mut self, credential_id: &str) -> Result<(), String> {
        let credential = self.credentials.get_mut(credential_id)
            .ok_or_else(|| format!("Credential {} not found", credential_id))?;

        // 如果已经撤销，返回错误
        if credential.is_revoked {
            return Err(format!("Credential {} is already revoked", credential_id));
        }

        credential.is_revoked = true;
        Ok(())
    }

    /// 选择最佳推理提供商（基于调度策略）
    pub fn select_best_provider(&self) -> Option<&ProviderRecord> {
        let active_providers = self.get_active_providers();

        if active_providers.is_empty() {
            return None;
        }

        active_providers
            .into_iter()
            .max_by(|a, b| {
                let score_a = a.compute_score(&self.scheduling_strategy);
                let score_b = b.compute_score(&self.scheduling_strategy);
                score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// 设置调度策略
    pub fn set_scheduling_strategy(&mut self, strategy: SchedulingStrategy) {
        self.scheduling_strategy = strategy;
    }

    /// 报告提供商推理指标
    pub fn report_provider_metrics(
        &mut self,
        provider_id: &str,
        efficiency: f64,
        success: bool,
    ) -> Result<(), String> {
        let provider = self.providers.get_mut(provider_id)
            .ok_or_else(|| format!("Provider {} not found", provider_id))?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        provider.last_active_at = Some(now);
        provider.total_inferences += 1;

        // 更新质量得分（指数移动平均）
        if success {
            provider.quality_score = provider.quality_score * 0.9 + 1.0 * 0.1;
        } else {
            provider.quality_score = provider.quality_score * 0.9 + 0.0 * 0.1;
        }

        // 更新平均效率（指数移动平均）
        provider.avg_efficiency = provider.avg_efficiency * 0.9 + efficiency * 0.1;

        Ok(())
    }

    /// 获取信誉管理器（只读）
    pub fn reputation_manager(&self) -> &ReputationManager {
        &self.reputation_manager
    }

    /// 获取信誉管理器（可变引用）
    pub fn reputation_manager_mut(&mut self) -> &mut ReputationManager {
        &mut self.reputation_manager
    }

    /// 获取节点数量
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// 获取提供商数量
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// 哈希校验（验证记忆层数据完整性）
    pub fn verify_memory_hash(&self, kv_data: &[u8], expected_hash: &str) -> bool {
        let computed_hash = format!("{:x}", Sha256::digest(kv_data));
        computed_hash == expected_hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_identity_creation() {
        let identity = NodeIdentity::new(
            "node_1".to_string(),
            "address_123".to_string(),
            NodeRole::Consensus,
            "pubkey_abc".to_string(),
            Some("Enterprise Info".to_string()),
        );

        assert_eq!(identity.node_id, "node_1");
        assert_eq!(identity.role, NodeRole::Consensus);
        assert!(identity.is_active);
        assert!(identity.registered_at > 0);
    }

    #[test]
    fn test_access_credential_validity() {
        let mut manager = NodeLayerManager::new("node_1".to_string(), "address_123".to_string());

        let credential = manager.issue_credential(
            "provider_1".to_string(),
            vec!["mem_1".to_string(), "mem_2".to_string()],
            AccessType::ReadOnly,
            3600, // 1 小时有效期
        ).unwrap();

        assert!(credential.is_valid());
        assert!(!credential.is_revoked);
    }

    #[test]
    fn test_provider_scheduling() {
        let mut manager = NodeLayerManager::new("node_1".to_string(), "address_123".to_string());

        // 注册两个提供商
        let mut provider1 = ProviderRecord::new(
            "provider_1".to_string(),
            "v1.0".to_string(),
            100, // 100 token/s
            0.1, // 10% 分成
        );
        provider1.status = ProviderStatus::Active;
        provider1.avg_efficiency = 95.0;
        provider1.quality_score = 0.98;

        let mut provider2 = ProviderRecord::new(
            "provider_2".to_string(),
            "v1.0".to_string(),
            80,
            0.05,
        );
        provider2.status = ProviderStatus::Active;
        provider2.avg_efficiency = 85.0;
        provider2.quality_score = 0.95;

        manager.register_provider(provider1).unwrap();
        manager.register_provider(provider2).unwrap();

        // 选择最佳提供商（综合评分）
        let best = manager.select_best_provider().unwrap();
        assert_eq!(best.provider_id, "provider_1");
    }

    #[test]
    fn test_credential_revocation() {
        let mut manager = NodeLayerManager::new("node_1".to_string(), "address_123".to_string());

        let credential = manager.issue_credential(
            "provider_1".to_string(),
            vec!["mem_1".to_string()],
            AccessType::ReadOnly,
            3600,
        ).unwrap();

        let credential_id = credential.credential_id.clone();

        // 撤销凭证
        manager.revoke_credential(&credential_id).unwrap();

        // 验证凭证已失效
        let revoked_credential = manager.credentials.get(&credential_id).unwrap();
        assert!(revoked_credential.is_revoked);
        assert!(!revoked_credential.is_valid());
    }

    #[test]
    fn test_provider_metrics_update() {
        let mut manager = NodeLayerManager::new("node_1".to_string(), "address_123".to_string());

        let provider = ProviderRecord::new(
            "provider_1".to_string(),
            "v1.0".to_string(),
            100,
            0.1,
        );
        manager.register_provider(provider).unwrap();

        // 报告多次推理指标
        for i in 0..10 {
            manager.report_provider_metrics("provider_1", 90.0 + i as f64, i % 2 == 0).unwrap();
        }

        let provider = manager.get_provider("provider_1").unwrap();
        assert_eq!(provider.total_inferences, 10);
        assert!(provider.last_active_at.is_some());
        assert!(provider.quality_score < 1.0);
        // 指数移动平均：初始 0，每次更新 0.9 * old + 0.1 * new
        // 经过 10 次更新后，avg_efficiency 应该是一个合理的值
        assert!(provider.avg_efficiency > 50.0);
        assert!(provider.avg_efficiency < 100.0);
    }

    #[test]
    fn test_hash_verification() {
        let manager = NodeLayerManager::new("node_1".to_string(), "address_123".to_string());

        let data = b"test_kv_data";
        let hash = format!("{:x}", Sha256::digest(data));

        assert!(manager.verify_memory_hash(data, &hash));

        let tampered_data = b"tampered_data";
        assert!(!manager.verify_memory_hash(tampered_data, &hash));
    }
}
