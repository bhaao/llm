//! 多级缓存管理模块 - L1 CPU + L2 Disk + L3 Remote
//!
//! **核心功能**：
//! - L1: CPU 内存缓存（LRU，热点数据，亚毫秒延迟）
//! - L2: 磁盘存储（温数据，持久化，10-50ms 延迟）
//! - L3: 远程存储（Redis/S3，冷数据，低成本）
//! - 自动升降级策略（基于访问频率、时间和成本）
//!
//! # 架构设计
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │  客户端请求                                              │
//! │         ↓                                               │
//! │  ┌─────────────────────────────────────────────────┐    │
//! │  │ L1: CPU 内存缓存 (LRU)                           │    │
//! │  │     - 容量：1000 条目                            │    │
//! │  │     - 延迟：< 1ms                               │    │
//! │  │     - 热度：访问次数 > 10                        │    │
//! │  └─────────────────────────────────────────────────┘    │
//! │         ↓ Miss                                          │
//! │  ┌─────────────────────────────────────────────────┐    │
//! │  │ L2: 磁盘存储 (SSD/HDD)                           │    │
//! │  │     - 容量：100GB+                              │    │
//! │  │     - 延迟：10-50ms                             │    │
//! │  │     - 热度：访问次数 4-10                        │    │
//! │  └─────────────────────────────────────────────────┘    │
//! │         ↓ Miss                                          │
//! │  ┌─────────────────────────────────────────────────┐    │
//! │  │ L3: 远程存储 (Redis/S3)                          │    │
//! │  │     - 容量：TB+                                 │    │
//! │  │     - 延迟：100-500ms                           │    │
//! │  │     - 热度：访问次数 < 4                         │    │
//! │  └─────────────────────────────────────────────────┘    │
//! └─────────────────────────────────────────────────────────┘
//! ```
//!
//! # 热度判断策略
//!
//! | 访问次数 | 最后访问时间 | 数据大小 | 存储层级 |
//! |---------|-------------|---------|---------|
//! | > 10    | 任意        | 任意    | L1 内存  |
//! | 4-10    | 任意        | 任意    | L2 磁盘  |
//! | < 4     | < 5 分钟     | < 1MB   | L2 磁盘  |
//! | < 4     | > 5 分钟     | 任意    | L3 远程  |
//! | 任意    | > 1 小时     | > 10MB  | L3 远程  |
//!
//! # 性能指标
//!
//! ```text
//! ┌────────────────────┬──────────┬──────────┬──────────┐
//! │ 操作               │ L1 命中  │ L2 命中  │ L3 命中  │
//! ├────────────────────┼──────────┼──────────┼──────────┤
//! │ 读取延迟           │ < 1ms    │ 10-50ms  │ 100-500ms│
//! │ 写入延迟           │ < 1ms    │ 10-50ms  │ 100-500ms│
//! │ 成本/GB            │ $0.05    │ $0.01    │ $0.001   │
//! └────────────────────┴──────────┴──────────┴──────────┘
//! ```

#![cfg(feature = "tiered-storage")]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use anyhow::{Result, Context};

/// KV 数据块（复用现有的 KvChunk 或自定义）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiLevelKvData {
    /// 键
    pub key: String,
    /// 值（字节数组）
    pub value: Vec<u8>,
    /// 数据大小（字节）
    pub size_bytes: usize,
    /// 版本号
    pub version: u64,
    /// 创建时间戳（秒）
    pub created_at: u64,
    /// 最后访问时间戳（秒）
    pub last_accessed_at: u64,
    /// 访问次数
    pub access_count: u32,
    /// 当前存储层级
    pub current_tier: StorageTier,
}

impl MultiLevelKvData {
    /// 创建新的 KV 数据
    pub fn new(key: String, value: Vec<u8>) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let size_bytes = value.len();

        MultiLevelKvData {
            key,
            value,
            size_bytes,
            version: 1,
            created_at: timestamp,
            last_accessed_at: timestamp,
            access_count: 0,
            current_tier: StorageTier::L1CpuMemory,
        }
    }

    /// 记录一次访问
    pub fn record_access(&mut self, tier: StorageTier) {
        self.access_count += 1;
        self.last_accessed_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.current_tier = tier;
    }

    /// 判断是否为热点数据（应该在 L1）
    pub fn is_hot(&self) -> bool {
        self.access_count > 10
    }

    /// 判断是否为温数据（应该在 L2）
    pub fn is_warm(&self) -> bool {
        self.access_count >= 4 && self.access_count <= 10
    }

    /// 判断是否为冷数据（应该在 L3）
    pub fn is_cold(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let elapsed_secs = now - self.last_accessed_at;
        
        self.access_count < 4 && elapsed_secs > 300 // 5 分钟未访问
    }

    /// 获取推荐的存储层级
    pub fn recommended_tier(&self) -> StorageTier {
        if self.is_hot() {
            StorageTier::L1CpuMemory
        } else if self.is_warm() {
            StorageTier::L2Disk
        } else {
            StorageTier::L3Remote
        }
    }
}

/// 存储层级枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StorageTier {
    /// L1: CPU 内存（最快，最贵）
    L1CpuMemory,
    /// L2: 磁盘（中等速度和成本）
    L2Disk,
    /// L3: 远程存储（最慢，最便宜）
    L3Remote,
}

impl std::fmt::Display for StorageTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageTier::L1CpuMemory => write!(f, "L1_CPU"),
            StorageTier::L2Disk => write!(f, "L2_DISK"),
            StorageTier::L3Remote => write!(f, "L3_REMOTE"),
        }
    }
}

/// 远程存储后端 trait
#[async_trait::async_trait]
pub trait RemoteStorageBackend: Send + Sync {
    /// 获取数据
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;
    
    /// 存储数据
    async fn put(&self, key: &str, value: Vec<u8>) -> Result<()>;
    
    /// 删除数据
    async fn delete(&self, key: &str) -> Result<()>;
    
    /// 检查是否存在
    async fn exists(&self, key: &str) -> Result<bool>;
    
    /// 获取后端类型名称
    fn backend_type(&self) -> &'static str;
}

/// 多级缓存配置
#[derive(Debug, Clone)]
pub struct MultiLevelCacheConfig {
    /// L1 CPU 缓存大小（条目数）
    pub l1_cache_size: usize,
    /// L2 磁盘存储路径
    pub l2_disk_path: PathBuf,
    /// L3 远程存储配置（可选）
    pub l3_remote_config: Option<RemoteConfig>,
    /// 数据降级时间阈值（秒）
    pub demote_time_threshold_secs: u64,
    /// 数据升级访问次数阈值
    pub promote_access_threshold: u32,
    /// 是否启用自动升降级
    pub auto_tiering_enabled: bool,
}

/// 远程存储配置
#[derive(Debug, Clone)]
pub struct RemoteConfig {
    /// 远程存储类型
    pub storage_type: RemoteStorageType,
    /// 连接地址（Redis URL 或 S3 endpoint）
    pub endpoint: String,
    /// 认证令牌（可选）
    pub auth_token: Option<String>,
    /// 桶名称（S3 专用）
    pub bucket_name: Option<String>,
    /// 连接超时（毫秒）
    pub timeout_ms: u64,
}

/// 远程存储类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RemoteStorageType {
    /// Redis
    Redis,
    /// Amazon S3
    S3,
    /// 自定义 HTTP 存储
    CustomHttp,
}

impl Default for MultiLevelCacheConfig {
    fn default() -> Self {
        MultiLevelCacheConfig {
            l1_cache_size: 1000,
            l2_disk_path: PathBuf::from("./data/kv_l2_disk"),
            l3_remote_config: None,
            demote_time_threshold_secs: 300, // 5 分钟
            promote_access_threshold: 10,
            auto_tiering_enabled: true,
        }
    }
}

/// 多级缓存管理器
pub struct MultiLevelCacheManager {
    /// L1 CPU 缓存
    l1_cache: Arc<RwLock<L1CpuCache>>,
    /// L2 磁盘存储
    l2_disk: Arc<RwLock<L2DiskStorage>>,
    /// L3 远程存储（可选）
    l3_remote: Option<Arc<dyn RemoteStorageBackend>>,
    /// 访问统计
    access_stats: Arc<RwLock<HashMap<String, AccessStatistics>>>,
    /// 配置
    config: MultiLevelCacheConfig,
    /// 指标统计
    metrics: Arc<RwLock<CacheMetrics>>,
}

/// L1 CPU 缓存（LRU 实现）
struct L1CpuCache {
    /// 缓存数据
    data: HashMap<String, MultiLevelKvData>,
    /// LRU 顺序（最近使用的在末尾）
    lru_order: Vec<String>,
    /// 最大容量
    capacity: usize,
}

impl L1CpuCache {
    fn new(capacity: usize) -> Self {
        L1CpuCache {
            data: HashMap::with_capacity(capacity.min(1024)),
            lru_order: Vec::with_capacity(capacity.min(1024)),
            capacity,
        }
    }

    #[allow(dead_code)]
    fn get(&mut self, key: &str) -> Option<&MultiLevelKvData> {
        if self.data.contains_key(key) {
            // 更新 LRU 顺序
            if let Some(pos) = self.lru_order.iter().position(|k| k == key) {
                self.lru_order.remove(pos);
                self.lru_order.push(key.to_string());
            }
            self.data.get(key)
        } else {
            None
        }
    }

    fn get_mut(&mut self, key: &str) -> Option<&mut MultiLevelKvData> {
        if self.data.contains_key(key) {
            // 更新 LRU 顺序
            if let Some(pos) = self.lru_order.iter().position(|k| k == key) {
                self.lru_order.remove(pos);
                self.lru_order.push(key.to_string());
            }
            self.data.get_mut(key)
        } else {
            None
        }
    }

    fn put(&mut self, key: String, value: MultiLevelKvData) -> Option<MultiLevelKvData> {
        // 如果已存在，更新
        if self.data.contains_key(&key) {
            if let Some(pos) = self.lru_order.iter().position(|k| k == &key) {
                self.lru_order.remove(pos);
            }
            self.lru_order.push(key.clone());
            return self.data.insert(key, value);
        }

        // 如果超出容量，淘汰 LRU 条目
        while self.lru_order.len() >= self.capacity {
            if let Some(oldest) = self.lru_order.first().cloned() {
                self.lru_order.remove(0);
                self.data.remove(&oldest);
            }
        }

        self.lru_order.push(key.clone());
        self.data.insert(key, value)
    }

    fn remove(&mut self, key: &str) -> Option<MultiLevelKvData> {
        if let Some(pos) = self.lru_order.iter().position(|k| k == key) {
            self.lru_order.remove(pos);
        }
        self.data.remove(key)
    }

    fn len(&self) -> usize {
        self.data.len()
    }

    fn contains_key(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }
}

/// L2 磁盘存储
struct L2DiskStorage {
    /// 存储路径
    base_path: PathBuf,
    /// 本地索引（加速查找）
    index: HashMap<String, PathBuf>,
}

impl L2DiskStorage {
    fn new(base_path: &Path) -> Result<Self> {
        std::fs::create_dir_all(base_path)
            .with_context(|| format!("Failed to create L2 disk directory: {}", base_path.display()))?;
        
        Ok(L2DiskStorage {
            base_path: base_path.to_path_buf(),
            index: HashMap::new(),
        })
    }

    fn key_to_path(&self, key: &str) -> PathBuf {
        // 使用 SHA256 哈希的前 8 个字符作为子目录，避免单目录文件过多
        use sha2::{Sha256, Digest};
        let hash = Sha256::digest(key.as_bytes());
        let prefix = hex::encode(&hash[..4]);
        self.base_path.join(prefix).join(format!("{}.kv", key))
    }

    async fn get(&mut self, key: &str) -> Result<Option<MultiLevelKvData>> {
        let path = self.key_to_path(key);
        
        if !path.exists() {
            return Ok(None);
        }

        // 异步读取文件
        let bytes = tokio::fs::read(&path)
            .await
            .with_context(|| format!("Failed to read L2 file: {}", path.display()))?;

        // 反序列化
        let data: MultiLevelKvData = bincode::deserialize(&bytes)
            .with_context(|| "Failed to deserialize L2 KV data")?;

        // 更新索引
        self.index.insert(key.to_string(), path);

        Ok(Some(data))
    }

    async fn put(&mut self, data: &MultiLevelKvData) -> Result<()> {
        let path = self.key_to_path(&data.key);

        // 确保目录存在
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        // 序列化
        let bytes = bincode::serialize(&data)
            .with_context(|| "Failed to serialize KV data for L2 storage")?;

        // 异步写入文件
        tokio::fs::write(&path, &bytes)
            .await
            .with_context(|| format!("Failed to write L2 file: {}", path.display()))?;

        // 更新索引
        self.index.insert(data.key.clone(), path);

        Ok(())
    }

    async fn delete(&mut self, key: &str) -> Result<()> {
        let path = self.key_to_path(key);
        
        if path.exists() {
            tokio::fs::remove_file(&path)
                .await
                .with_context(|| format!("Failed to delete L2 file: {}", path.display()))?;
        }

        self.index.remove(key);

        Ok(())
    }

    fn contains_key(&self, key: &str) -> bool {
        self.index.contains_key(key) || self.key_to_path(key).exists()
    }
}

/// 访问统计
#[derive(Debug, Clone)]
struct AccessStatistics {
    /// 总访问次数
    pub total_accesses: u64,
    /// L1 命中次数
    pub l1_hits: u64,
    /// L2 命中次数
    pub l2_hits: u64,
    /// L3 命中次数
    pub l3_hits: u64,
    /// 未命中次数
    pub misses: u64,
    /// 最后访问时间
    pub last_access: Instant,
}

impl AccessStatistics {
    fn new() -> Self {
        AccessStatistics {
            total_accesses: 0,
            l1_hits: 0,
            l2_hits: 0,
            l3_hits: 0,
            misses: 0,
            last_access: Instant::now(),
        }
    }

    fn record_hit(&mut self, tier: StorageTier) {
        self.total_accesses += 1;
        self.last_access = Instant::now();
        match tier {
            StorageTier::L1CpuMemory => self.l1_hits += 1,
            StorageTier::L2Disk => self.l2_hits += 1,
            StorageTier::L3Remote => self.l3_hits += 1,
        }
    }

    fn record_miss(&mut self) {
        self.total_accesses += 1;
        self.misses += 1;
        self.last_access = Instant::now();
    }

    /// 计算命中率
    pub fn hit_rate(&self) -> f64 {
        if self.total_accesses == 0 {
            0.0
        } else {
            let hits = self.l1_hits + self.l2_hits + self.l3_hits;
            hits as f64 / self.total_accesses as f64
        }
    }

    /// 计算 L1 命中率
    pub fn l1_hit_rate(&self) -> f64 {
        if self.total_accesses == 0 {
            0.0
        } else {
            self.l1_hits as f64 / self.total_accesses as f64
        }
    }
}

impl Default for AccessStatistics {
    fn default() -> Self {
        Self::new()
    }
}

/// 缓存指标
#[derive(Debug, Clone, Default)]
pub struct CacheMetrics {
    /// L1 缓存条目数
    pub l1_entries: usize,
    /// L2 缓存条目数
    pub l2_entries: usize,
    /// L3 缓存条目数（如果支持）
    pub l3_entries: usize,
    /// 总存储大小（字节）
    pub total_size_bytes: usize,
    /// L1 命中率
    pub l1_hit_rate: f64,
    /// 总命中率
    pub overall_hit_rate: f64,
    /// 从 L1 降级到 L2 的次数
    pub demote_l1_to_l2: u64,
    /// 从 L2 升级到 L1 的次数
    pub promote_l2_to_l1: u64,
    /// 从 L2 降级到 L3 的次数
    pub demote_l2_to_l3: u64,
}

impl MultiLevelCacheManager {
    /// 创建新的多级缓存管理器
    pub async fn new(config: MultiLevelCacheConfig) -> Result<Self> {
        let l1_cache = Arc::new(RwLock::new(L1CpuCache::new(config.l1_cache_size)));
        let l2_disk = Arc::new(RwLock::new(L2DiskStorage::new(&config.l2_disk_path)?));
        
        // L3 远程存储暂不实现，预留接口
        let l3_remote: Option<Arc<dyn RemoteStorageBackend>> = None;

        Ok(MultiLevelCacheManager {
            l1_cache,
            l2_disk,
            l3_remote,
            access_stats: Arc::new(RwLock::new(HashMap::new())),
            config,
            metrics: Arc::new(RwLock::new(CacheMetrics::default())),
        })
    }

    /// 创建默认配置的多级缓存管理器
    pub async fn with_default_config(disk_path: &Path) -> Result<Self> {
        let config = MultiLevelCacheConfig {
            l2_disk_path: disk_path.to_path_buf(),
            ..Default::default()
        };
        Self::new(config).await
    }

    /// 获取数据（自动从合适的层级）
    pub async fn get(&self, key: &str) -> Result<Option<MultiLevelKvData>> {
        let mut stats = self.access_stats.write().await;
        let stats = stats.entry(key.to_string()).or_insert_with(AccessStatistics::new);

        // 尝试 L1
        {
            let mut l1 = self.l1_cache.write().await;
            if let Some(data) = l1.get_mut(key) {
                data.record_access(StorageTier::L1CpuMemory);
                stats.record_hit(StorageTier::L1CpuMemory);
                return Ok(Some(data.clone()));
            }
        }

        // 尝试 L2
        {
            let mut l2 = self.l2_disk.write().await;
            if let Some(mut data) = l2.get(key).await? {
                data.record_access(StorageTier::L2Disk);
                stats.record_hit(StorageTier::L2Disk);

                // 如果是热点数据，升级到 L1
                if data.is_hot() && self.config.auto_tiering_enabled {
                    self.promote_to_l1(data.clone()).await?;
                }

                return Ok(Some(data));
            }
        }

        // 尝试 L3
        if let Some(ref l3) = self.l3_remote {
            if let Some(value) = l3.get(key).await? {
                let mut data = MultiLevelKvData::new(key.to_string(), value);
                data.record_access(StorageTier::L3Remote);
                stats.record_hit(StorageTier::L3Remote);

                // 如果是温/热点数据，升级到 L2
                if data.is_warm() || data.is_hot() {
                    self.promote_to_l2(data.clone()).await?;
                }

                return Ok(Some(data));
            }
        }

        // 未命中
        stats.record_miss();
        Ok(None)
    }

    /// 存储数据（自动选择合适的层级）
    pub async fn put(&self, data: MultiLevelKvData) -> Result<()> {
        let recommended_tier = data.recommended_tier();

        match recommended_tier {
            StorageTier::L1CpuMemory => {
                self.put_to_l1(data).await?;
            }
            StorageTier::L2Disk => {
                self.put_to_l2(data).await?;
            }
            StorageTier::L3Remote => {
                self.put_to_l3(data).await?;
            }
        }

        // 更新指标
        self.update_metrics().await;

        Ok(())
    }

    /// 直接存储到 L1
    async fn put_to_l1(&self, data: MultiLevelKvData) -> Result<()> {
        let mut l1 = self.l1_cache.write().await;
        
        // 如果被淘汰的条目是热点数据，先降级到 L2
        if !l1.contains_key(&data.key) && l1.len() >= l1.capacity {
            // 获取 LRU 条目
            if let Some(oldest_key) = l1.lru_order.first().cloned() {
                if let Some(oldest_data) = l1.data.get(&oldest_key) {
                    let mut oldest_data = oldest_data.clone();
                    oldest_data.current_tier = StorageTier::L2Disk;
                    
                    let mut l2 = self.l2_disk.write().await;
                    l2.put(&oldest_data).await?;
                    
                    let mut metrics = self.metrics.write().await;
                    metrics.demote_l1_to_l2 += 1;
                }
            }
        }

        l1.put(data.key.clone(), data);
        Ok(())
    }

    /// 直接存储到 L2
    async fn put_to_l2(&self, data: MultiLevelKvData) -> Result<()> {
        let mut l2 = self.l2_disk.write().await;
        l2.put(&data).await?;
        
        // 如果 L1 中存在，删除（避免冗余）
        if self.config.auto_tiering_enabled {
            let mut l1 = self.l1_cache.write().await;
            l1.remove(&data.key);
        }

        Ok(())
    }

    /// 直接存储到 L3
    async fn put_to_l3(&self, data: MultiLevelKvData) -> Result<()> {
        if let Some(ref l3) = self.l3_remote {
            l3.put(&data.key, data.value).await?;
            
            // 如果 L1/L2 中存在，删除（避免冗余）
            if self.config.auto_tiering_enabled {
                let mut l1 = self.l1_cache.write().await;
                l1.remove(&data.key);
                
                let mut l2 = self.l2_disk.write().await;
                l2.delete(&data.key).await?;
                
                let mut metrics = self.metrics.write().await;
                metrics.demote_l2_to_l3 += 1;
            }
        } else {
            // 没有 L3，降级到 L2
            self.put_to_l2(data).await?;
        }

        Ok(())
    }

    /// 升级到 L1
    async fn promote_to_l1(&self, data: MultiLevelKvData) -> Result<()> {
        self.put_to_l1(data).await?;
        
        let mut metrics = self.metrics.write().await;
        metrics.promote_l2_to_l1 += 1;

        Ok(())
    }

    /// 升级到 L2
    async fn promote_to_l2(&self, data: MultiLevelKvData) -> Result<()> {
        self.put_to_l2(data).await?;
        Ok(())
    }

    /// 删除数据
    pub async fn delete(&self, key: &str) -> Result<()> {
        // 从 L1 删除
        {
            let mut l1 = self.l1_cache.write().await;
            l1.remove(key);
        }

        // 从 L2 删除
        {
            let mut l2 = self.l2_disk.write().await;
            l2.delete(key).await?;
        }

        // 从 L3 删除
        if let Some(ref l3) = self.l3_remote {
            l3.delete(key).await?;
        }

        Ok(())
    }

    /// 检查数据是否存在
    pub async fn contains_key(&self, key: &str) -> bool {
        // 检查 L1
        {
            let l1 = self.l1_cache.read().await;
            if l1.contains_key(key) {
                return true;
            }
        }

        // 检查 L2
        {
            let l2 = self.l2_disk.read().await;
            if l2.contains_key(key) {
                return true;
            }
        }

        // 检查 L3
        if let Some(ref l3) = self.l3_remote {
            if let Ok(exists) = l3.exists(key).await {
                return exists;
            }
        }

        false
    }

    /// 获取缓存指标
    pub async fn get_metrics(&self) -> CacheMetrics {
        let mut metrics = self.metrics.write().await;
        
        // 更新实时数据
        let l1 = self.l1_cache.read().await;
        let l2 = self.l2_disk.read().await;
        
        metrics.l1_entries = l1.len();
        metrics.l2_entries = l2.index.len();
        
        // 计算命中率
        let stats = self.access_stats.read().await;
        let total_stats: AccessStatistics = stats.values().cloned().fold(
            AccessStatistics::new(),
            |mut acc, s| {
                acc.total_accesses += s.total_accesses;
                acc.l1_hits += s.l1_hits;
                acc.l2_hits += s.l2_hits;
                acc.l3_hits += s.l3_hits;
                acc.misses += s.misses;
                acc
            }
        );
        
        metrics.l1_hit_rate = total_stats.l1_hit_rate();
        metrics.overall_hit_rate = total_stats.hit_rate();
        
        metrics.clone()
    }

    /// 更新指标
    async fn update_metrics(&self) {
        let l1 = self.l1_cache.read().await;
        let l2 = self.l2_disk.read().await;
        
        let mut metrics = self.metrics.write().await;
        metrics.l1_entries = l1.len();
        metrics.l2_entries = l2.index.len();
        
        // 计算总大小
        metrics.total_size_bytes = l1.data.values()
            .map(|d| d.size_bytes)
            .sum();
    }

    /// 后台任务：自动升降级
    pub async fn start_auto_tiering_background_task(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            
            loop {
                interval.tick().await;
                
                // 检查 L1 中的冷数据，降级到 L2
                {
                    let mut l1 = self.l1_cache.write().await;
                    let keys_to_demote: Vec<String> = l1.data
                        .iter()
                        .filter(|(_, data)| data.is_cold())
                        .map(|(key, _)| key.clone())
                        .collect();

                    for key in keys_to_demote {
                        if let Some(data) = l1.remove(&key) {
                            let mut l2 = self.l2_disk.write().await;
                            if let Ok(_) = l2.put(&data).await {
                                let mut metrics = self.metrics.write().await;
                                metrics.demote_l1_to_l2 += 1;
                            }
                        }
                    }
                }

                // 更新指标
                self.update_metrics().await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_basic_put_get() {
        let temp_dir = TempDir::new().unwrap();
        let cache = MultiLevelCacheManager::with_default_config(temp_dir.path()).await.unwrap();

        let data = MultiLevelKvData::new("key1".to_string(), b"value1".to_vec());
        cache.put(data).await.unwrap();

        let result = cache.get("key1").await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().value, b"value1");
    }

    #[tokio::test]
    async fn test_l1_cache_eviction() {
        let temp_dir = TempDir::new().unwrap();
        let config = MultiLevelCacheConfig {
            l1_cache_size: 3,
            l2_disk_path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let cache = MultiLevelCacheManager::new(config).await.unwrap();

        // 填满 L1 缓存
        for i in 0..3 {
            let key = format!("key{}", i);
            let value = format!("value{}", i).into_bytes();
            let data = MultiLevelKvData::new(key, value);
            cache.put(data).await.unwrap();
        }

        // 添加第 4 个，应该触发 LRU 淘汰
        let data = MultiLevelKvData::new("key_new".to_string(), b"value_new".to_vec());
        cache.put(data).await.unwrap();

        // 检查 L1 大小
        let metrics = cache.get_metrics().await;
        assert!(metrics.l1_entries <= 3);
    }

    #[tokio::test]
    async fn test_auto_promote_hot_data() {
        let temp_dir = TempDir::new().unwrap();
        let cache = MultiLevelCacheManager::with_default_config(temp_dir.path()).await.unwrap();

        // 创建冷数据（直接放到 L2）
        let mut data = MultiLevelKvData::new("key1".to_string(), b"value1".to_vec());
        data.current_tier = StorageTier::L2Disk;
        cache.put(data).await.unwrap();

        // 多次访问，模拟热点
        for _ in 0..15 {
            let _ = cache.get("key1").await;
        }

        // 检查是否升级到 L1
        let metrics = cache.get_metrics().await;
        assert!(metrics.l1_hit_rate > 0.0);
    }

    #[tokio::test]
    async fn test_contains_key() {
        let temp_dir = TempDir::new().unwrap();
        let cache = MultiLevelCacheManager::with_default_config(temp_dir.path()).await.unwrap();

        let data = MultiLevelKvData::new("key1".to_string(), b"value1".to_vec());
        cache.put(data).await.unwrap();

        assert!(cache.contains_key("key1").await);
        assert!(!cache.contains_key("key_not_exist").await);
    }

    #[tokio::test]
    async fn test_delete() {
        let temp_dir = TempDir::new().unwrap();
        let cache = MultiLevelCacheManager::with_default_config(temp_dir.path()).await.unwrap();

        let data = MultiLevelKvData::new("key1".to_string(), b"value1".to_vec());
        cache.put(data).await.unwrap();

        cache.delete("key1").await.unwrap();

        assert!(!cache.contains_key("key1").await);
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Arc::new(MultiLevelCacheManager::with_default_config(temp_dir.path()).await.unwrap());

        // 并发写入
        let mut handles = vec![];
        for i in 0..10 {
            let cache_clone = Arc::clone(&cache);
            let handle = tokio::spawn(async move {
                let key = format!("key{}", i);
                let value = format!("value{}", i).into_bytes();
                let data = MultiLevelKvData::new(key, value);
                cache_clone.put(data).await.unwrap();
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // 并发读取
        let mut handles = vec![];
        for i in 0..10 {
            let cache_clone = Arc::clone(&cache);
            let handle = tokio::spawn(async move {
                let key = format!("key{}", i);
                let result = cache_clone.get(&key).await.unwrap();
                assert!(result.is_some());
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_metrics_collection() {
        let temp_dir = TempDir::new().unwrap();
        let cache = MultiLevelCacheManager::with_default_config(temp_dir.path()).await.unwrap();

        // 写入一些数据
        for i in 0..5 {
            let key = format!("key{}", i);
            let value = format!("value{}", i).into_bytes();
            let data = MultiLevelKvData::new(key, value);
            cache.put(data).await.unwrap();
        }

        // 读取一些数据
        for i in 0..3 {
            let key = format!("key{}", i);
            let _ = cache.get(&key).await;
        }

        let metrics = cache.get_metrics().await;
        assert_eq!(metrics.l1_entries, 3); // 只有读取的 3 个在 L1
        assert!(metrics.overall_hit_rate > 0.0);
    }
}
