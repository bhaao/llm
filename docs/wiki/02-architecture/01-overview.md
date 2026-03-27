# 整体架构

> **阅读时间**: 20 分钟  
> **适用对象**: 架构师、开发者

---

## 1. 架构概览

### 1.1 三层架构

系统采用三层解耦架构，各层职责清晰：

```
┌─────────────────────────────────────────────────────────────┐
│                    推理提供商层 (Provider Layer)             │
│  • 从记忆层读取 KV/上下文                                    │
│  • 执行 LLM 推理计算（vLLM/SGLang API）                      │
│  • 向记忆层写入新生成的 KV                                   │
│  • 向审计日志层上报推理指标                                  │
└─────────────────────────────────────────────────────────────┘
                              ↑ ↓ 读取/写入 KV
┌─────────────────────────────────────────────────────────────┐
│                    记忆层 (Memory Layer)                     │
│  • KV Cache 存储（分片、分层、压缩）                         │
│  • 哈希链式校验（防篡改）                                    │
│  • 分布式多副本存储（容灾）                                  │
│  • 版本控制/访问授权                                         │
└─────────────────────────────────────────────────────────────┘
                              ↑ ↓ 哈希存证
┌─────────────────────────────────────────────────────────────┐
│                    审计日志层 (Audit Layer)                  │
│  • KV 哈希存证（不可篡改）                                   │
│  • 节点信誉管理                                              │
│  • 共识结果记录                                              │
└─────────────────────────────────────────────────────────────┘
```

### 1.2 模块依赖关系

```
services/ (应用层，生产就绪)
    ↓
blockchain.rs (库层，单节点生产就绪)
    ↓
memory_layer.rs (库层，生产就绪)
    ↓
node_layer.rs (库层，生产就绪)
```

### 1.3 依赖约束

```text
推理提供商 → 依赖 → 记忆层（读取/写入 KV）
推理提供商 → 依赖 → 审计日志层（上报指标）
记忆层   → 依赖 → 审计日志层（哈希存证）
审计日志层 → 不依赖 → 推理提供商/记忆层
```

---

## 2. 双链架构

### 2.1 区块链（Blockchain）- 主链

| 属性 | 说明 |
|------|------|
| **定位** | 全局可信存证链，所有节点共享 |
| **存储内容** | 元数据、KV 哈希存证、信誉记录、共识结果 |
| **特点** | 不可篡改、异步提交、全网共识 |
| **核心模块** | `src/blockchain.rs` (1222 行) |

### 2.2 记忆链（MemoryChain）- 数据链

| 属性 | 说明 |
|------|------|
| **定位** | 分布式 KV 上下文存储，按节点分片 |
| **存储内容** | 实际 KV 数据、哈希链式串联、版本控制 |
| **特点** | 多副本容灾、本地缓存、仅哈希上链 |
| **核心模块** | `src/memory_layer.rs` (1157 行) |

### 2.3 两条链的关系

```text
推理流程：
1. 推理提供商 → 从记忆链读取 KV 上下文
2. 推理提供商 → 执行 LLM 推理
3. 推理提供商 → 向记忆链写入新 KV
4. 记忆层 → 计算新 KV 哈希
5. 协调器 → 将 KV 哈希作为存证提交到区块链
6. 区块链 → 验证并记录存证（异步）

验证流程：
1. 验证方 → 从区块链读取 KV 哈希存证
2. 验证方 → 从记忆链读取实际 KV 数据
3. 验证方 → 计算 KV 哈希并与链上存证比对
4. 验证方 → 确认数据完整性
```

---

## 3. 设计原则

### 3.1 单一职责

每个模块只负责一个明确的功能领域：

| 模块 | 职责 |
|------|------|
| `InferenceOrchestrator` | 推理编排 |
| `CommitmentService` | 存证上链 |
| `FailoverService` | 故障切换 |
| `MemoryLayerManager` | KV 存储管理 |
| `Blockchain` | 审计日志管理 |

### 3.2 异步优先

全链路采用 async/await 模式：

```rust
use tokio::sync::RwLock;
use std::sync::Arc;

// 异步读写锁
let lock = Arc::new(RwLock::new(data));

// 异步读取
let data = lock.read().await;

// 异步写入
let mut data = lock.write().await;
```

### 3.3 错误处理

使用 anyhow + thiserror 组合：

```rust
use anyhow::Result;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("KV not found: {0}")]
    NotFound(String),
    #[error("Access denied: {0}")]
    AccessDenied(String),
}

pub async fn read_kv(key: &str) -> Result<Vec<u8>, MemoryError> {
    // ...
}
```

---

## 4. 锁顺序规范

为避免死锁，所有锁操作遵循以下顺序：

```
L1 缓存锁 → L2 磁盘锁 → L3 远程锁 → 审计日志锁 → 记忆层锁
```

违反顺序会在 debug 模式下触发警告。

### 4.1 锁顺序示例

```rust
// 正确：按顺序获取锁
async fn correct_order() {
    let l1 = l1_lock.read().await;
    let l2 = l2_lock.read().await;
    let l3 = l3_lock.read().await;
    // ...
}

// 错误：可能死锁
async fn wrong_order() {
    let l2 = l2_lock.read().await;
    let l1 = l1_lock.read().await;  // 警告！
    // ...
}
```

---

## 5. 配置管理

### 5.1 Builder 模式

```rust
use block_chain_with_context::BlockchainConfig;

let config = BlockchainConfig::builder()
    .trust_threshold(0.75)
    .inference_timeout_ms(30000)
    .commit_timeout_ms(10000)
    .max_retries(5)
    .log_level("info")
    .build()
    .expect("配置验证失败");
```

### 5.2 配置文件

```toml
# config.toml

[node]
node_id = "node_1"
address = "0.0.0.0:3000"

[blockchain]
trust_threshold = 0.67
inference_timeout_ms = 30000
commit_timeout_ms = 10000
max_retries = 5

[lie_group]
enabled = true
mapper_strategy = "exponential"
aggregator_formula = "geometric_mean"
distance_threshold = 0.5

[cache]
l1_capacity = 1000
l2_path = "./data/l2_cache"
l3_redis_url = "redis://localhost:6379"
```

---

## 6. 监控与可观测性

### 6.1 核心指标

| 指标 | 说明 |
|------|------|
| `kv_cache_hit_rate` | KV 缓存命中率 |
| `kv_cache_l1_hit_rate` | L1 缓存命中率 |
| `kv_cache_l2_hit_rate` | L2 缓存命中率 |
| `kv_cache_l3_hit_rate` | L3 缓存命中率 |
| `inference_latency_ms` | 推理延迟 |
| `consensus_duration_ms` | 共识耗时 |
| `active_nodes` | 活跃节点数 |
| `provider_health_score` | 提供商健康分 |

### 6.2 日志配置

```rust
use block_chain_with_context::LogConfig;

let log_config = LogConfig {
    level: "info".to_string(),
    enable_file_logging: true,
    log_file_path: Some("/var/log/blockchain.log".to_string()),
    enable_rotation: true,
    rotation_days: 7,
};
```

---

## 7. 部署架构

### 7.1 单节点部署

```text
┌─────────────────────────────────────┐
│         单节点部署                   │
│  ┌─────────────────────────────┐    │
│  │  Provider Layer             │    │
│  │  + Memory Layer             │    │
│  │  + Audit Layer              │    │
│  └─────────────────────────────┘    │
└─────────────────────────────────────┘
```

**适用场景**: 开发测试、原型验证

### 7.2 多节点部署（计划中）

```text
┌─────────────────────────────────────────────────────────┐
│                    多节点部署                            │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐              │
│  │ Node 1   │  │ Node 2   │  │ Node 3   │              │
│  │ + Memory │  │ + Memory │  │ + Memory │              │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘              │
│       │             │             │                     │
│       └─────────────┴─────────────┘                     │
│                    Gossip Sync                          │
│       ┌─────────────────────────────┐                   │
│       │      PBFT Consensus         │                   │
│       └─────────────────────────────┘                   │
└─────────────────────────────────────────────────────────┘
```

**适用场景**: 生产环境（待 v0.6.0 完成）

---

## 8. 相关文档

- [模块详解](02-modules.md) - 5 个核心模块详解
- [数据流](03-dataflow.md) - 推理流程、共识流程
- [李群验证](04-lie-group.md) - 信任根上移、四层架构

---

*最后更新：2026-03-26*  
*项目版本：v0.5.0*
