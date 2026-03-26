//! Gossip 同步协议 - KV 分片最终一致性同步
//!
//! **架构定位**：
//! - 真正的分布式 KV 同步协议
//! - 解决 replica_locations 只是列表的问题
//! - 实现跨节点 KV 数据最终一致性
//!
//! **核心特性**：
//! - Vector Clock 解决冲突
//! - Gossip 协议最终一致性
//! - Merkle Tree 验证完整性
//!
//! **同步流程**：
//! 1. 节点随机选择 peer 节点
//! 2. 比较 Vector Clock 判断数据新旧
//! 3. 推送/拉取更新数据
//! 4. Merkle Tree 验证完整性
//!
//! ⚠️ **生产就绪度说明**
//!
//! 当前实现是**Gossip 同步协议原型**，适用于：
//! - ✅ 架构验证/技术演示
//! - ✅ 学习 Gossip 协议和 Vector Clock 机制
//! - ✅ 小规模测试环境（≤5 节点）
//!
//! ❌ **当前实现与生产级的差距**：
//! - 缺少真实网络层（使用内存模拟节点间通信）
//! - 缺少抗 Sybil 攻击机制（无节点身份验证）
//! - 缺少网络分区处理（假设网络始终连通）
//! - 缺少持久化（节点重启后数据丢失）
//!
//! 🔧 **生产环境建议**：
//! - 集成 [Scuttlebutt](https://en.wikipedia.org/wiki/Gossip_protocol) 或 [HyParView](https://asc.di.fct.unl.pt/~jleitao/pdf/dsn07-leitao.pdf) 协议
//! - 使用 [libp2p](https://libp2p.io/) 实现真实 P2P 网络
//! - 添加节点身份认证和消息签名
//! - 实现数据持久化和恢复机制
//!
//! **v0.4.1 更新**：
//! - ✅ 添加网络接口 trait，支持切换不同网络实现
//! - ✅ 支持内存模拟（测试）和 gRPC（生产）两种模式
//! - ✅ 通过 feature flag 控制网络层选择

use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use std::collections::{HashMap, HashSet, BTreeMap};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::interval;
use log::{info, debug, warn, error};

/// Gossip 网络接口 trait
///
/// 定义了 Gossip 同步所需的网络通信能力
#[tonic::async_trait]
pub trait GossipNetwork: Send + Sync {
    /// 推送 Gossip 消息
    async fn gossip(&self, data: GossipMessage) -> Result<(), String>;
    
    /// 选择 Gossip peer（用于扇出）
    fn select_peers(&self, fanout: usize) -> Vec<String>;
}

/// 内存 Gossip 网络实现（用于测试）
pub struct MemoryGossipNetwork;

#[tonic::async_trait]
impl GossipNetwork for MemoryGossipNetwork {
    async fn gossip(&self, data: GossipMessage) -> Result<(), String> {
        debug!("MemoryGossipNetwork: gossip message for shard {}", data.shard_id);
        Ok(())
    }
    
    fn select_peers(&self, _fanout: usize) -> Vec<String> {
        Vec::new()
    }
}

/// Vector Clock - 向量时钟
///
/// 用于解决分布式系统中的数据冲突
///
/// **设计原理**：
/// - 每个节点维护一个计数器
/// - 每次本地更新时递增自己的计数器
/// - 比较两个 Vector Clock 可以判断因果关系
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VectorClock {
    /// 节点 ID -> 计数器
    clocks: BTreeMap<String, u64>,
}

impl VectorClock {
    /// 创建空的 Vector Clock
    pub fn new() -> Self {
        VectorClock {
            clocks: BTreeMap::new(),
        }
    }

    /// 创建带初始节点的 Vector Clock
    pub fn with_node(node_id: String) -> Self {
        let mut clocks = BTreeMap::new();
        clocks.insert(node_id, 1);
        VectorClock { clocks }
    }

    /// 递增指定节点的计数器
    pub fn increment(&mut self, node_id: &str) {
        *self.clocks.entry(node_id.to_string()).or_insert(0) += 1;
    }

    /// 获取指定节点的计数器值
    pub fn get(&self, node_id: &str) -> u64 {
        *self.clocks.get(node_id).unwrap_or(&0)
    }

    /// 合并另一个 Vector Clock（取最大值）
    pub fn merge(&mut self, other: &VectorClock) {
        for (node_id, &value) in &other.clocks {
            let entry = self.clocks.entry(node_id.clone()).or_insert(0);
            *entry = (*entry).max(value);
        }
    }

    /// 获取所有时钟数据
    pub fn get_clocks(&self) -> &BTreeMap<String, u64> {
        &self.clocks
    }

    /// 比较两个 Vector Clock
    ///
    /// # Returns
    /// - `Ordering::Less`: self 发生在 other 之前
    /// - `Ordering::Greater`: self 发生在 other 之后
    /// - `Ordering::Equal`: 两者相等
    /// - `Ordering::Concurrent`: 并发（无法比较）
    pub fn compare(&self, other: &VectorClock) -> ClockOrdering {
        let mut self_greater = false;
        let mut other_greater = false;

        // 收集所有节点
        let all_nodes: HashSet<_> = self.clocks.keys()
            .chain(other.clocks.keys())
            .collect();

        for node in all_nodes {
            let self_val = self.get(node);
            let other_val = other.get(node);

            if self_val > other_val {
                self_greater = true;
            } else if self_val < other_val {
                other_greater = true;
            }
        }

        match (self_greater, other_greater) {
            (false, false) => ClockOrdering::Equal,
            (true, false) => ClockOrdering::Greater,
            (false, true) => ClockOrdering::Less,
            (true, true) => ClockOrdering::Concurrent,
        }
    }

    /// 是否等于另一个 Vector Clock
    pub fn equals(&self, other: &VectorClock) -> bool {
        self.clocks == other.clocks
    }

    /// 获取所有节点
    pub fn nodes(&self) -> Vec<String> {
        self.clocks.keys().cloned().collect()
    }

    /// 计算哈希
    pub fn hash(&self) -> String {
        let data = serde_json::to_string(&self.clocks).unwrap_or_default();
        let hash = Sha256::digest(data.as_bytes());
        format!("{:x}", hash)
    }
}

impl Default for VectorClock {
    fn default() -> Self {
        Self::new()
    }
}

/// Vector Clock 比较结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockOrdering {
    /// self 发生在 other 之前
    Less,
    /// self 发生在 other 之后
    Greater,
    /// 两者相等
    Equal,
    /// 并发（无法比较）
    Concurrent,
}

/// KV 分片数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KVShard {
    /// 分片 ID
    pub shard_id: String,
    /// KV 数据
    pub data: HashMap<String, Vec<u8>>,
    /// 版本（Vector Clock）
    pub version: VectorClock,
    /// Merkle 根
    pub merkle_root: String,
    /// 同步状态
    pub sync_state: SyncState,
    /// 最后更新时间
    pub last_updated: u64,
}

impl KVShard {
    /// 创建新的 KV 分片
    pub fn new(shard_id: String, node_id: String) -> Self {
        let now = Self::current_timestamp();
        let mut version = VectorClock::new();
        version.increment(&node_id);

        KVShard {
            shard_id,
            data: HashMap::new(),
            version,
            merkle_root: Self::compute_merkle_root(&HashMap::new()),
            sync_state: SyncState::new(),
            last_updated: now,
        }
    }

    /// 获取值
    pub fn get(&self, key: &str) -> Option<&Vec<u8>> {
        self.data.get(key)
    }

    /// 设置值
    pub fn set(&mut self, key: String, value: Vec<u8>, node_id: &str) {
        self.data.insert(key, value);
        self.version.increment(node_id);
        self.merkle_root = Self::compute_merkle_root(&self.data);
        self.last_updated = Self::current_timestamp();
    }

    /// 合并另一个分片（解决冲突）
    pub fn merge(&mut self, other: &KVShard) -> MergeResult {
        match self.version.compare(&other.version) {
            ClockOrdering::Less => {
                // other 更新，覆盖
                self.data = other.data.clone();
                self.version = other.version.clone();
                self.merkle_root = other.merkle_root.clone();
                self.last_updated = other.last_updated;
                MergeResult::Overwritten
            }
            ClockOrdering::Greater => {
                // self 更新，保持不变
                MergeResult::Unchanged
            }
            ClockOrdering::Equal => {
                // 相同，无需合并
                MergeResult::Unchanged
            }
            ClockOrdering::Concurrent => {
                // 并发冲突，需要解决
                self.resolve_conflict(other)
            }
        }
    }

    /// 解决并发冲突
    fn resolve_conflict(&mut self, other: &KVShard) -> MergeResult {
        // 简单策略：使用 Merkle 根较大的（确定性选择）
        if self.merkle_root > other.merkle_root {
            // 保持 self
            MergeResult::Unchanged
        } else if self.merkle_root < other.merkle_root {
            // 使用 other
            self.data = other.data.clone();
            self.version.merge(&other.version);
            self.merkle_root = other.merkle_root.clone();
            self.last_updated = other.last_updated;
            MergeResult::Overwritten
        } else {
            // Merkle 根相同，数据应该一致
            MergeResult::Unchanged
        }
    }

    /// 计算 Merkle 根
    fn compute_merkle_root(data: &HashMap<String, Vec<u8>>) -> String {
        if data.is_empty() {
            return Self::empty_hash();
        }

        // 排序后计算哈希
        let mut hashes: Vec<String> = data.iter()
            .map(|(k, v)| {
                let combined = format!("{}:{}", k, hex::encode(v));
                let hash = Sha256::digest(combined.as_bytes());
                format!("{:x}", hash)
            })
            .collect();

        hashes.sort();

        // 构建 Merkle Tree
        while hashes.len() > 1 {
            let mut new_hashes = Vec::new();
            for chunk in hashes.chunks(2) {
                let combined = match chunk.len() {
                    2 => format!("{}{}", chunk[0], chunk[1]),
                    1 => format!("{}{}", chunk[0], chunk[0]),
                    _ => unreachable!(),
                };
                let hash = Sha256::digest(combined.as_bytes());
                new_hashes.push(format!("{:x}", hash));
            }
            hashes = new_hashes;
        }

        hashes.into_iter().next().unwrap_or_else(Self::empty_hash)
    }

    fn empty_hash() -> String {
        "0000000000000000000000000000000000000000000000000000000000000000".to_string()
    }

    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    /// 验证数据完整性
    pub fn verify_integrity(&self) -> bool {
        let computed = Self::compute_merkle_root(&self.data);
        computed == self.merkle_root
    }
}

/// 合并结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeResult {
    /// 被覆盖
    Overwritten,
    /// 保持不变
    Unchanged,
    /// 部分合并
    PartiallyMerged,
}

/// 同步状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    /// 已知副本列表
    pub replicas: Vec<ReplicaInfo>,
    /// 每个副本的同步进度（Vector Clock）
    pub replica_versions: HashMap<String, VectorClock>,
    /// 待同步的节点
    pub pending_sync: HashSet<String>,
    /// 最后同步时间
    pub last_sync_time: Option<u64>,
    /// 同步失败次数
    pub sync_failures: u32,
}

impl SyncState {
    /// 创建新的同步状态
    pub fn new() -> Self {
        SyncState {
            replicas: Vec::new(),
            replica_versions: HashMap::new(),
            pending_sync: HashSet::new(),
            last_sync_time: None,
            sync_failures: 0,
        }
    }

    /// 添加副本
    pub fn add_replica(&mut self, replica: ReplicaInfo) {
        self.replicas.push(replica);
    }

    /// 更新副本版本
    pub fn update_replica_version(&mut self, node_id: String, version: VectorClock) {
        self.replica_versions.insert(node_id, version);
    }

    /// 标记待同步
    pub fn mark_pending_sync(&mut self, node_id: String) {
        self.pending_sync.insert(node_id);
    }

    /// 标记同步完成
    pub fn mark_sync_complete(&mut self, node_id: &str) {
        self.pending_sync.remove(node_id);
        self.last_sync_time = Some(SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs());
        self.sync_failures = 0;
    }

    /// 标记同步失败
    pub fn mark_sync_failure(&mut self) {
        self.sync_failures += 1;
    }

    /// 获取待同步节点列表
    pub fn get_pending_sync(&self) -> Vec<String> {
        self.pending_sync.iter().cloned().collect()
    }

    /// 是否需要同步
    pub fn needs_sync(&self) -> bool {
        !self.pending_sync.is_empty()
    }
}

impl Default for SyncState {
    fn default() -> Self {
        Self::new()
    }
}

/// 副本信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicaInfo {
    /// 节点 ID
    pub node_id: String,
    /// 节点地址
    pub address: String,
    /// 是否在线
    pub is_online: bool,
    /// 最后心跳时间
    pub last_heartbeat: u64,
}

impl ReplicaInfo {
    /// 创建新的副本信息
    pub fn new(node_id: String, address: String) -> Self {
        ReplicaInfo {
            node_id,
            address,
            is_online: true,
            last_heartbeat: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// 更新心跳
    pub fn heartbeat(&mut self) {
        self.last_heartbeat = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.is_online = true;
    }

    /// 检查是否超时
    pub fn is_timeout(&self, timeout_secs: u64) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now - self.last_heartbeat > timeout_secs
    }
}

/// Gossip 协议配置
#[derive(Debug, Clone)]
pub struct GossipConfig {
    /// 节点 ID
    pub node_id: String,
    /// 节点地址
    pub address: String,
    /// Gossip 间隔（毫秒）
    pub gossip_interval_ms: u64,
    /// 每次 Gossip 的 peer 数量
    pub fanout: usize,
    /// 超时时间（秒）
    pub timeout_secs: u64,
}

impl Default for GossipConfig {
    fn default() -> Self {
        GossipConfig {
            node_id: "node_1".to_string(),
            address: "localhost:8080".to_string(),
            gossip_interval_ms: 1000,
            fanout: 2,
            timeout_secs: 30,
        }
    }
}

/// Gossip 消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipMessage {
    /// 分片 ID
    pub shard_id: String,
    /// KV 分片数据
    pub shard: KVShard,
    /// Vector Clock
    pub vector_clock: VectorClock,
    /// Merkle 根
    pub merkle_root: String,
    /// 时间戳
    pub timestamp: u64,
}

/// Gossip 协议实现
pub struct GossipProtocol<Net: GossipNetwork = MemoryGossipNetwork> {
    /// 配置
    config: GossipConfig,
    /// 本地分片
    shards: HashMap<String, KVShard>,
    /// 已知节点列表
    peers: Vec<ReplicaInfo>,
    /// 同步状态
    sync_states: HashMap<String, SyncState>,
    /// 网络层（可选）
    network: Option<Net>,
}

impl<Net: GossipNetwork> GossipProtocol<Net> {
    /// 创建新的 Gossip 协议（不带网络层）
    pub fn new(config: GossipConfig) -> Self {
        GossipProtocol {
            config,
            shards: HashMap::new(),
            peers: Vec::new(),
            sync_states: HashMap::new(),
            network: None,
        }
    }
    
    /// 创建带网络层的 Gossip 协议
    pub fn with_network(config: GossipConfig, network: Net) -> Self {
        GossipProtocol {
            config,
            shards: HashMap::new(),
            peers: Vec::new(),
            sync_states: HashMap::new(),
            network: Some(network),
        }
    }

    /// 添加 KV 分片
    pub fn add_shard(&mut self, shard: KVShard) {
        let shard_id = shard.shard_id.clone();
        self.shards.insert(shard_id, shard);
    }

    /// 获取分片
    pub fn get_shard(&self, shard_id: &str) -> Option<&KVShard> {
        self.shards.get(shard_id)
    }

    /// 获取分片（可变引用）
    pub fn get_shard_mut(&mut self, shard_id: &str) -> Option<&mut KVShard> {
        self.shards.get_mut(shard_id)
    }

    /// 添加 peer 节点
    pub fn add_peer(&mut self, peer: ReplicaInfo) {
        self.peers.push(peer);
    }

    /// 移除 peer 节点
    pub fn remove_peer(&mut self, node_id: &str) {
        self.peers.retain(|p| p.node_id != node_id);
    }

    /// 选择随机 peer 节点
    pub fn select_random_peers(&self) -> Vec<ReplicaInfo> {
        // 过滤在线节点
        let online_peers: Vec<_> = self.peers.iter()
            .filter(|p| p.is_online && !p.is_timeout(self.config.timeout_secs))
            .collect();

        // 随机选择 fanout 个节点（简单实现：取前 n 个）
        let n = self.config.fanout.min(online_peers.len());
        online_peers.into_iter().take(n).cloned().collect()
    }

    /// 同步分片到 peer 节点
    pub async fn sync_shard(&mut self, shard_id: &str) -> Result<SyncResult, String> {
        let shard = self.shards.get(shard_id)
            .ok_or_else(|| format!("Shard {} not found", shard_id))?
            .clone();

        let peers = self.select_random_peers();

        if peers.is_empty() {
            return Ok(SyncResult {
                synced_count: 0,
                failed_count: 0,
                message: "No available peers".to_string(),
            });
        }

        let mut synced = 0;
        let mut failed = 0;

        for peer in peers {
            match self.sync_to_peer(&shard, &peer).await {
                Ok(_) => synced += 1,
                Err(e) => {
                    warn!("Failed to sync to peer {}: {}", peer.node_id, e);
                    failed += 1;
                }
            }
        }

        Ok(SyncResult {
            synced_count: synced,
            failed_count: failed,
            message: format!("Synced to {} peers, {} failed", synced, failed),
        })
    }

    /// 同步到单个 peer
    async fn sync_to_peer(&self, shard: &KVShard, peer: &ReplicaInfo) -> Result<(), String> {
        // 如果有网络层，使用真实网络发送
        if let Some(network) = &self.network {
            let gossip_msg = GossipMessage {
                shard_id: shard.shard_id.clone(),
                shard: shard.clone(),
                vector_clock: shard.version.clone(),
                merkle_root: shard.merkle_root.clone(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            };
            
            network.gossip(gossip_msg).await?;
            debug!("Synced shard {} to peer {} via network", shard.shard_id, peer.node_id);
        } else {
            // 实际实现中，这里会通过网络发送数据
            // 这里仅模拟同步过程
            debug!("Syncing shard {} to peer {} (memory simulation)", shard.shard_id, peer.node_id);

            // 模拟网络延迟
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        Ok(())
    }

    /// 接收来自 peer 的同步请求
    pub fn receive_sync(&mut self, shard: KVShard, from_node: &str) -> Result<SyncResponse, String> {
        let shard_id = shard.shard_id.clone();

        // 获取或创建本地分片
        let local_shard = self.shards.entry(shard_id.clone())
            .or_insert_with(|| KVShard::new(shard_id.clone(), self.config.node_id.clone()));

        // 合并数据
        let merge_result = local_shard.merge(&shard);

        // 更新同步状态
        if let Some(state) = self.sync_states.get_mut(&shard_id) {
            state.update_replica_version(from_node.to_string(), shard.version.clone());
            state.mark_sync_complete(from_node);
        }

        Ok(SyncResponse {
            success: true,
            merge_result,
            local_version: local_shard.version.clone(),
        })
    }

    /// 获取同步统计信息
    pub fn stats(&self) -> GossipStats {
        let total_shards = self.shards.len();
        let total_peers = self.peers.len();
        let online_peers = self.peers.iter().filter(|p| p.is_online).count();

        GossipStats {
            node_id: self.config.node_id.clone(),
            total_shards,
            total_peers,
            online_peers,
            sync_states_count: self.sync_states.len(),
        }
    }

    /// 运行 Gossip 循环
    pub async fn run(&mut self) -> Result<(), String> {
        let interval_ms = self.config.gossip_interval_ms;
        let mut ticker = interval(Duration::from_millis(interval_ms));

        info!("GossipProtocol started: interval={}ms, fanout={}", interval_ms, self.config.fanout);

        loop {
            ticker.tick().await;

            // 同步所有分片
            for shard_id in self.shards.keys().cloned().collect::<Vec<_>>() {
                if let Err(e) = self.sync_shard(&shard_id).await {
                    error!("Failed to sync shard {}: {}", shard_id, e);
                }
            }
        }
    }
}

/// 同步结果
#[derive(Debug, Clone)]
pub struct SyncResult {
    /// 成功同步的节点数
    pub synced_count: usize,
    /// 失败的节点数
    pub failed_count: usize,
    /// 消息
    pub message: String,
}

/// 同步响应
#[derive(Debug, Clone)]
pub struct SyncResponse {
    /// 是否成功
    pub success: bool,
    /// 合并结果
    pub merge_result: MergeResult,
    /// 本地版本
    pub local_version: VectorClock,
}

/// Gossip 统计信息
#[derive(Debug, Clone, Default)]
pub struct GossipStats {
    /// 节点 ID
    pub node_id: String,
    /// 分片总数
    pub total_shards: usize,
    /// 节点总数
    pub total_peers: usize,
    /// 在线节点数
    pub online_peers: usize,
    /// 同步状态数
    pub sync_states_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_clock_creation() {
        let vc = VectorClock::new();
        assert!(vc.clocks.is_empty());

        let vc = VectorClock::with_node("node_1".to_string());
        assert_eq!(vc.get("node_1"), 1);
    }

    #[test]
    fn test_vector_clock_increment() {
        let mut vc = VectorClock::new();
        vc.increment("node_1");
        vc.increment("node_1");
        vc.increment("node_2");

        assert_eq!(vc.get("node_1"), 2);
        assert_eq!(vc.get("node_2"), 1);
        assert_eq!(vc.get("node_3"), 0);
    }

    #[test]
    fn test_vector_clock_merge() {
        let mut vc1 = VectorClock::new();
        vc1.increment("node_1");
        vc1.increment("node_2");

        let mut vc2 = VectorClock::new();
        vc2.increment("node_2");
        vc2.increment("node_2");
        vc2.increment("node_3");

        vc1.merge(&vc2);

        assert_eq!(vc1.get("node_1"), 1);
        assert_eq!(vc1.get("node_2"), 2);
        assert_eq!(vc1.get("node_3"), 1);
    }

    #[test]
    fn test_vector_clock_compare() {
        let mut vc1 = VectorClock::new();
        vc1.increment("node_1");

        let mut vc2 = VectorClock::new();
        vc2.increment("node_1");
        vc2.increment("node_1");

        assert_eq!(vc1.compare(&vc2), ClockOrdering::Less);
        assert_eq!(vc2.compare(&vc1), ClockOrdering::Greater);

        let vc3 = vc1.clone();
        assert_eq!(vc1.compare(&vc3), ClockOrdering::Equal);

        // 并发情况
        let mut vc4 = VectorClock::new();
        vc4.increment("node_2");

        assert_eq!(vc1.compare(&vc4), ClockOrdering::Concurrent);
    }

    #[test]
    fn test_kv_shard_creation() {
        let shard = KVShard::new("shard_1".to_string(), "node_1".to_string());

        assert_eq!(shard.shard_id, "shard_1");
        assert!(shard.data.is_empty());
        assert_eq!(shard.version.get("node_1"), 1);
    }

    #[test]
    fn test_kv_shard_set_get() {
        let mut shard = KVShard::new("shard_1".to_string(), "node_1".to_string());

        shard.set("key_1".to_string(), b"value_1".to_vec(), "node_1");

        assert_eq!(shard.get("key_1"), Some(&b"value_1".to_vec()));
        assert_eq!(shard.version.get("node_1"), 2);
    }

    #[test]
    fn test_kv_shard_merge() {
        let mut shard1 = KVShard::new("shard_1".to_string(), "node_1".to_string());
        shard1.set("key_1".to_string(), b"value_1".to_vec(), "node_1");

        let mut shard2 = KVShard::new("shard_1".to_string(), "node_2".to_string());
        shard2.set("key_2".to_string(), b"value_2".to_vec(), "node_2");

        // 让 shard2 的版本明确大于 shard1
        // shard1 的 version: {node_1: 2}
        // shard2 的 version: {node_2: 1}
        // 这是并发的，需要手动设置 shard2 的版本使其更大
        shard2.version = shard1.version.clone();
        shard2.version.increment("node_2");
        // 现在 shard2 的 version: {node_1: 2, node_2: 2}，明确大于 shard1

        let result = shard1.merge(&shard2);
        assert_eq!(result, MergeResult::Overwritten);
        assert_eq!(shard1.get("key_2"), Some(&b"value_2".to_vec()));
    }

    #[test]
    fn test_gossip_protocol_creation() {
        let config = GossipConfig::default();
        let gossip = GossipProtocol::new(config);

        assert_eq!(gossip.stats().total_peers, 0);
        assert_eq!(gossip.stats().total_shards, 0);
    }

    #[test]
    fn test_gossip_peer_selection() {
        let config = GossipConfig {
            fanout: 2,
            ..Default::default()
        };
        let mut gossip = GossipProtocol::new(config);

        // 添加 5 个 peer
        for i in 0..5 {
            gossip.add_peer(ReplicaInfo::new(
                format!("node_{}", i),
                format!("localhost:{}", 8080 + i),
            ));
        }

        let peers = gossip.select_random_peers();
        assert!(peers.len() <= 2);
    }

    #[test]
    fn test_shard_integrity_verification() {
        let mut shard = KVShard::new("shard_1".to_string(), "node_1".to_string());
        
        shard.set("key_1".to_string(), b"value_1".to_vec(), "node_1");
        shard.set("key_2".to_string(), b"value_2".to_vec(), "node_1");

        assert!(shard.verify_integrity());

        // 篡改数据
        shard.data.insert("key_3".to_string(), b"value_3".to_vec());

        assert!(!shard.verify_integrity());
    }
}
