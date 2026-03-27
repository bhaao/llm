//! 统一多级缓存管理模块 - L1 CPU + L2 Disk + L3 Remote
//!
//! **核心功能**：
//! - L1: CPU 内存缓存（DashMap, LRU 淘汰）
//! - L2: 磁盘存储（温数据，持久化）
//! - L3: 远程存储（Redis，冷数据，按需加载）
//! - 自动升降级策略（基于访问频率）
//!
//! # 架构设计
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │  客户端请求                                              │
//! │         ↓                                               │
//! │  ┌─────────────────────────────────────────────────┐    │
//! │  │ L1: CPU 内存缓存 (DashMap, LRU)                 │    │
//! │  │     - 容量：1000-5000 条目                       │    │
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
//! │  │ L3: 远程存储 (Redis)                             │    │
//! │  │     - 容量：TB+                                 │    │
//! │  │     - 延迟：100-500ms                           │    │
//! │  │     - 热度：访问次数 < 4                         │    │
//! │  └─────────────────────────────────────────────────┘    │
//! └─────────────────────────────────────────────────────────┘
//! ```
//!
//! # 热度判断策略
//!
//! | 访问次数 | 存储层级 | 说明       |
//! |---------|---------|-----------|
//! | > 10    | L1 内存  | 热点数据   |
//! | 4-10    | L2 磁盘  | 温数据     |
//! | < 4     | L3 远程  | 冷数据     |

#![cfg(feature = "tiered-storage")]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use anyhow::{Result, Context};
use dashmap::DashMap;
use linked_hash_map::LinkedHashMap;

/// KV 数据块
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
        }
    }

    /// 记录一次访问
    pub fn record_access(&mut self) {
        self.access_count += 1;
        self.last_accessed_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
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
        self.access_count < 4
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
            promote_access_threshold: 10,
            auto_tiering_enabled: true,
        }
    }
}

/// 访问统计
#[derive(Debug, Clone)]
pub struct AccessStatistics {
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
}

/// 统一多级缓存管理器
pub struct MultiLevelCacheManager {
    /// L1 CPU 缓存（使用 DashMap 实现细粒度锁）
    l1_cache: Arc<DashMap<String, MultiLevelKvData>>,
    /// L1 LRU 顺序（用于淘汰）- 使用 LinkedHashMap 实现 O(1) LRU
    l1_lru: Arc<RwLock<LinkedHashMap<String, ()>>>,
    /// L2 磁盘存储路径
    l2_disk_path: PathBuf,
    /// L2 本地索引
    l2_index: Arc<DashMap<String, PathBuf>>,
    /// L3 远程存储（可选）
    l3_remote: Option<Arc<dyn RemoteStorageBackend>>,
    /// 访问统计
    access_stats: Arc<DashMap<String, AccessStatistics>>,
    /// 配置
    config: MultiLevelCacheConfig,
    /// 指标统计
    metrics: Arc<RwLock<CacheMetrics>>,
    /// 智能预取器（可选）
    prefetcher: Option<Arc<crate::prefetcher::Prefetcher>>,
}

impl MultiLevelCacheManager {
    /// 创建新的多级缓存管理器
    pub async fn new(config: MultiLevelCacheConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.l2_disk_path)
            .with_context(|| format!("Failed to create L2 disk directory: {}", config.l2_disk_path.display()))?;

        // L3 远程存储：根据配置创建 Redis 后端
        let l3_remote: Option<Arc<dyn RemoteStorageBackend>> = if let Some(ref remote_config) = config.l3_remote_config {
            #[cfg(feature = "redis-backend")]
            {
                // 使用 remote_config 创建 Redis 后端
                match crate::redis_backend::RedisStorageBackend::new(remote_config.clone()).await {
                    Ok(backend) => Some(Arc::new(backend)),
                    Err(e) => {
                        tracing::warn!("Failed to create Redis backend: {}. L3 remote storage disabled.", e);
                        None
                    }
                }
            }
            #[cfg(not(feature = "redis-backend"))]
            {
                // 避免未使用变量警告
                let _ = remote_config;
                tracing::warn!("Redis backend feature not enabled. L3 remote storage disabled.");
                None
            }
        } else {
            None
        };

        Ok(MultiLevelCacheManager {
            l1_cache: Arc::new(DashMap::new()),
            l1_lru: Arc::new(RwLock::new(LinkedHashMap::with_capacity(config.l1_cache_size))),
            l2_disk_path: config.l2_disk_path.clone(),
            l2_index: Arc::new(DashMap::new()),
            l3_remote,
            access_stats: Arc::new(DashMap::new()),
            config,
            metrics: Arc::new(RwLock::new(CacheMetrics::default())),
            prefetcher: None, // 预取器默认不启用，可通过 enable_prefetcher() 启用
        })
    }

    /// 启用预取器
    ///
    /// # 参数
    ///
    /// * `prefetch_window` - 预取窗口大小（预测多少个后续数据）
    /// * `max_history_size` - 最大历史记录数
    /// * `decay_factor` - 时间衰减因子（0.9 表示每小时衰减 10%）
    pub fn enable_prefetcher(&mut self, prefetch_window: usize, max_history_size: usize, decay_factor: f64) {
        self.prefetcher = Some(Arc::new(crate::prefetcher::Prefetcher::new(prefetch_window, max_history_size, decay_factor)));
    }

    /// 创建默认配置的多级缓存管理器
    pub async fn with_default_config(disk_path: &Path) -> Result<Self> {
        let config = MultiLevelCacheConfig {
            l2_disk_path: disk_path.to_path_buf(),
            ..Default::default()
        };
        Self::new(config).await
    }

    /// 创建默认配置的多级缓存管理器（带预取器）
    pub async fn with_default_config_and_prefetcher(disk_path: &Path, prefetch_window: usize) -> Result<Self> {
        let config = MultiLevelCacheConfig {
            l2_disk_path: disk_path.to_path_buf(),
            ..Default::default()
        };
        let mut manager = Self::new(config).await?;
        manager.enable_prefetcher(prefetch_window, 100_000, 0.9);
        Ok(manager)
    }

    /// 启动时从 L2 磁盘预加载热点数据到 L1
    ///
    /// 该方法扫描 L2 磁盘目录，读取所有 KV 数据文件，
    /// 将热点数据（访问次数 > 10）加载到 L1 缓存中。
    ///
    /// # 性能说明
    ///
    /// - 首次启动时可能需要较长时间（取决于 L2 数据量）
    /// - 建议限制预加载的热点数据数量（默认最多 500 条）
    /// - 预加载完成后 L1 缓存即可命中，提升后续访问性能
    pub async fn preload_hot_data_from_l2(&self, max_items: usize) -> Result<usize> {
        use tokio::fs;
        use tokio::sync::Semaphore;
        use std::sync::Arc;

        let mut loaded_count = 0;

        // 扫描 L2 磁盘目录
        let mut entries = fs::read_dir(&self.l2_disk_path).await?;
        let mut l2_files: Vec<PathBuf> = Vec::new();

        // 收集所有 L2 文件（最多扫描 10000 个文件）
        let mut file_count = 0;
        while let Some(entry) = entries.next_entry().await? {
            if file_count >= 10000 {
                break;
            }
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("kv") {
                l2_files.push(path);
            }
            file_count += 1;
        }

        // 使用信号量限制并发数（最多 10 个并发）
        let semaphore = Arc::new(Semaphore::new(10));
        let mut tasks = Vec::new();

        for path in l2_files {
            let sem = Arc::clone(&semaphore);
            let task = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                let bytes = fs::read(&path).await.ok()?;
                let data: MultiLevelKvData = bincode::deserialize(&bytes).ok()?;
                Some((data, path))
            });
            tasks.push(task);
        }

        // 收集所有结果
        let results = futures::future::join_all(tasks).await;

        for result in results {
            if let Ok(Some((data, path))) = result {
                if data.is_hot() && loaded_count < max_items {
                    self.l1_cache.insert(data.key.clone(), data.clone());
                    self.l2_index.insert(data.key.clone(), path);
                    loaded_count += 1;
                }
            }
        }

        tracing::info!("Preloaded {} hot items from L2 to L1", loaded_count);
        Ok(loaded_count)
    }

    /// 获取数据（自动从合适的层级）
    pub async fn get(&self, key: &str) -> Result<Option<MultiLevelKvData>> {
        let mut stats = self.access_stats.entry(key.to_string()).or_insert_with(AccessStatistics::default);

        // 尝试 L1
        if let Some(mut data) = self.l1_cache.get_mut(key) {
            data.record_access();
            stats.record_hit(StorageTier::L1CpuMemory);
            self.update_lru_order(key).await;
            // 记录访问到预取器
            if let Some(ref prefetcher) = self.prefetcher {
                prefetcher.record_access(key.to_string()).await;
            }
            return Ok(Some(data.clone()));
        }

        // 尝试 L2
        let l2_path = self.key_to_path(key);
        if l2_path.exists() {
            let bytes = tokio::fs::read(&l2_path)
                .await
                .with_context(|| format!("Failed to read L2 file: {}", l2_path.display()))?;

            let mut data: MultiLevelKvData = bincode::deserialize(&bytes)
                .with_context(|| "Failed to deserialize L2 KV data")?;

            data.record_access();
            stats.record_hit(StorageTier::L2Disk);

            // 记录访问到预取器
            if let Some(ref prefetcher) = self.prefetcher {
                prefetcher.record_access(key.to_string()).await;
            }

            // 如果是热点数据，升级到 L1
            if data.is_hot() && self.config.auto_tiering_enabled {
                self.promote_to_l1(data.clone()).await?;
                let mut metrics = self.metrics.write().await;
                metrics.promote_l2_to_l1 += 1;
            }

            // 更新 L2 索引
            self.l2_index.insert(key.to_string(), l2_path);

            return Ok(Some(data));
        }

        // 尝试 L3
        if let Some(ref l3) = self.l3_remote {
            if let Some(value) = l3.get(key).await? {
                let mut data = MultiLevelKvData::new(key.to_string(), value);
                data.record_access();
                stats.record_hit(StorageTier::L3Remote);

                // 记录访问到预取器
                if let Some(ref prefetcher) = self.prefetcher {
                    prefetcher.record_access(key.to_string()).await;
                }

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

    /// 预测下一个可能访问的 keys（如果启用了预取器）
    ///
    /// # 返回
    ///
    /// * `Vec<String>` - 预测的 key 列表
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// let predictions = manager.predict_next().await;
    /// for key in predictions {
    ///     // 预取数据到 L1
    ///     if let Some(data) = manager.get(&key).await? {
    ///         manager.promote_to_l1(data).await?;
    ///     }
    /// }
    /// ```
    pub async fn predict_next(&self) -> Vec<String> {
        if let Some(ref prefetcher) = self.prefetcher {
            prefetcher.predict_next().await
        } else {
            Vec::new()
        }
    }

    /// 预取预测的数据到 L1
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// manager.prefetch_predicted_data().await?;
    /// ```
    pub async fn prefetch_predicted_data(&self) -> Result<usize> {
        let predictions = self.predict_next().await;
        let mut prefetched_count = 0;

        for key in &predictions {
            if !self.l1_cache.contains_key(key) {
                if let Some(data) = self.get(key).await? {
                    self.promote_to_l1(data).await?;
                    prefetched_count += 1;
                }
            }
        }

        Ok(prefetched_count)
    }

    /// 直接存储到 L1
    async fn put_to_l1(&self, data: MultiLevelKvData) -> Result<()> {
        // 如果超出容量，淘汰 LRU 条目
        while self.l1_cache.len() >= self.config.l1_cache_size {
            let mut lru = self.l1_lru.write().await;
            if let Some((oldest_key, _)) = lru.pop_front() {
                drop(lru); // 释放写锁
                if let Some((_, oldest_data)) = self.l1_cache.remove(&oldest_key) {
                    // 降级到 L2
                    self.put_to_l2(oldest_data).await?;

                    let mut metrics = self.metrics.write().await;
                    metrics.demote_l1_to_l2 += 1;
                }
            } else {
                break;
            }
        }

        self.l1_cache.insert(data.key.clone(), data.clone());
        self.update_lru_order(&data.key).await;

        Ok(())
    }

    /// 直接存储到 L2
    async fn put_to_l2(&self, data: MultiLevelKvData) -> Result<()> {
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
        self.l2_index.insert(data.key.clone(), path);

        // 如果 L1 中存在，删除（避免冗余）
        if self.config.auto_tiering_enabled {
            self.l1_cache.remove(&data.key);
        }

        Ok(())
    }

    /// 直接存储到 L3
    async fn put_to_l3(&self, data: MultiLevelKvData) -> Result<()> {
        if let Some(ref l3) = self.l3_remote {
            l3.put(&data.key, data.value).await?;

            // 如果 L1/L2 中存在，删除（避免冗余）
            if self.config.auto_tiering_enabled {
                self.l1_cache.remove(&data.key);
                let path = self.key_to_path(&data.key);
                if path.exists() {
                    tokio::fs::remove_file(&path).await?;
                    self.l2_index.remove(&data.key);
                }
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
        self.l1_cache.remove(key);

        // 从 L2 删除
        let path = self.key_to_path(key);
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }
        self.l2_index.remove(key);

        // 从 L3 删除
        if let Some(ref l3) = self.l3_remote {
            l3.delete(key).await?;
        }

        Ok(())
    }

    /// 检查数据是否存在
    pub async fn contains_key(&self, key: &str) -> bool {
        // 检查 L1
        if self.l1_cache.contains_key(key) {
            return true;
        }

        // 检查 L2
        if self.l2_index.contains_key(key) || self.key_to_path(key).exists() {
            return true;
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
        metrics.l1_entries = self.l1_cache.len();
        metrics.l2_entries = self.l2_index.len();

        // 计算总大小
        metrics.total_size_bytes = self.l1_cache.iter()
            .map(|r| r.value().size_bytes)
            .sum();

        // 计算命中率
        let total_stats: AccessStatistics = self.access_stats.iter()
            .map(|r| r.value().clone())
            .fold(AccessStatistics::default(), |mut acc, s| {
                acc.total_accesses += s.total_accesses;
                acc.l1_hits += s.l1_hits;
                acc.l2_hits += s.l2_hits;
                acc.l3_hits += s.l3_hits;
                acc.misses += s.misses;
                acc
            });

        metrics.l1_hit_rate = if total_stats.total_accesses == 0 {
            0.0
        } else {
            total_stats.l1_hits as f64 / total_stats.total_accesses as f64
        };

        metrics.overall_hit_rate = if total_stats.total_accesses == 0 {
            0.0
        } else {
            let hits = total_stats.l1_hits + total_stats.l2_hits + total_stats.l3_hits;
            hits as f64 / total_stats.total_accesses as f64
        };

        metrics.clone()
    }

    /// 更新指标
    async fn update_metrics(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.l1_entries = self.l1_cache.len();
        metrics.l2_entries = self.l2_index.len();

        // 计算总大小
        metrics.total_size_bytes = self.l1_cache.iter()
            .map(|r| r.value().size_bytes)
            .sum();
    }

    /// 更新 LRU 顺序（使用 LinkedHashMap 实现真正的 O(1) 操作）
    async fn update_lru_order(&self, key: &str) {
        let mut lru = self.l1_lru.write().await;

        // 移除已存在的并添加到末尾（LinkedHashMap 的 insert 和 get_mut 都是 O(1)）
        lru.insert(key.to_string(), ());

        // 超出容量时移除最旧的（前端）- O(1)
        while lru.len() > self.config.l1_cache_size {
            if let Some((oldest, _)) = lru.pop_front() {
                self.l1_cache.remove(&oldest);
            }
        }
    }

    /// key 转路径（使用 SHA256 哈希的前 8 个字符作为子目录）
    fn key_to_path(&self, key: &str) -> PathBuf {
        use sha2::{Sha256, Digest};
        let hash = Sha256::digest(key.as_bytes());
        let prefix = hex::encode(&hash[..4]);
        self.l2_disk_path.join(prefix).join(format!("{}.kv", key))
    }

    /// 后台任务：自动升降级
    pub async fn start_auto_tiering_background_task(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));

            loop {
                interval.tick().await;

                // 检查 L1 中的冷数据，降级到 L2
                let keys_to_demote: Vec<String> = self.l1_cache.iter()
                    .filter(|r| r.value().is_cold())
                    .map(|r| r.key().clone())
                    .collect();

                for key in keys_to_demote {
                    if let Some((_, data)) = self.l1_cache.remove(&key) {
                        if let Ok(_) = self.put_to_l2(data).await {
                            let mut metrics = self.metrics.write().await;
                            metrics.demote_l1_to_l2 += 1;
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

        // 写入 4 个热点数据（超过容量 3）
        for i in 0..4 {
            let mut data = MultiLevelKvData::new(format!("key_{}", i), format!("value_{}", i).as_bytes().to_vec());
            // 模拟多次访问使其成为热点
            for _ in 0..15 {
                data.record_access();
            }
            cache.put(data).await.unwrap();
        }

        // L1 缓存应该只有 3 个
        assert!(cache.l1_cache.len() <= 3);

        // 最早写入的 key_0 应该被淘汰到 L2，但仍能通过 get 获取
        let result = cache.get("key_0").await.unwrap();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_hot_data_promotion() {
        let temp_dir = TempDir::new().unwrap();
        let cache = MultiLevelCacheManager::with_default_config(temp_dir.path()).await.unwrap();

        // 写入热点数据（模拟多次访问）
        let mut data = MultiLevelKvData::new("hot_key".to_string(), b"hot_value".to_vec());
        for _ in 0..15 {
            data.record_access();
        }
        cache.put(data).await.unwrap();

        // 现在应该在 L1
        assert!(cache.l1_cache.contains_key("hot_key"));
    }

    #[tokio::test]
    async fn test_metrics() {
        let temp_dir = TempDir::new().unwrap();
        let cache = MultiLevelCacheManager::with_default_config(temp_dir.path()).await.unwrap();

        let data = MultiLevelKvData::new("key1".to_string(), b"value1".to_vec());
        cache.put(data).await.unwrap();

        let _ = cache.get("key1").await.unwrap();
        let _ = cache.get("key1").await.unwrap();

        let metrics = cache.get_metrics().await;
        assert!(metrics.l1_entries > 0 || metrics.l2_entries > 0);
    }
}
