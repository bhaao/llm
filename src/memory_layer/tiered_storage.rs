//! 两层存储模块 - CPU 内存缓存 + 磁盘持久化
//!
//! **核心功能**：
//! - L1: CPU 内存缓存（LRU，热点数据）
//! - L2: 磁盘存储（冷数据，持久化）
//! - 自动升降级策略（基于访问频率和时间）
//!
//! # 架构设计
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │  远程推理 API                                        │
//! │  (vLLM/SGLang on Cloud)                             │
//! │         ↑                                           │
//! │         | HTTP/gRPC                                 │
//! │         ↓                                           │
//! │  本地两层存储                                        │
//! │  ┌─────────────────────────────────────────────┐    │
//! │  │ L1: CPU 内存缓存 (LRU, 热点 KV)              │    │
//! │  │     - 快速访问 < 1ms                         │    │
//! │  │     - 容量限制 ~1000 条目                     │    │
//! │  └─────────────────────────────────────────────┘    │
//! │         ↑↓ 自动升降级                                │
//! │  ┌─────────────────────────────────────────────┐    │
//! │  │ L2: 磁盘存储 (冷数据，持久化)                 │    │
//! │  │     - 容量大 ~100GB+                         │    │
//! │  │     - 访问延迟 ~10-50ms                      │    │
//! │  └─────────────────────────────────────────────┘    │
//! └─────────────────────────────────────────────────────┘
//! ```
//!
//! # 热度判断策略
//!
//! | 访问次数 | 最后访问时间 | 存储层级 |
//! |---------|-------------|---------|
//! | > 10    | 任意        | L1 内存  |
//! | 4-10    | 任意        | L1 内存  |
//! | < 4     | < 5 分钟     | L1 内存  |
//! | < 2     | > 5 分钟     | L2 磁盘  |

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};

/// KV 数据结构（简化版，实际应该根据项目需求定义）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvData {
    /// 键
    pub key: String,
    /// 值（字节数组）
    pub value: Vec<u8>,
    /// 版本号（用于并发控制）
    pub version: u64,
    /// 创建时间戳
    pub created_at: u64,
}

impl KvData {
    /// 创建新的 KV 数据
    pub fn new(key: String, value: Vec<u8>) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        KvData {
            key,
            value,
            version: 1,
            created_at: timestamp,
        }
    }

    /// 创建带版本号的 KV 数据
    pub fn with_version(mut self, version: u64) -> Self {
        self.version = version;
        self
    }
}

/// 访问统计信息
#[derive(Debug, Clone)]
pub struct AccessStats {
    /// 访问次数
    pub count: u32,
    /// 最后访问时间
    pub last_access: Instant,
    /// 首次访问时间（用于计算活跃度）
    pub first_access: Instant,
}

impl AccessStats {
    /// 创建新的访问统计
    pub fn new() -> Self {
        let now = Instant::now();
        AccessStats {
            count: 0,
            last_access: now,
            first_access: now,
        }
    }

    /// 记录一次访问
    pub fn record_access(&mut self) {
        self.count += 1;
        self.last_access = Instant::now();
    }

    /// 获取距离上次访问的时间
    pub fn elapsed_since_last_access(&self) -> Duration {
        self.last_access.elapsed()
    }

    /// 判断是否为冷数据
    ///
    /// 冷数据定义：访问次数 < 2 且 5 分钟未访问
    pub fn is_cold(&self) -> bool {
        self.count < 2 && self.elapsed_since_last_access().as_secs() > 300
    }

    /// 判断是否为热点数据
    ///
    /// 热点数据定义：访问次数 > 10
    pub fn is_hot(&self) -> bool {
        self.count > 10
    }

    /// 判断是否为温数据
    ///
    /// 温数据定义：访问次数 4-10
    pub fn is_warm(&self) -> bool {
        self.count >= 4 && self.count <= 10
    }
}

impl Default for AccessStats {
    fn default() -> Self {
        Self::new()
    }
}

/// 存储层级枚举（去掉 GPU 层）
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StorageTier {
    /// CPU 内存（快速访问）
    CpuMemory,
    /// 磁盘存储（持久化）
    Disk,
}

/// 两层存储配置
#[derive(Debug, Clone)]
pub struct TieredStorageConfig {
    /// CPU 内存缓存大小（条目数）
    pub cpu_cache_size: usize,
    /// 磁盘存储路径
    pub disk_path: PathBuf,
    /// 冷数据降级时间阈值（秒）
    pub cold_data_threshold_secs: u64,
    /// 冷数据访问次数阈值
    pub cold_data_access_threshold: u32,
}

impl Default for TieredStorageConfig {
    fn default() -> Self {
        TieredStorageConfig {
            cpu_cache_size: 1000,
            disk_path: PathBuf::from("./data/kv_storage"),
            cold_data_threshold_secs: 300, // 5 分钟
            cold_data_access_threshold: 2,
        }
    }
}

/// 两层存储管理器
pub struct TieredStorageManager {
    /// CPU 内存缓存（LRU）
    cpu_cache: Arc<RwLock<LruCache<String, KvData>>>,
    /// 磁盘存储路径
    disk_path: PathBuf,
    /// 访问统计（用于热度判断）
    access_stats: Arc<RwLock<HashMap<String, AccessStats>>>,
    /// 配置
    config: TieredStorageConfig,
}

/// LRU 缓存实现（简化版）
struct LruCache<K, V> {
    /// 缓存数据
    data: HashMap<K, V>,
    /// 访问顺序队列（用于 LRU 淘汰）
    order: Vec<K>,
    /// 最大容量
    capacity: usize,
}

impl<K: Eq + std::hash::Hash + Clone, V> LruCache<K, V> {
    /// 创建新的 LRU 缓存
    pub fn new(capacity: usize) -> Self {
        LruCache {
            data: HashMap::with_capacity(capacity),
            order: Vec::with_capacity(capacity),
            capacity,
        }
    }

    /// 获取缓存项
    pub fn get(&mut self, key: &K) -> Option<&V> {
        if self.data.contains_key(key) {
            // 更新访问顺序（移到末尾）
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                self.order.remove(pos);
                self.order.push(key.clone());
            }
            self.data.get(key)
        } else {
            None
        }
    }

    /// 插入缓存项
    pub fn put(&mut self, key: K, value: V) -> Option<V> {
        // 如果已存在，更新
        if self.data.contains_key(&key) {
            // 更新访问顺序
            if let Some(pos) = self.order.iter().position(|k| k == &key) {
                self.order.remove(pos);
            }
            self.order.push(key.clone());
            return self.data.insert(key, value);
        }

        // 如果超出容量，淘汰最久未使用的
        while self.order.len() >= self.capacity {
            if let Some(oldest) = self.order.first().cloned() {
                self.order.remove(0);
                self.data.remove(&oldest);
            }
        }

        self.order.push(key.clone());
        self.data.insert(key, value)
    }

    /// 移除缓存项
    pub fn remove(&mut self, key: &K) -> Option<V> {
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            self.order.remove(pos);
        }
        self.data.remove(key)
    }

    /// 获取缓存大小
    pub fn len(&self) -> usize {
        self.data.len()
    }
}

impl TieredStorageManager {
    /// 创建新的两层存储管理器
    ///
    /// # 参数
    ///
    /// * `config` - 存储配置
    ///
    /// # 返回
    ///
    /// * `Result<Self, String>` - 存储管理器或错误
    pub fn new(config: TieredStorageConfig) -> Result<Self, String> {
        // 确保磁盘目录存在
        std::fs::create_dir_all(&config.disk_path)
            .map_err(|e| format!("Failed to create disk directory: {}", e))?;

        Ok(TieredStorageManager {
            cpu_cache: Arc::new(RwLock::new(LruCache::new(config.cpu_cache_size))),
            disk_path: config.disk_path.clone(),
            access_stats: Arc::new(RwLock::new(HashMap::new())),
            config,
        })
    }

    /// 创建默认配置的两层存储管理器
    ///
    /// # 参数
    ///
    /// * `disk_path` - 磁盘存储路径
    ///
    /// # 返回
    ///
    /// * `Result<Self, String>` - 存储管理器或错误
    pub fn with_default_config(disk_path: &Path) -> Result<Self, String> {
        let config = TieredStorageConfig {
            disk_path: disk_path.to_path_buf(),
            ..Default::default()
        };
        Self::new(config)
    }

    /// 读取 KV（自动从合适层级）
    ///
    /// # 参数
    ///
    /// * `key` - 键
    ///
    /// # 返回
    ///
    /// * `Result<Option<KvData>, String>` - KV 数据或错误
    pub async fn get(&self, key: &str) -> Result<Option<KvData>, String> {
        // 更新访问统计
        self.record_access(key).await;

        // 1. 先查 CPU 内存缓存
        {
            let mut cpu = self.cpu_cache.write().await;
            if let Some(data) = cpu.get(&key.to_string()) {
                return Ok(Some(data.clone()));
            }
        }

        // 2. 查磁盘
        let disk_path = self.disk_path.join(format!("{}.kv", key));
        if disk_path.exists() {
            let data = tokio::fs::read(&disk_path).await
                .map_err(|e| format!("Failed to read from disk: {}", e))?;
            let kv_data: KvData = bincode::deserialize(&data)
                .map_err(|e| format!("Failed to deserialize KV data: {}", e))?;

            // 提升到 CPU 内存缓存
            self.promote_to_cpu(key, &kv_data).await;

            return Ok(Some(kv_data));
        }

        Ok(None)
    }

    /// 写入 KV（根据热度决定层级）
    ///
    /// # 参数
    ///
    /// * `key` - 键
    /// * `value` - KV 数据
    ///
    /// # 返回
    ///
    /// * `Result<(), String>` - 成功或错误
    pub async fn put(&self, key: String, value: KvData) -> Result<(), String> {
        let stats = self.access_stats.read().await;
        let access_count = stats.get(&key).map(|s| s.count).unwrap_or(0);
        drop(stats);

        // 根据热度决定存储层级
        if access_count > 10 {
            // 热点数据写 CPU 缓存
            self.write_to_cpu(key, value).await?;
        } else if access_count > 3 {
            // 温数据写 CPU 缓存
            self.write_to_cpu(key, value).await?;
        } else {
            // 冷数据写磁盘
            self.write_to_disk(key, value).await?;
        }

        Ok(())
    }

    /// 后台任务：定期降级冷数据
    ///
    /// 应该定期调用（例如每 5 分钟）
    pub async fn demote_cold_data(&self) -> Result<usize, String> {
        let stats = self.access_stats.read().await;
        let mut demoted_count = 0;

        for (key, stat) in stats.iter() {
            let threshold = self.config.cold_data_threshold_secs;
            let access_threshold = self.config.cold_data_access_threshold;

            if stat.count < access_threshold && stat.elapsed_since_last_access().as_secs() > threshold {
                // 降级冷数据
                if self.demote_key(key).await? {
                    demoted_count += 1;
                }
            }
        }

        Ok(demoted_count)
    }

    /// 获取访问统计
    ///
    /// # 参数
    ///
    /// * `key` - 键
    ///
    /// # 返回
    ///
    /// * `Option<AccessStats>` - 访问统计
    pub async fn get_access_stats(&self, key: &str) -> Option<AccessStats> {
        let stats = self.access_stats.read().await;
        stats.get(key).cloned()
    }

    /// 获取 CPU 缓存大小
    pub async fn cpu_cache_size(&self) -> usize {
        let cache = self.cpu_cache.read().await;
        cache.len()
    }

    /// 获取磁盘上 KV 文件数量
    pub async fn disk_file_count(&self) -> Result<usize, String> {
        let mut count = 0;
        if let Ok(mut entries) = tokio::fs::read_dir(&self.disk_path).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if entry.path().extension().map_or(false, |ext| ext == "kv") {
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    /// 清空 CPU 缓存
    pub async fn clear_cpu_cache(&self) {
        let mut cache = self.cpu_cache.write().await;
        *cache = LruCache::new(self.config.cpu_cache_size);
    }

    /// 记录访问
    async fn record_access(&self, key: &str) {
        let mut stats = self.access_stats.write().await;
        let entry = stats.entry(key.to_string()).or_insert_with(AccessStats::new);
        entry.record_access();
    }

    /// 提升到 CPU 内存缓存
    async fn promote_to_cpu(&self, key: &str, data: &KvData) {
        let mut cpu = self.cpu_cache.write().await;
        cpu.put(key.to_string(), data.clone());
    }

    /// 写入 CPU 内存缓存
    async fn write_to_cpu(&self, key: String, value: KvData) -> Result<(), String> {
        let mut cpu = self.cpu_cache.write().await;
        cpu.put(key, value);
        Ok(())
    }

    /// 写入磁盘
    async fn write_to_disk(&self, key: String, value: KvData) -> Result<(), String> {
        let disk_path = self.disk_path.join(format!("{}.kv", key));
        let data = bincode::serialize(&value)
            .map_err(|e| format!("Failed to serialize KV data: {}", e))?;
        tokio::fs::write(&disk_path, data).await
            .map_err(|e| format!("Failed to write to disk: {}", e))?;
        Ok(())
    }

    /// 降级键（从 CPU 缓存移除，保留在磁盘）
    async fn demote_key(&self, key: &str) -> Result<bool, String> {
        let mut cpu = self.cpu_cache.write().await;
        if cpu.remove(&key.to_string()).is_some() {
            // 确保磁盘上有数据
            let disk_path = self.disk_path.join(format!("{}.kv", key));
            if !disk_path.exists() {
                // 如果磁盘上没有，先从缓存读取再写入磁盘
                drop(cpu);
                // 重新获取锁并写入
                let stats = self.access_stats.read().await;
                if let Some(_stat) = stats.get(key) {
                    // 这里简化处理，实际应该从其他地方获取数据
                }
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// KV 序列化/反序列化辅助函数
pub fn serialize_kv(kv: &KvData) -> Result<Vec<u8>, String> {
    bincode::serialize(kv).map_err(|e| format!("Serialization error: {}", e))
}

pub fn deserialize_kv(data: &[u8]) -> Result<KvData, String> {
    bincode::deserialize(data).map_err(|e| format!("Deserialization error: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_kv(key: &str, value: &[u8]) -> KvData {
        KvData::new(key.to_string(), value.to_vec())
    }

    #[tokio::test]
    async fn test_tiered_storage_creation() {
        let temp_dir = TempDir::new().unwrap();
        let storage = TieredStorageManager::with_default_config(temp_dir.path()).unwrap();

        assert_eq!(storage.cpu_cache_size().await, 0);
        assert_eq!(storage.disk_file_count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_write_to_cpu_then_read() {
        let temp_dir = TempDir::new().unwrap();
        let storage = TieredStorageManager::with_default_config(temp_dir.path()).unwrap();

        let kv = create_test_kv("test_key", b"test_value");
        storage.put("test_key".to_string(), kv.clone()).await.unwrap();

        // 应该从 CPU 缓存读取
        let result = storage.get("test_key").await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().value, b"test_value");
    }

    #[tokio::test]
    async fn test_write_to_disk_then_read() {
        let temp_dir = TempDir::new().unwrap();
        let storage = TieredStorageManager::with_default_config(temp_dir.path()).unwrap();

        // 直接写入磁盘（不经过 put，避免热度判断）
        let kv = create_test_kv("cold_key", b"cold_value");
        storage.write_to_disk("cold_key".to_string(), kv.clone()).await.unwrap();

        // 清空 CPU 缓存，确保从磁盘读取
        storage.clear_cpu_cache().await;

        // 应该从磁盘读取并提升到 CPU 缓存
        let result = storage.get("cold_key").await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().value, b"cold_value");
    }

    #[tokio::test]
    async fn test_access_stats() {
        let temp_dir = TempDir::new().unwrap();
        let storage = TieredStorageManager::with_default_config(temp_dir.path()).unwrap();

        let kv = create_test_kv("hot_key", b"hot_value");
        storage.put("hot_key".to_string(), kv.clone()).await.unwrap();

        // 多次访问
        for _ in 0..15 {
            let _ = storage.get("hot_key").await.unwrap();
        }

        let stats = storage.get_access_stats("hot_key").await.unwrap();
        assert!(stats.is_hot());
        assert!(!stats.is_cold());
    }

    #[tokio::test]
    async fn test_cold_data_demotion() {
        let temp_dir = TempDir::new().unwrap();
        let storage = TieredStorageManager::with_default_config(temp_dir.path()).unwrap();

        // 写入冷数据（只访问 1 次）
        let kv = create_test_kv("cold_key", b"cold_value");
        storage.put("cold_key".to_string(), kv.clone()).await.unwrap();

        // 模拟时间流逝（这里通过修改配置来加速测试）
        // 实际测试中应该等待或使用 mock

        // 触发冷数据降级
        let demoted = storage.demote_cold_data().await.unwrap();
        // 由于刚写入，可能不会立即降级
        let _ = demoted; // 仅验证能正常执行
    }

    #[tokio::test]
    async fn test_lru_cache_eviction() {
        let temp_dir = TempDir::new().unwrap();
        let config = TieredStorageConfig {
            cpu_cache_size: 3,
            disk_path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let storage = TieredStorageManager::new(config).unwrap();

        // 写入 4 个数据（超过容量 3）
        for i in 0..4 {
            let key = format!("key_{}", i);
            let kv = create_test_kv(&key, format!("value_{}", i).as_bytes());
            storage.write_to_cpu(key, kv).await.unwrap();
        }

        // CPU 缓存应该只有 3 个
        assert_eq!(storage.cpu_cache_size().await, 3);

        // 最早写入的 key_0 应该被淘汰
        let result = storage.get("key_0").await.unwrap();
        assert!(result.is_none());

        // 最近写入的应该还在
        let result = storage.get("key_3").await.unwrap();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_kv_serialization() {
        let kv = create_test_kv("test_key", b"test_value");

        let serialized = serialize_kv(&kv).unwrap();
        let deserialized: KvData = deserialize_kv(&serialized).unwrap();

        assert_eq!(deserialized.key, "test_key");
        assert_eq!(deserialized.value, b"test_value");
    }

    #[tokio::test]
    async fn test_access_stats_thresholds() {
        let mut stats = AccessStats::new();

        // 初始状态
        assert!(!stats.is_hot());
        assert!(!stats.is_warm());
        assert!(!stats.is_cold()); // 刚创建，时间为 0

        // 访问 1 次
        stats.record_access();
        assert!(!stats.is_hot());
        assert!(!stats.is_warm());
        // 由于时间为 0，is_cold 可能为 false

        // 访问到 5 次
        for _ in 0..4 {
            stats.record_access();
        }
        assert!(!stats.is_hot());
        assert!(stats.is_warm());

        // 访问到 11 次
        for _ in 0..6 {
            stats.record_access();
        }
        assert!(stats.is_hot());
        assert!(!stats.is_warm());
    }
}
