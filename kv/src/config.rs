//! 统一配置管理模块
//!
//! **功能**：
//! - 统一的 KvCacheConfig 配置结构体
//! - 支持从环境变量加载配置
//! - 配置校验功能

use std::path::PathBuf;
use serde::{Serialize, Deserialize};

/// 统一 KV 缓存配置
///
/// # 示例
///
/// ```rust
/// use kv_cache::config::{KvCacheConfig, MultiLevelCacheConfig};
/// use std::path::PathBuf;
///
/// // 创建默认配置
/// let config = KvCacheConfig::default();
///
/// // 自定义配置
/// let config = KvCacheConfig {
///     node_id: "my_node".to_string(),
///     multi_level_config: MultiLevelCacheConfig {
///         l1_cache_size: 5000,
///         l2_disk_path: PathBuf::from("/data/kv_l2"),
///         ..Default::default()
///     },
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone)]
pub struct KvCacheConfig {
    /// 节点 ID
    pub node_id: String,
    /// 多级缓存配置
    pub multi_level_config: MultiLevelCacheConfig,
    /// 热点缓存配置
    pub hot_cache_config: HotCacheConfig,
    /// Bloom Filter 配置
    pub bloom_filter_config: BloomFilterConfig,
    /// 预取器配置
    pub prefetcher_config: PrefetcherConfig,
}

/// 热点缓存配置
#[derive(Debug, Clone)]
pub struct HotCacheConfig {
    /// 热点阈值（访问次数超过此值进入 L1）
    pub hot_threshold: u32,
    /// 温数据阈值（访问次数在此范围内为温数据）
    pub warm_threshold_min: u32,
    /// 温数据阈值上限
    pub warm_threshold_max: u32,
    /// 热点缓存最大条目数
    pub max_entries: usize,
    /// 时间衰减因子（每小时衰减比例，0.9 表示每小时衰减 10%）
    pub decay_factor: f64,
}

/// Bloom Filter 配置
#[derive(Debug, Clone)]
pub struct BloomFilterConfig {
    /// 预期元素数量
    pub expected_items: u64,
    /// 假阳性率（0.01 表示 1%）
    pub false_positive_rate: f64,
    /// 扩容阈值（容量使用超过此比例时扩容）
    pub capacity_threshold: f64,
}

/// 预取器配置
#[derive(Debug, Clone)]
pub struct PrefetcherConfig {
    /// 最小 N-gram 大小
    pub min_ngram_size: usize,
    /// 最大 N-gram 大小
    pub max_ngram_size: usize,
    /// 时间衰减因子（每小时衰减比例）
    pub decay_factor: f64,
    /// 预取窗口大小（每次预取多少个数据）
    pub prefetch_window_size: usize,
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
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RemoteStorageType {
    /// Redis
    Redis,
    /// Amazon S3
    S3,
    /// 自定义 HTTP 存储
    CustomHttp,
}

impl Default for KvCacheConfig {
    fn default() -> Self {
        KvCacheConfig {
            node_id: "default_node".to_string(),
            multi_level_config: MultiLevelCacheConfig::default(),
            hot_cache_config: HotCacheConfig::default(),
            bloom_filter_config: BloomFilterConfig::default(),
            prefetcher_config: PrefetcherConfig::default(),
        }
    }
}

impl Default for HotCacheConfig {
    fn default() -> Self {
        HotCacheConfig {
            hot_threshold: 10,
            warm_threshold_min: 4,
            warm_threshold_max: 10,
            max_entries: 1000,
            decay_factor: 0.9, // 每小时衰减 10%
        }
    }
}

impl Default for BloomFilterConfig {
    fn default() -> Self {
        BloomFilterConfig {
            expected_items: 100_000,
            false_positive_rate: 0.01, // 1% 假阳性率
            capacity_threshold: 0.8,   // 80% 容量时扩容
        }
    }
}

impl Default for PrefetcherConfig {
    fn default() -> Self {
        PrefetcherConfig {
            min_ngram_size: 2,
            max_ngram_size: 8,
            decay_factor: 0.9, // 每小时衰减 10%
            prefetch_window_size: 5,
        }
    }
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

impl KvCacheConfig {
    /// 从环境变量加载配置
    ///
    /// 支持的环境变量：
    /// - `KV_NODE_ID`: 节点 ID
    /// - `KV_L1_CACHE_SIZE`: L1 缓存大小
    /// - `KV_L2_DISK_PATH`: L2 磁盘路径
    /// - `KV_REDIS_ENDPOINT`: Redis 连接地址
    ///
    /// # 示例
    ///
    /// ```bash
    /// export KV_NODE_ID=my_node
    /// export KV_L1_CACHE_SIZE=5000
    /// export KV_L2_DISK_PATH=/data/kv_l2
    /// cargo run
    /// ```
    pub fn from_env() -> Self {
        let mut config = KvCacheConfig::default();

        if let Ok(node_id) = std::env::var("KV_NODE_ID") {
            config.node_id = node_id;
        }

        if let Ok(l1_size) = std::env::var("KV_L1_CACHE_SIZE") {
            if let Ok(size) = l1_size.parse::<usize>() {
                config.multi_level_config.l1_cache_size = size;
            }
        }

        if let Ok(l2_path) = std::env::var("KV_L2_DISK_PATH") {
            config.multi_level_config.l2_disk_path = PathBuf::from(l2_path);
        }

        if let Ok(redis_endpoint) = std::env::var("KV_REDIS_ENDPOINT") {
            config.multi_level_config.l3_remote_config = Some(RemoteConfig {
                storage_type: RemoteStorageType::Redis,
                endpoint: redis_endpoint,
                auth_token: None,
                bucket_name: None,
                timeout_ms: 5000,
            });
        }

        config
    }

    /// 配置校验
    ///
    /// 检查配置是否有效，返回错误信息
    pub fn validate(&self) -> Result<(), String> {
        if self.node_id.is_empty() {
            return Err("Node ID cannot be empty".to_string());
        }

        if self.multi_level_config.l1_cache_size == 0 {
            return Err("L1 cache size must be greater than 0".to_string());
        }

        if self.hot_cache_config.hot_threshold == 0 {
            return Err("Hot threshold must be greater than 0".to_string());
        }

        if self.bloom_filter_config.false_positive_rate <= 0.0
            || self.bloom_filter_config.false_positive_rate >= 1.0 {
            return Err("False positive rate must be between 0 and 1".to_string());
        }

        if self.prefetcher_config.min_ngram_size == 0 {
            return Err("Min N-gram size must be greater than 0".to_string());
        }

        if self.prefetcher_config.min_ngram_size > self.prefetcher_config.max_ngram_size {
            return Err("Min N-gram size must be less than or equal to max N-gram size".to_string());
        }

        if self.hot_cache_config.decay_factor <= 0.0 || self.hot_cache_config.decay_factor > 1.0 {
            return Err("Decay factor must be between 0 and 1".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = KvCacheConfig::default();
        assert_eq!(config.node_id, "default_node");
        assert_eq!(config.multi_level_config.l1_cache_size, 1000);
        assert_eq!(config.hot_cache_config.hot_threshold, 10);
    }

    #[test]
    fn test_config_validation() {
        let config = KvCacheConfig::default();
        assert!(config.validate().is_ok());

        let mut invalid_config = KvCacheConfig::default();
        invalid_config.node_id = "".to_string();
        assert!(invalid_config.validate().is_err());

        let mut invalid_config = KvCacheConfig::default();
        invalid_config.multi_level_config.l1_cache_size = 0;
        assert!(invalid_config.validate().is_err());
    }
}
