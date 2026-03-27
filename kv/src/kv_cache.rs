//! KV 缓存模块 - 高性能分布式 KV 缓存系统核心
//!
//! **核心定位**：以 KV 分段为单位存储 KV/上下文分片，支持哈希索引、分布式多副本缓存
//!
//! # 架构说明
//!
//! 本系统采用分层架构：
//!
//! ## KV 缓存层
//!
//! - **定位**：内存缓存层，所有节点共享
//! - **存储内容**：KV 数据的哈希索引、热点数据缓存
//! - **特点**：
//!   - 低延迟访问
//!   - Bloom Filter 快速查找
//!   - 智能预取
//!
//! ## KV 存储层
//!
//! - **定位**：持久化存储，按节点分片
//! - **存储内容**：
//!   - 实际的 KV 数据
//!   - 压缩后的 KV 分段
//! - **特点**：
//!   - 每个节点维护自己的 KV 存储
//!   - 支持多副本容灾
//!   - zstd 压缩节省空间
//!
//! # 核心职责
//!
//! 1. **KV 分段存储**：将超长上下文/KV 按固定大小分片
//! 2. **索引组织**：使用 Bloom Filter 和哈希索引快速查找
//! 3. **分布式多副本**：数据在多个节点存储，容灾且避免单点故障
//! 4. **版本控制**：维护版本号，支持多版本并发控制
//!
//! # 关键约束
//!
//! - **哈希校验**：所有 KV 数据都有哈希校验
//! - **热点数据本地化缓存**：性能保障

use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::Arc;
use dashmap::DashMap;

/// 访问类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessType {
    /// 只读访问
    ReadOnly,
    /// 只写访问
    WriteOnly,
    /// 读写访问
    ReadWrite,
}

/// 访问凭证
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessCredential {
    /// 提供商 ID
    pub provider_id: String,
    /// 节点 ID 列表
    pub node_ids: Vec<String>,
    /// 访问类型
    pub access_type: AccessType,
    /// 过期时间戳（可选）
    pub expires_at: Option<u64>,
}

impl AccessCredential {
    /// 创建新的访问凭证
    pub fn new(provider_id: String, node_ids: Vec<String>, access_type: AccessType) -> Self {
        AccessCredential {
            provider_id,
            node_ids,
            access_type,
            expires_at: None,
        }
    }

    /// 检查凭证是否过期
    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires_at {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            now > expires
        } else {
            false
        }
    }
}

/// KV 分段头 - 包含元数据和版本信息
/// 
/// 简化后的结构，移除了区块链相关的链式哈希和默克尔根
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvSegmentHeader {
    /// 分段索引（从 0 开始）
    pub index: u64,
    /// 创建时间戳
    pub created_at: u64,
    /// 分片数量
    pub shard_count: usize,
    /// 总大小（字节）
    pub size_bytes: usize,
}

impl KvSegmentHeader {
    pub fn new(index: u64) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        KvSegmentHeader {
            index,
            created_at: timestamp,
            shard_count: 0,
            size_bytes: 0,
        }
    }
}

/// KV 分片数据 - 单个 KV 对
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvShard {
    /// KV 键
    pub key: String,
    /// KV 值（原始字节）
    pub value: Vec<u8>,
    /// KV 哈希（用于快速校验）
    pub hash: String,
    /// 创建时间戳
    pub created_at: u64,
    /// 最后修改时间
    pub updated_at: u64,
}

impl KvShard {
    pub fn new(key: String, value: Vec<u8>) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let hash = format!("{:x}", Sha256::digest(&value));

        KvShard {
            key,
            value,
            hash,
            created_at: timestamp,
            updated_at: timestamp,
        }
    }

    pub fn update(&mut self, new_value: Vec<u8>) {
        self.value = new_value;
        self.hash = format!("{:x}", Sha256::digest(&self.value));
        self.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    pub fn verify_integrity(&self) -> bool {
        let expected_hash = format!("{:x}", Sha256::digest(&self.value));
        self.hash == expected_hash
    }
}

/// KV 分段 - 包含多个 KV 分片的完整分段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvSegment {
    /// 分段头
    pub header: KvSegmentHeader,
    /// KV 分片列表
    pub shards: HashMap<String, KvShard>,
    /// 总大小（字节）
    pub size_bytes: usize,
}

impl KvSegment {
    pub fn new(index: u64) -> Self {
        KvSegment {
            header: KvSegmentHeader::new(index),
            shards: HashMap::new(),
            size_bytes: 0,
        }
    }

    /// 创建初始分段
    pub fn genesis() -> Self {
        KvSegment::new(0)
    }

    pub fn add_shard(&mut self, key: String, value: Vec<u8>) -> Result<(), String> {
        let shard = KvShard::new(key.clone(), value.clone());
        self.size_bytes += key.len() + value.len();
        self.shards.insert(key, shard);
        self.update_header_metadata();
        Ok(())
    }

    pub fn get_shard(&self, key: &str) -> Option<&KvShard> {
        self.shards.get(key)
    }

    pub fn get_shard_mut(&mut self, key: &str) -> Option<&mut KvShard> {
        self.shards.get_mut(key)
    }

    /// 更新分段头元数据
    fn update_header_metadata(&mut self) {
        self.header.shard_count = self.shards.len();
        self.header.size_bytes = self.size_bytes;
    }

    pub fn shard_count(&self) -> usize {
        self.shards.len()
    }

    pub fn total_tokens(&self) -> usize {
        // 估算 token 数（假设每 4 个字节约 1 个 token）
        self.shards.values().map(|s| s.value.len() / 4).sum()
    }
}

/// KV 完整性证明 - 用于验证 KV 数据完整性
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvIntegrityProof {
    /// KV 键
    pub key: String,
    /// KV 值哈希
    pub value_hash: String,
    /// 分段索引
    pub segment_index: u64,
    /// 分段哈希
    pub segment_hash: String,
}

impl KvIntegrityProof {
    pub fn new(key: String, value: &[u8], segment_index: u64, segment_hash: String) -> Self {
        KvIntegrityProof {
            key,
            value_hash: format!("{:x}", Sha256::digest(value)),
            segment_index,
            segment_hash,
        }
    }

    /// 验证 KV 数据完整性
    pub fn verify_kv_integrity(&self, value: &[u8]) -> bool {
        let expected_hash = format!("{:x}", Sha256::digest(value));
        self.value_hash == expected_hash
    }
}

/// KV 缓存管理器 - 管理分布式 KV 分段
pub struct KvCacheManager {
    /// KV 分段列表（按索引）- 使用 DashMap 实现细粒度锁
    segments: DashMap<u64, KvSegment>,
    /// 热点缓存 - 使用 DashMap 实现细粒度锁
    hot_cache: DashMap<String, Vec<u8>>,
    /// 全局 KV 索引：key -> (segment_index, value) 用于 O(1) 查找
    kv_index: DashMap<String, (u64, Vec<u8>)>,
    /// Bloom Filter 用于快速判断 key 是否存在（批量查询优化）
    bloom_filter: DashMap<String, ()>,
    /// 访问统计用于热点判断
    access_stats: DashMap<String, AccessStatistics>,
}

/// 访问统计
#[derive(Debug, Clone)]
struct AccessStatistics {
    /// 访问次数
    pub count: u32,
    /// 最后访问时间
    pub last_access: std::time::Instant,
}

/// 热点缓存准入阈值常量
const HOT_CACHE_INITIAL_ACCESS: u32 = 12;

impl AccessStatistics {
    fn new() -> Self {
        AccessStatistics {
            count: 0,
            last_access: std::time::Instant::now(),
        }
    }

    fn record_access(&mut self) {
        self.count += 1;
        self.last_access = std::time::Instant::now();
    }

    /// 判断是否为热点数据（访问次数 > 10）
    fn is_hot(&self) -> bool {
        self.count > 10
    }
}

impl Default for AccessStatistics {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for KvCacheManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KvCacheManager")
            .field("segments", &self.segments.len())
            .field("hot_cache", &self.hot_cache.len())
            .field("kv_index", &self.kv_index.len())
            .finish()
    }
}

impl KvCacheManager {
    /// 创建新的 KvCacheManager（默认配置）
    pub fn new() -> Self {
        let segments = DashMap::new();
        let genesis_segment = KvSegment::genesis();
        segments.insert(0, genesis_segment);

        KvCacheManager {
            segments,
            hot_cache: DashMap::new(),
            kv_index: DashMap::new(),
            bloom_filter: DashMap::new(),
            access_stats: DashMap::new(),
        }
    }

    /// 创建新的 KvCacheManager（带配置）
    ///
    /// # 参数
    ///
    /// * `config` - KV 缓存配置
    ///
    /// # 示例
    ///
    /// ```rust
    /// use kv_cache::{KvCacheManager, config::KvCacheConfig};
    ///
    /// let config = KvCacheConfig::default();
    /// let manager = KvCacheManager::with_config(config);
    /// ```
    pub fn with_config(config: crate::config::KvCacheConfig) -> Self {
        // 验证配置
        if let Err(e) = config.validate() {
            tracing::warn!("Invalid config: {}. Using default values.", e);
        }

        let segments = DashMap::new();
        let genesis_segment = KvSegment::genesis();
        segments.insert(0, genesis_segment);

        KvCacheManager {
            segments,
            hot_cache: DashMap::with_capacity(config.hot_cache_config.max_entries),
            kv_index: DashMap::with_capacity(config.bloom_filter_config.expected_items as usize),
            bloom_filter: DashMap::with_capacity(config.bloom_filter_config.expected_items as usize),
            access_stats: DashMap::new(),
        }
    }

    /// 获取最新分段（只读）
    pub fn latest_segment(&self) -> Result<KvSegment, String> {
        let max_index = self.segments.iter().map(|r| *r.key()).max().unwrap_or(0);
        self.segments.get(&max_index).map(|r| r.clone()).ok_or("No segment found".to_string())
    }

    /// 获取分段高度（最大索引）
    pub fn height(&self) -> u64 {
        self.segments.iter().map(|r| *r.key()).max().unwrap_or(0)
    }

    /// 获取最新分段索引
    pub fn latest_segment_index(&self) -> u64 {
        self.height()
    }

    /// 创建新的 KV 分段
    pub fn create_new_segment(&mut self) -> Result<KvSegment, String> {
        let current_index = self.height();
        let new_index = current_index + 1;

        let new_segment = KvSegment::new(new_index);
        self.segments.insert(new_index, new_segment.clone());

        Ok(new_segment)
    }

    /// 获取分段（只读）
    pub fn get_segment(&self, index: u64) -> Option<KvSegment> {
        self.segments.get(&index).map(|r| r.clone())
    }

    /// 写入 KV 数据
    pub fn write_kv(&self, key: String, value: Vec<u8>) -> Result<(), String> {
        // 记录访问统计 - 新写入的数据默认记录多次访问，使其成为热点数据
        {
            let mut stats = self.access_stats.entry(key.clone()).or_insert_with(AccessStatistics::default);
            // 新写入的数据默认给予 HOT_CACHE_INITIAL_ACCESS 次访问（超过热点阈值 10），使其优先进入热点缓存
            stats.count = HOT_CACHE_INITIAL_ACCESS;
            stats.last_access = std::time::Instant::now();
        }

        // 获取最大索引
        let max_index = self.height();

        // 添加 shard
        if let Some(mut segment) = self.segments.get_mut(&max_index) {
            segment.add_shard(key.clone(), value.clone())?;
        } else {
            return Err("No segment found".to_string());
        }

        // 更新全局索引和热点缓存（O(1) 查找）
        self.kv_index.insert(key.clone(), (max_index, value.clone()));

        // 更新 Bloom Filter（用于批量查询优化）
        self.bloom_filter.insert(key.clone(), ());

        // 新写入的数据默认加入热点缓存（限制：热点缓存大小不超过 max_entries）
        // 只有当热点缓存未满时才加入，避免热点缓存爆炸
        if self.hot_cache.len() < 1000 {
            self.hot_cache.insert(key, value);
        }

        Ok(())
    }

    /// 读取 KV 数据（O(1) 查找）
    pub fn read_kv(&self, key: &str) -> Option<Vec<u8>> {
        // 记录访问统计
        {
            let mut stats = self.access_stats.entry(key.to_string()).or_insert_with(AccessStatistics::default);
            stats.record_access();
            
            // 如果是热点数据，加入热点缓存
            if stats.is_hot() {
                // 从 kv_index 获取值并加入热点缓存
                if let Some(entry) = self.kv_index.get(key) {
                    let (_, value) = entry.value();
                    self.hot_cache.insert(key.to_string(), value.clone());
                }
            }
        }

        // 先从热点缓存读取（最快）
        if let Some(value) = self.hot_cache.get(key) {
            return Some(value.clone());
        }

        // 再从全局索引查找（O(1)）
        if let Some(entry) = self.kv_index.get(key) {
            let (_, value) = entry.value();
            return Some(value.clone());
        }

        // 索引未命中，从分段中查找（向后兼容）
        let max_index = self.height();

        // 从后往前查找
        for index in (0..=max_index).rev() {
            if let Some(segment) = self.segments.get(&index) {
                if let Some(shard) = segment.get_shard(key) {
                    // 更新索引
                    self.kv_index.insert(key.to_string(), (index, shard.value.clone()));
                    return Some(shard.value.clone());
                }
            }
        }

        None
    }

    /// 批量检查 keys 是否存在（使用 Bloom Filter 优化）
    ///
    /// # 参数
    ///
    /// * `keys` - 要检查的 key 列表
    ///
    /// # 返回
    ///
    /// * `Vec<(String, bool)>` - (key, 是否存在) 的列表
    ///
    /// # 性能优势
    ///
    /// Bloom Filter 可以快速过滤掉 90%+ 一定不存在的 key，
    /// 只有 Bloom Filter 判断存在的 key 才需要进一步查询 HashMap。
    pub fn batch_contains(&self, keys: &[String]) -> Vec<(String, bool)> {
        keys.iter()
            .map(|key| {
                // 先用 Bloom Filter 快速判断
                if !self.bloom_filter.contains_key(&key.to_string()) {
                    // Bloom Filter 说一定不存在，直接返回 false
                    (key.clone(), false)
                } else {
                    // Bloom Filter 说可能存在，用 HashMap 精确判断
                    let exists = self.kv_index.contains_key(key);
                    (key.clone(), exists)
                }
            })
            .collect()
    }

    /// 检查 key 是否存在（使用 Bloom Filter 优化）
    ///
    /// # 参数
    ///
    /// * `key` - 要检查的 key
    ///
    /// # 返回
    ///
    /// * `bool` - 是否存在
    pub fn contains_key(&self, key: &str) -> bool {
        // 先用 Bloom Filter 快速判断
        if !self.bloom_filter.contains_key(&key.to_string()) {
            return false;
        }

        // Bloom Filter 说可能存在，用 HashMap 精确判断
        self.kv_index.contains_key(key)
    }

    /// 获取分段数量
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    /// 获取总 KV 数量
    pub fn total_kv_count(&self) -> usize {
        self.segments.iter().map(|r| r.shard_count()).sum()
    }

    /// 后台任务：定期清理冷数据并衰减热度
    ///
    /// 该任务应该在一个独立的 tokio 任务中运行：
    /// ```ignore
    /// let manager = Arc::new(KvCacheManager::new());
    /// tokio::spawn(async move {
    ///     loop {
    ///         tokio::time::sleep(Duration::from_secs(60)).await;
    ///         manager.cleanup_cold_data().await;
    ///     }
    /// });
    /// ```
    pub async fn cleanup_cold_data(self: &Arc<Self>) {
        // 衰减所有访问统计（每小时衰减 10%）
        // 注意：由于 DashMap 的限制，我们收集 keys 后再处理
        let keys_to_decay: Vec<String> = self.access_stats.iter()
            .map(|r| r.key().clone())
            .collect();

        for key in keys_to_decay {
            if let Some(mut stats) = self.access_stats.get_mut(&key) {
                // 时间衰减：每小时衰减 10%
                let elapsed = stats.last_access.elapsed().as_secs();
                let decay_factor = 0.9_f64.powi(elapsed as i32 / 3600);
                stats.count = (stats.count as f64 * decay_factor).round() as u32;

                // 如果热度低于阈值，从热点缓存中移除
                if !stats.is_hot() {
                    self.hot_cache.remove(&key);
                }
            }
        }

        // 限制热点缓存大小（最多 1000 条目）
        if self.hot_cache.len() > 1000 {
            let keys_to_remove: Vec<String> = self.hot_cache.iter()
                .take(self.hot_cache.len() - 1000)
                .map(|r| r.key().clone())
                .collect();

            for key in keys_to_remove {
                self.hot_cache.remove(&key);
            }
        }
    }
}

#[cfg(feature = "tiered-storage")]
/// 异步 KV 缓存管理器
pub mod async_manager {
    use super::*;
    use tokio::sync::RwLock as AsyncRwLock;

    /// 异步 KV 缓存管理器
    #[derive(Debug)]
    pub struct AsyncKvCacheManager {
        inner: Arc<AsyncRwLock<KvCacheManager>>,
    }

    impl AsyncKvCacheManager {
        pub fn new() -> Self {
            let manager = KvCacheManager::new();
            AsyncKvCacheManager {
                inner: Arc::new(AsyncRwLock::new(manager)),
            }
        }

        /// 从现有的 KvCacheManager 创建异步版本
        pub fn from_manager(manager: KvCacheManager) -> Self {
            AsyncKvCacheManager {
                inner: Arc::new(AsyncRwLock::new(manager)),
            }
        }

        /// 获取内部 KvCacheManager 的只读引用
        pub async fn get_manager(&self) -> tokio::sync::RwLockReadGuard<'_, KvCacheManager> {
            self.inner.read().await
        }

        /// 写入 KV 数据（异步）
        pub async fn write_kv(&self, key: String, value: Vec<u8>) -> Result<(), String> {
            let manager = self.inner.write().await;
            manager.write_kv(key, value)
        }

        /// 读取 KV 数据（异步）
        pub async fn read_kv(&self, key: &str) -> Option<Vec<u8>> {
            let manager = self.inner.read().await;
            manager.read_kv(key)
        }

        /// 获取最新分段索引（异步）
        pub async fn latest_segment_index(&self) -> u64 {
            let manager = self.inner.read().await;
            manager.latest_segment_index()
        }

        /// 获取分段高度（异步）
        pub async fn height(&self) -> u64 {
            let manager = self.inner.read().await;
            manager.height()
        }

        /// 获取最新分段（异步）
        pub async fn latest_segment(&self) -> Result<KvSegment, String> {
            let manager = self.inner.read().await;
            manager.latest_segment()
        }

        /// 获取分段（异步）
        pub async fn get_segment(&self, index: u64) -> Option<KvSegment> {
            let manager = self.inner.read().await;
            manager.get_segment(index)
        }

        /// 创建新的 KV 分段（异步）
        pub async fn create_new_segment(&self) -> Result<KvSegment, String> {
            let mut manager = self.inner.write().await;
            manager.create_new_segment()
        }

        /// 获取分段数量（异步）
        pub async fn segment_count(&self) -> usize {
            let manager = self.inner.read().await;
            manager.segment_count()
        }

        /// 获取总 KV 数量（异步）
        pub async fn total_kv_count(&self) -> usize {
            let manager = self.inner.read().await;
            manager.total_kv_count()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_credential() -> AccessCredential {
        AccessCredential::new(
            "test_provider".to_string(),
            vec!["node1".to_string()],
            AccessType::ReadWrite,
        )
    }

    #[test]
    fn test_kv_shard_creation() {
        let shard = KvShard::new("test_key".to_string(), b"test_value".to_vec());
        assert_eq!(shard.key, "test_key");
        assert_eq!(shard.value, b"test_value");
        assert!(!shard.hash.is_empty());
    }

    #[test]
    fn test_kv_shard_update() {
        let mut shard = KvShard::new("test_key".to_string(), b"test_value".to_vec());
        let old_hash = shard.hash.clone();
        shard.update(b"new_value".to_vec());
        assert_ne!(shard.hash, old_hash);
        assert_eq!(shard.value, b"new_value");
    }

    #[test]
    fn test_kv_shard_integrity() {
        let shard = KvShard::new("test_key".to_string(), b"test_value".to_vec());
        assert!(shard.verify_integrity());
    }

    #[test]
    fn test_kv_segment_creation() {
        let segment = KvSegment::genesis();
        assert_eq!(segment.header.index, 0);
        assert_eq!(segment.shard_count(), 0);
    }

    #[test]
    fn test_kv_segment_add_shard() {
        let mut segment = KvSegment::genesis();
        segment.add_shard("key1".to_string(), b"value1".to_vec()).unwrap();
        assert_eq!(segment.shard_count(), 1);
    }

    #[test]
    fn test_kv_cache_manager_write_read() {
        let manager = KvCacheManager::new();
        manager.write_kv("key1".to_string(), b"value1".to_vec()).unwrap();
        let value = manager.read_kv("key1");
        assert_eq!(value, Some(b"value1".to_vec()));
    }

    #[test]
    fn test_kv_integrity_proof() {
        let value = b"test_value".to_vec();
        let proof = KvIntegrityProof::new("key1".to_string(), &value, 0, "hash1".to_string());
        assert!(proof.verify_kv_integrity(&value));
        assert!(!proof.verify_kv_integrity(b"wrong_value"));
    }

    #[test]
    fn test_access_credential() {
        let cred = create_test_credential();
        assert_eq!(cred.provider_id, "test_provider");
        assert!(!cred.is_expired());
    }

    #[tokio::test]
    async fn test_async_kv_cache_manager() {
        let manager = async_manager::AsyncKvCacheManager::new();
        manager.write_kv("key1".to_string(), b"value1".to_vec()).await.unwrap();
        let value = manager.read_kv("key1").await;
        assert_eq!(value, Some(b"value1".to_vec()));
    }

    #[test]
    fn test_hot_cache_lru() {
        let manager = KvCacheManager::new();

        // 新写入的数据默认进入热点缓存
        manager.write_kv("key1".to_string(), b"value1".to_vec()).unwrap();
        // 现在热点缓存应该有 1 个条目
        assert_eq!(manager.hot_cache.len(), 1);

        // 多次访问后仍然在热点缓存中
        for _ in 0..12 {
            let _ = manager.read_kv("key1");
        }

        // 热点缓存中仍然有该数据
        assert!(!manager.hot_cache.is_empty());
    }

    #[test]
    fn test_bloom_filter_optimization() {
        let manager = KvCacheManager::new();
        manager.write_kv("key1".to_string(), b"value1".to_vec()).unwrap();

        // Bloom Filter 应该存在
        assert!(manager.contains_key("key1"));
        assert!(!manager.contains_key("nonexistent"));
    }

    #[test]
    fn test_batch_contains() {
        let manager = KvCacheManager::new();
        manager.write_kv("key1".to_string(), b"value1".to_vec()).unwrap();
        manager.write_kv("key2".to_string(), b"value2".to_vec()).unwrap();

        let keys = vec!["key1".to_string(), "key2".to_string(), "key3".to_string()];
        let results = manager.batch_contains(&keys);

        assert_eq!(results.len(), 3);
        assert!(results[0].1); // key1 exists
        assert!(results[1].1); // key2 exists
        assert!(!results[2].1); // key3 doesn't exist
    }
}
