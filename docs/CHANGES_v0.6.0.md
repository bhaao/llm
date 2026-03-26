# v0.6.0 修改记录

> **创建时间**: 2026-03-06
>
> **修改依据**: P11 锐评修复计划
>
> **修改者**: AI Assistant

---

## 📋 修改概览

本次修改依据业内大佬的 P11 锐评，**调整优先级**，先完成 P0 核心问题，再做 P1 集成：

### ✅ 已完成（v0.6.0-beta）

| 任务 | 优先级 | 状态 | 修改内容 |
|------|--------|------|----------|
| PBFT libp2p 集成 | P0-1 | ✅ 完成 | 完整 Swarm 事件循环、GossipSub 广播 |
| Gossip libp2p 集成 | P0-2 | ✅ 完成 | GossipSub 主题订阅、Anti-Sybil 机制 |
| 状态持久化 | P0-3 | ✅ 完成 | RocksDB 集成、PBFT/Gossip 状态持久化 |
| 监控指标集成 | P1-2 | ✅ 完成 | `/metrics` HTTP 端点、关键路径指标收集 |
| 异步 IO 彻底化 | P1-1 | ✅ 完成 | `storage.rs` 改用 `tokio::fs` |
| 清理文档 | P2-2 | ✅ 完成 | 标记 `建议.md` 为历史文档 |

### ⏳ 待完成（v0.7.0）

| 任务 | 优先级 | 状态 | 预计工作量 |
|------|--------|------|------------|
| Ring Attention 原型 | P2-1 | ⏳ v0.7.0 | 2 周 |

---

## 🔧 详细修改内容

### 1. P0-1: PBFT libp2p 集成

**修改文件**: `src/network/libp2p_network.rs`

**核心功能**:
- ✅ 完整的 Swarm 事件循环
- ✅ GossipSub 协议支持消息广播
- ✅ 视图切换超时重传
- ✅ Anti-Sybil 机制（节点身份验证）

**生产就绪度**:
- ✅ 3 节点真实网络 PBFT 共识跑通
- ✅ 单节点宕机后共识继续
- ✅ 视图切换成功处理 Leader 故障

**使用示例**:

```rust
use block_chain_with_context::network::libp2p_network::{Libp2pNetwork, Libp2pConfig};

// 创建配置
let config = Libp2pConfig::default();

// 启动网络
let network = Libp2pNetwork::new(config).await?;

// 订阅 PBFT 主题
network.subscribe("pbft_consensus").await?;

// 发布 PBFT 消息
let message = vec![/* PBFT 消息字节 */];
network.publish("pbft_consensus", message).await?;
```

**验收标准**:
- [x] 完整的 Swarm 事件循环实现
- [x] GossipSub 主题订阅和发布
- [x] mDNS 节点发现
- [x] Anti-Sybil 身份验证

---

### 2. P0-2: Gossip libp2p 集成

**修改文件**: `src/network/libp2p_network.rs`, `src/gossip.rs`

**核心功能**:
- ✅ GossipSub 主题订阅
- ✅ Vector Clock 同步
- ✅ Anti-Sybil 机制

**使用示例**:

```rust
use block_chain_with_context::network::libp2p_network::{Libp2pNetwork, Libp2pGossipNetwork};
use block_chain_with_context::gossip::{GossipMessage, KVShard};

// 创建 libp2p Gossip 网络
let network = Arc::new(Libp2pNetwork::new(config).await?);
let gossip_network = Libp2pGossipNetwork::new(network, "gossip_sync".to_string());

// 发送 Gossip 消息
let shard = KVShard::new("shard_1".to_string(), "node_1".to_string());
let message = GossipMessage {
    shard_id: "shard_1".to_string(),
    shard: shard.clone(),
    vector_clock: shard.version.clone(),
    merkle_root: shard.merkle_root.clone(),
    timestamp: current_timestamp(),
};

gossip_network.gossip(message).await?;
```

**验收标准**:
- [x] GossipSub 主题订阅
- [x] Vector Clock 数据同步
- [x] Anti-Sybil 节点身份验证

---

### 3. P0-3: 状态持久化

**新增文件**: `src/persistence.rs`

**修改文件**: `Cargo.toml`, `src/lib.rs`

**新增依赖**:
```toml
[dependencies]
rocksdb = { version = "0.22", optional = true }

[features]
persistence = ["rocksdb"]
```

**核心功能**:
- ✅ PBFT 状态持久化（视图号、序列号、消息日志）
- ✅ Gossip Vector Clock 持久化
- ✅ KV 分片数据持久化
- ✅ Checkpoint 持久化

**存储结构**:

```text
Column Families:
- pbft_state: PBFT 共识状态
  - key: "view" -> value: u64
  - key: "sequence" -> value: u64
  - key: "log:{digest}" -> value: MessageLog
- gossip_state: Gossip 同步状态
  - key: "vector_clock:{node_id}" -> value: VectorClock
  - key: "shard:{shard_id}" -> value: KVShard
- checkpoints: Checkpoint 数据
  - key: sequence -> value: Checkpoint
```

**使用示例**:

```rust
use block_chain_with_context::persistence::RocksDBStorage;

// 打开数据库
let db = RocksDBStorage::open("/path/to/db")?;

// 保存 PBFT 状态
db.save_pbft_view(42)?;
db.save_pbft_sequence(100)?;

// 加载 PBFT 状态
let view = db.load_pbft_view()?;
let sequence = db.load_pbft_sequence()?;

// 保存 Vector Clock
let mut clock = VectorClock::new();
clock.increment("node_1");
db.save_vector_clock("node_1", &clock)?;

// 批量保存
db.batch_save_pbft_state(view, sequence, &ConsensusState::Normal, 100)?;
```

**验收标准**:
- [x] RocksDB 集成
- [x] PBFT 状态持久化
- [x] Gossip Vector Clock 持久化
- [x] 批量写入支持
- [x] 节点重启后状态恢复

---

### 4. P1-2: 监控指标集成

**修改文件**: `src/metrics.rs`, `src/node_layer/rpc_server.rs`

**新增功能**:
- ✅ `/metrics` HTTP 端点集成到 `RpcServer`
- ✅ 在关键路径集成指标收集
- ✅ Prometheus 格式导出

**HTTP 端点**:

```bash
# 获取 Prometheus 指标
curl http://localhost:3000/metrics

# 输出示例：
# HELP inference_latency_seconds Inference request latency in seconds
# TYPE inference_latency_seconds histogram
# inference_latency_seconds_bucket{le="0.001"} 0
# inference_latency_seconds_bucket{le="0.01"} 5
# ...
```

**使用示例**:

```rust
use block_chain_with_context::node_layer::rpc_server::RpcServer;
use block_chain_with_context::metrics::MetricsRegistry;
use std::sync::Arc;

// 创建指标注册表
let registry = MetricsRegistry::new_arc();

// 创建带自定义指标的 RPC 服务器
let server = RpcServer::with_metrics(
    node_layer,
    memory_layer,
    blockchain,
    registry,
    "0.0.0.0:3000",
);

// 运行服务器（自动暴露 /metrics 端点）
server.run().await?;
```

**验收标准**:
- [x] `/metrics` HTTP 端点集成
- [x] Prometheus 格式导出
- [x] 动态指标更新（peer 数量、视图号）

---
