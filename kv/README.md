# KV Cache for Blockchain Project

高性能分布式 KV 缓存系统，为区块链项目中的 LLM 推理场景优化。

## 快速开始

### 添加到项目

在 `Cargo.toml` 中添加：

```toml
[dependencies]
kv-cache = { path = "./kv" }
```

### 基本使用

```rust
use kv_cache::{KvCacheManager, KvSegment};

// 创建 KV 缓存管理器
let manager = KvCacheManager::new();

// 写入 KV 数据
manager.write_kv("key1".to_string(), b"value1".to_vec()).unwrap();

// 读取 KV 数据
let value = manager.read_kv("key1");
assert_eq!(value, Some(b"value1".to_vec()));
```

### 配置化使用

```rust
use kv_cache::config::KvCacheConfig;

// 创建配置
let config = KvCacheConfig::default();

// 使用配置创建管理器
let manager = KvCacheManager::with_config(config);
```

### 多级缓存使用

```rust
use kv_cache::multi_level_cache::{MultiLevelCacheManager, MultiLevelCacheConfig};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 创建多级缓存配置
    let config = MultiLevelCacheConfig {
        l1_cache_size: 5000,
        l2_disk_path: PathBuf::from("./data/kv_l2"),
        ..Default::default()
    };

    // 创建多级缓存管理器
    let cache_manager = MultiLevelCacheManager::new(config).await?;

    // 使用缓存
    // ...

    Ok(())
}
```

## 核心特性

- **KV 分段存储**：将超长上下文按固定大小分片存储
- **多级缓存**：L1（内存）+ L2（磁盘）+ L3（Redis）
- **智能预取**：基于 N-gram 模式的预取器
- **Bloom Filter 索引**：O(1) 时间复杂度的快速查找
- **zstd 压缩**：节省 93% 存储空间
- **异步支持**：完整的异步 API
- **区块链集成**：支持与区块链项目的记忆层集成

## 架构

```
┌─────────────────────────────────────────────────────────┐
│                    KV Cache Manager                      │
├─────────────────────────────────────────────────────────┤
│  L1 Cache (Memory)  │  L2 Cache (Disk)  │  L3 (Redis)   │
├─────────────────────────────────────────────────────────┤
│          Bloom Filter + Hash Index                      │
├─────────────────────────────────────────────────────────┤
│              Compression (zstd)                         │
└─────────────────────────────────────────────────────────┘
```

## 特性标志

| 特性 | 说明 | 默认 |
|------|------|------|
| `compression` | 启用 zstd 压缩支持 | ✅ |
| `tiered-storage` | 启用多级存储（L1/L2/L3） | ✅ |
| `redis-backend` | 启用 Redis 后端支持 | ❌ |
| `metrics` | 启用 Prometheus 指标导出 | ❌ |

## 模块说明

| 模块 | 功能 |
|------|------|
| `kv_cache` | 核心 KV 缓存管理 |
| `kv_index` | Bloom Filter 索引 |
| `config` | 配置管理 |
| `error` | 错误类型定义 |
| `multi_level_cache` | 多级缓存管理 |
| `kv_chunk` | KV 分片存储 |
| `kv_compression` | INT8 量化/稀疏化压缩 |
| `kv_compressor` | zstd 压缩器 |
| `prefetcher` | 智能预取器 |
| `context_sharding` | 上下文分片 |
| `async_storage` | 异步存储后端 |
| `redis_backend` | Redis 后端（需启用 `redis-backend` 特性） |

## 编译和测试

```bash
# 编译
cd kv
cargo build

# 运行测试
cargo test

# 运行基准测试
cargo bench
```

## 与区块链项目集成

在区块链项目的记忆层中使用 kv-cache：

```rust
use kv_cache::KvCacheManager;
use block_chain_with_context::memory_layer::MemoryLayerManager;

// 创建 KV 缓存管理器
let kv_cache = KvCacheManager::new();

// 创建记忆层管理器
let mut memory_layer = MemoryLayerManager::new("node_1");

// KV 缓存加速记忆层访问
// 1. 先检查 KV 缓存
if let Some(cached) = kv_cache.read_kv("context_key") {
    // 使用缓存的数据
} else {
    // 从记忆层读取并缓存
    let data = memory_layer.read_kv("context_key", &credential);
    kv_cache.write_kv("context_key".to_string(), data.value);
}
```

## 适配说明

详细适配文档请参阅 [ADAPTATION.md](ADAPTATION.md)。

## 许可证

MIT License
