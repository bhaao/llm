//! KV Cache - 高性能分布式 KV 缓存系统
//!
//! 本库提供一个高性能、分布式的 KV 缓存系统，专为区块链项目中的 LLM 推理场景优化。
//!
//! # 核心特性
//!
//! - **KV 分段存储**：将超长上下文按固定大小分片存储
//! - **多级缓存**：L1（内存）+ L2（磁盘）+ L3（Redis）
//! - **智能预取**：基于 N-gram 模式的预取器
//! - **Bloom Filter 索引**：O(1) 时间复杂度的快速查找
//! - **zstd 压缩**：节省 93% 存储空间
//! - **异步支持**：完整的异步 API
//! - **区块链集成**：支持与区块链项目的记忆层集成
//!
//! # 架构
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                    KV Cache Manager                      │
//! ├─────────────────────────────────────────────────────────┤
//! │  L1 Cache (Memory)  │  L2 Cache (Disk)  │  L3 (Redis)   │
//! ├─────────────────────────────────────────────────────────┤
//! │          Bloom Filter + Hash Index                      │
//! ├─────────────────────────────────────────────────────────┤
//! │              Compression (zstd)                         │
//! └─────────────────────────────────────────────────────────┘
//! ```
//!
//! # 使用示例
//!
//! ## 基本使用
//!
//! ```rust,no_run
//! use kv_cache::{KvCacheManager, KvSegment};
//!
//! // 创建 KV 缓存管理器（DashMap 实现内部可变性，不需要 mut）
//! let manager = KvCacheManager::new();
//!
//! // 写入 KV 数据
//! manager.write_kv("key1".to_string(), b"value1".to_vec()).unwrap();
//!
//! // 读取 KV 数据
//! let value = manager.read_kv("key1");
//! assert_eq!(value, Some(b"value1".to_vec()));
//! ```
//!
//! ## 与区块链项目集成
//!
//! ```rust,no_run
//! use kv_cache::config::KvCacheConfig;
//!
//! // 创建配置
//! let config = KvCacheConfig::default();
//!
//! // 在区块链节点中初始化 KV 缓存
//! // 用于存储 LLM 推理的 KV 缓存，加速上下文复用
//! ```
//!
//! # Feature Flags
//!
//! - `compression` - 启用 zstd 压缩支持（默认启用）
//! - `tiered-storage` - 启用多级存储（L1/L2/L3）（默认启用）
//! - `redis-backend` - 启用 Redis 后端支持
//! - `metrics` - 启用 Prometheus 指标导出
//!
//! # 模块结构
//!
//! - [`kv_cache`] - 核心 KV 缓存管理器和数据结构
//! - [`error`] - 错误类型定义
//! - [`concurrency`] - 并发工具
//! - [`metrics`] - Prometheus 指标（可选）
//! - [`config`] - 配置管理
//! - [`multi_level_cache`] - 统一多级缓存管理（L1/L2/L3）（可选）
//! - [`kv_chunk`] - KV 分片存储（可选）
//! - [`kv_index`] - Bloom Filter 索引（可选）
//! - [`kv_compression`] - INT8 量化/稀疏化压缩（可选）
//! - [`kv_compressor`] - zstd 压缩器（可选）
//! - [`prefetcher`] - 智能预取器（可选）
//! - [`async_storage`] - 异步存储后端（可选）
//! - [`context_sharding`] - 上下文分片（可选）
//! - [`redis_backend`] - Redis 后端（可选）

pub mod error;
pub mod kv_cache;
pub mod concurrency;
pub mod metrics;
pub mod config;

// 重新导出常用类型
pub use error::{AppError, AppResult};
pub use kv_cache::{
    AccessCredential, AccessType,
    KvCacheManager, KvSegment, KvSegmentHeader, KvShard, KvIntegrityProof,
};
pub use config::{KvCacheConfig, MultiLevelCacheConfig, HotCacheConfig, BloomFilterConfig, PrefetcherConfig};

#[cfg(feature = "tiered-storage")]
pub mod multi_level_cache;

#[cfg(feature = "tiered-storage")]
pub mod kv_chunk;

#[cfg(feature = "tiered-storage")]
pub mod kv_index;

#[cfg(feature = "tiered-storage")]
pub mod kv_compression;

#[cfg(feature = "tiered-storage")]
pub mod kv_compressor;

#[cfg(feature = "tiered-storage")]
pub mod prefetcher;

#[cfg(feature = "tiered-storage")]
pub mod async_storage;

#[cfg(feature = "tiered-storage")]
pub mod context_sharding;

#[cfg(feature = "redis-backend")]
pub mod redis_backend;

/// 库版本
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
