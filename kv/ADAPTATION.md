# KV Cache 适配文档

## 概述

本文档说明如何将 `/kv` 目录下的 KV 缓存项目适配到区块链项目中。

## 目录结构

```
block_chain_with_context/
├── kv/                          # 复制过来的 KV 缓存项目
│   ├── Cargo.toml              # 已适配依赖版本
│   └── src/
│       ├── lib.rs              # 已更新文档注释
│       ├── kv_cache.rs         # 核心 KV 缓存管理
│       ├── kv_index.rs         # Bloom Filter 索引（已适配 bloomfilter crate）
│       ├── config.rs           # 配置管理
│       ├── error.rs            # 错误类型
│       ├── multi_level_cache.rs # 多级缓存（L1/L2/L3）
│       ├── kv_chunk.rs         # KV 分片
│       ├── kv_compression.rs   # INT8 量化/稀疏化压缩
│       ├── kv_compressor.rs    # zstd 压缩器
│       ├── prefetcher.rs       # 智能预取器
│       ├── context_sharding.rs # 上下文分片
│       └── ...
└── src/
    └── memory_layer.rs         # 已添加 kv-cache 集成说明
```

## 适配内容

### 1. Cargo.toml 依赖适配

**变更内容**：
- 将 `bloom` crate 改为 `bloomfilter`（与主项目一致）
- 将 `redis` 版本从 `0.24` 升级到 `0.27`
- 调整 `tokio` 特性以匹配主项目配置
- 移除 `testcontainers` 测试依赖

**原因**：确保与主项目依赖版本一致，避免冲突。

### 2. kv_index.rs Bloom Filter 适配

**原代码**（使用 `bloom` crate）：
```rust
use bloom::{BloomFilter, ASMS};

let bloom = BloomFilter::with_rate(fpr, n);
```

**适配后**（使用 `bloomfilter` crate）：
```rust
use bloomfilter::Bloom;

let bloom = Bloom::new(n, fpr);
```

### 3. lib.rs 文档更新

添加了区块链集成说明和使用示例，明确 kv-cache 在区块链项目中的定位。

### 4. 主项目 Cargo.toml 集成

添加 workspace 配置和 kv-cache 依赖：

```toml
[workspace]
members = [
    ".",
    "kv",
]

[dependencies]
kv-cache = { path = "./kv", features = ["compression", "tiered-storage"] }
```

### 5. memory_layer.rs 集成说明

在记忆层模块文档中添加了与 kv-cache 集成的说明：

- **KV 缓存加速**：使用 kv-cache 的多级缓存系统
- **智能预取**：基于 kv-cache 的预取器
- **压缩存储**：使用 kv-cache 的 zstd/INT8 量化压缩
- **上下文分片**：使用 kv-cache 的 context_sharding 模块

## 使用示例

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

### 与区块链记忆层集成

```rust
use kv_cache::multi_level_cache::{MultiLevelCacheManager, MultiLevelCacheConfig};
use std::path::PathBuf;

// 创建多级缓存配置
let config = MultiLevelCacheConfig {
    l1_cache_size: 5000,
    l2_disk_path: PathBuf::from("./data/kv_l2"),
    ..Default::default()
};

// 创建多级缓存管理器（用于加速记忆层访问）
let cache_manager = MultiLevelCacheManager::new(config).await.unwrap();
```

## 特性说明

### 默认特性

- `compression` - 启用 zstd 压缩支持
- `tiered-storage` - 启用多级存储（L1/L2/L3）

### 可选特性

- `redis-backend` - 启用 Redis 后端支持（L3 缓存）
- `metrics` - 启用 Prometheus 指标导出

## 模块说明

| 模块 | 功能 | 状态 |
|------|------|------|
| `kv_cache` | 核心 KV 缓存管理 | ✅ 已适配 |
| `kv_index` | Bloom Filter 索引 | ✅ 已适配 |
| `config` | 配置管理 | ✅ 已适配 |
| `error` | 错误类型 | ✅ 已适配 |
| `multi_level_cache` | 多级缓存 | ✅ 已适配 |
| `kv_chunk` | KV 分片 | ✅ 已适配 |
| `kv_compression` | INT8 量化/稀疏化 | ✅ 已适配 |
| `kv_compressor` | zstd 压缩器 | ✅ 已适配 |
| `prefetcher` | 智能预取器 | ✅ 已适配 |
| `context_sharding` | 上下文分片 | ✅ 已适配 |
| `async_storage` | 异步存储后端 | ✅ 已适配 |
| `redis_backend` | Redis 后端 | ⚠️ 需启用 redis-backend 特性 |

## 编译验证

```bash
# 编译 kv-cache crate
cd kv
cargo build

# 编译主项目（会自动包含 kv-cache）
cd ..
cargo build

# 运行测试
cargo test -p kv-cache
```

## 后续工作

1. **深度集成**：在记忆层管理器中直接使用 kv-cache 作为底层存储
2. **性能优化**：根据区块链场景调整缓存大小和预取策略
3. **监控指标**：启用 Prometheus 指标，集成到项目监控系统
4. **分布式协调**：增强跨节点 KV 缓存同步机制

## 注意事项

1. **保持独立性**：原 `/home/hao/kv` 项目保持不变，所有修改仅在复制过来的 `block_chain_with_context/kv` 目录中
2. **版本兼容**：确保 kv-cache 的依赖版本与主项目一致
3. **特性管理**：按需启用 kv-cache 的特性，避免不必要的依赖
