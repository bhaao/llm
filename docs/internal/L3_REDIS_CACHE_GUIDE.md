# L3 Redis 缓存集成指南

**版本**: v0.5.0  
**状态**: 生产就绪  
**最后更新**: 2026-03-05

---

## 📋 概述

多级缓存架构：

```
┌─────────────────────────────────────────────────────────┐
│  客户端请求                                              │
│         ↓                                               │
│  ┌─────────────────────────────────────────────────┐    │
│  │ L1: CPU 内存缓存 (LRU)                           │    │
│  │     - 容量：1000 条目                            │    │
│  │     - 延迟：< 1ms                               │    │
│  │     - 热度：访问次数 > 10                        │    │
│  └─────────────────────────────────────────────────┘    │
│         ↓ Miss                                          │
│  ┌─────────────────────────────────────────────────┐    │
│  │ L2: 磁盘存储 (SSD/HDD)                           │    │
│  │     - 容量：100GB+                              │    │
│  │     - 延迟：10-50ms                             │    │
│  │     - 热度：访问次数 4-10                        │    │
│  └─────────────────────────────────────────────────┘    │
│         ↓ Miss                                          │
│  ┌─────────────────────────────────────────────────┐    │
│  │ L3: 远程存储 (Redis)                             │    │
│  │     - 容量：TB+                                 │    │
│  │     - 延迟：100-500ms                           │    │
│  │     - 热度：访问次数 < 4                         │    │
│  └─────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────┘
```

---

## ✅ 实现状态

| 组件 | 文件 | 状态 | 说明 |
|------|------|------|------|
| **L1 CPU 缓存** | `multi_level_cache.rs` | ✅ 生产就绪 | LRU 策略，热点数据 |
| **L2 磁盘存储** | `async_storage.rs` | ✅ 生产就绪 | 持久化，温数据 |
| **L3 Redis** | `redis_backend.rs` | ✅ 生产就绪 | 远程存储，冷数据 |
| **自动升降级** | `multi_level_cache.rs` | ✅ 生产就绪 | 基于热度自动迁移 |

---

## 🚀 快速开始

### 1. 启用 remote-storage 特性

```bash
# Cargo.toml
[dependencies]
block_chain_with_context = { version = "0.5.0", features = ["remote-storage"] }
```

### 2. 启动 Redis 服务器

```bash
# Docker 方式
docker run -d -p 6379:6379 redis:latest

# 或者本地安装
redis-server
```

### 3. 创建多级缓存管理器

```rust
use block_chain_with_context::memory_layer::{
    MultiLevelCacheManager, MultiLevelCacheConfig,
    RemoteConfig, RemoteStorageType,
};

// 创建 L3 配置
let l3_config = RemoteConfig {
    storage_type: RemoteStorageType::Redis,
    endpoint: "redis://127.0.0.1:6379".to_string(),
    max_connections: 10,
    timeout_ms: 5000,
};

// 创建多级缓存配置
let config = MultiLevelCacheConfig {
    l1_cache_size: 1000,
    l2_disk_path: "/tmp/kv_cache".into(),
    l3_remote_config: Some(l3_config),
    ..Default::default()
};

// 创建缓存管理器
let cache = MultiLevelCacheManager::new(config).await?;
```

### 4. 使用缓存

```rust
use block_chain_with_context::memory_layer::MultiLevelKvData;

// 写入数据
let data = MultiLevelKvData::new("key1".to_string(), b"value1".to_vec());
cache.put(data).await?;

// 读取数据
let result = cache.get("key1").await?;
if let Some(data) = result {
    println!("Value: {:?}", data.value);
}

// 检查键是否存在
if cache.contains_key("key1").await {
    println!("Key exists");
}

// 删除数据
cache.delete("key1").await?;
```

---

## 📊 性能指标

### 延迟对比

| 操作 | L1 命中 | L2 命中 | L3 命中 |
|------|--------|--------|--------|
| **读取** | < 1ms | 10-50ms | 100-500ms |
| **写入** | < 1ms | 10-50ms | 100-500ms |
| **删除** | < 1ms | 10-50ms | 100-500ms |

### 命中率监控

```rust
let metrics = cache.get_metrics().await;

println!("L1 命中率：{:.2}%", metrics.l1_hit_rate * 100.0);
println!("L2 命中率：{:.2}%", metrics.l2_hit_rate * 100.0);
println!("L3 命中率：{:.2}%", metrics.l3_hit_rate * 100.0);
println!("总体命中率：{:.2}%", metrics.overall_hit_rate * 100.0);

println!("L1 条目数：{}", metrics.l1_entries);
println!("L2 条目数：{}", metrics.l2_entries);
println!("L3 条目数：{}", metrics.l3_entries);
```

---

## 🔧 配置选项

### MultiLevelCacheConfig

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `l1_cache_size` | `usize` | 1000 | L1 缓存最大条目数 |
| `l2_disk_path` | `PathBuf` | `./kv_cache` | L2 磁盘存储路径 |
| `l3_remote_config` | `Option<RemoteConfig>` | `None` | L3 远程存储配置 |
| `auto_tiering_enabled` | `bool` | `true` | 自动升降级开关 |
| `compression_enabled` | `bool` | `true` | 压缩开关 |

### RemoteConfig

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `storage_type` | `RemoteStorageType` | - | 存储类型（Redis/S3） |
| `endpoint` | `String` | - | 连接地址（Redis URL 或 S3 endpoint） |
| `max_connections` | `u32` | 10 | 最大连接数 |
| `timeout_ms` | `u64` | 5000 | 超时时间（毫秒） |

### RemoteStorageType

```rust
pub enum RemoteStorageType {
    Redis,      // Redis 缓存
    S3,         // S3 对象存储（计划中）
    Custom(String), // 自定义存储
}
```

---

## 🧪 测试

### 运行 Redis 集成测试

```bash
# 启动 Redis
docker run -d -p 6379:6379 redis:latest

# 运行测试
cargo test --package block_chain_with_context --lib memory_layer::redis_backend::tests --features remote-storage -- --nocapture
```

### 运行多级缓存测试

```bash
# 运行所有缓存测试
cargo test --package block_chain_with_context --lib memory_layer::multi_level_cache::tests --features tiered-storage -- --nocapture

# 运行并发测试
cargo test --package block_chain_with_context --lib memory_layer::multi_level_cache::tests::test_concurrent_access --features tiered-storage -- --nocapture
```

---

## 📈 自动升降级策略

### 热度判断

| 访问次数 | 最后访问时间 | 数据大小 | 存储层级 |
|---------|-------------|---------|---------|
| > 10    | 任意        | 任意    | L1 内存  |
| 4-10    | 任意        | 任意    | L2 磁盘  |
| < 4     | < 5 分钟     | < 1MB   | L2 磁盘  |
| < 4     | > 5 分钟     | 任意    | L3 远程  |
| 任意    | > 1 小时     | > 10MB  | L3 远程  |

### 自动升降级

```rust
// 启动后台自动升降级任务
let cache = Arc::new(cache);
cache.clone().start_auto_tiering_background_task();

// 后台任务每分钟检查一次，自动迁移冷热数据
```

---

## 🔍 故障排查

### Redis 连接失败

**错误**: `Failed to create Redis client`

**解决方案**:
1. 检查 Redis 服务器是否运行：`redis-cli ping`
2. 检查连接地址是否正确
3. 检查防火墙设置

### L3 命中率过低

**问题**: L3 命中率接近 0%

**解决方案**:
1. 增加 L1/L2 缓存容量
2. 检查数据访问模式，优化热点数据
3. 考虑调整升降级阈值

### 内存泄漏

**问题**: 内存使用持续增长

**解决方案**:
1. 检查 L1 缓存大小限制是否生效
2. 监控 `cache.get_metrics().await` 指标
3. 启用自动升降级任务

---

## 🎯 最佳实践

### 1. 合理设置 L1 缓存大小

```rust
// 根据可用内存设置
let l1_size = if has_lots_of_memory { 5000 } else { 1000 };
```

### 2. 启用压缩

```rust
let config = MultiLevelCacheConfig {
    compression_enabled: true,
    ..Default::default()
};
```

### 3. 监控指标

```rust
// 定期收集指标
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        let metrics = cache.get_metrics().await;
        info!("Cache metrics: {:?}", metrics);
    }
});
```

### 4. 预热缓存

```rust
// 启动时预加载热点数据
for key in &hot_keys {
    if let Some(data) = l3.get(key).await? {
        cache.put(data).await?;
    }
}
```

---

## 📚 参考资料

1. **Redis 官方文档**: https://redis.io/docs/
2. **redis-rs 客户端**: https://github.com/redis-rs/redis-rs
3. **多级缓存设计**: https://aws.amazon.com/caching/

---

## 🎯 下一步行动

### 短期（v0.5.0）

- [x] Redis 后端实现
- [x] 多级缓存集成
- [ ] 添加性能基准测试

### 中期（v0.6.0）

- [ ] S3 对象存储支持
- [ ] 分布式缓存一致性
- [ ] 缓存预热策略优化

### 长期（v1.0.0）

- [ ] 自适应升降级算法
- [ ] 缓存命中率预测
- [ ] 跨节点缓存共享

---

**文档状态**: ✅ 完成
*最后更新*: 2026-03-27
**维护者**: Block Chain with Context Team
