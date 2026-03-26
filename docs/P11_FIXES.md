# P11 锐评修复计划 - v0.6.0

> **创建时间**: 2026-03-06
> 
> **目标版本**: v0.6.0
> 
> **综合评分目标**: 4.5/5 → 4.8/5

---

## 📋 P11 锐评核心问题回顾

### 🔴 P0 致命问题（完成度 40%）

| 问题 | 现状 | 目标 | 优先级 |
|------|------|------|--------|
| PBFT 共识是"内存玩具" | 消息传递用 `VecDeque`，不是真实网络 | 集成 libp2p GossipSub，实现真实 P2P 广播 | P0-1 |
| Gossip 同步是"单机模拟" | 节点间通信靠内存，无 Anti-Sybil 机制 | 集成 libp2p，添加节点身份认证 | P0-2 |
| 状态不持久化 | PBFT/Gossip 状态重启即丢失 | 持久化到 RocksDB/Redis | P0-3 |

### 🟠 P1 严重问题（完成度 70%）

| 问题 | 现状 | 目标 | 优先级 |
|------|------|------|--------|
| 异步 IO 不彻底 | `storage.rs` 使用 `std::fs` | 改用 `tokio::fs` | P1-1 ✅ |
| 监控指标缺失 | 无 Prometheus/Grafana | 添加关键指标导出 | P1-2 |

### 🟡 P2 一般问题

| 问题 | 现状 | 目标 | 优先级 |
|------|------|------|--------|
| Ring Attention 未实现 | 伪代码 | 2 节点原型 | P2-1 |
| 文档"假大空"痕迹 | `建议.md` 包含未兑现计划 | 清理/标记为历史文档 | P2-2 ✅ |

---

## 🎯 v0.6.0 修复计划

### P0-1: PBFT 共识 libp2p 集成

**当前状态**: ⚠️ 原型（内存模拟）

**目标状态**: ✅ 生产就绪（真实 P2P 网络）

**待完成任务**:

1. **完整的 Swarm 事件循环**
   - 实现 libp2p Swarm 的持续事件循环
   - 处理 GossipSub 消息订阅/发布
   - 处理节点连接/断开事件

2. **PBFT 消息广播**
   - Pre-prepare/Prepare/Commit 消息通过 GossipSub 广播
   - 消息签名验证（防止伪造）
   - 消息去重（防止重放攻击）

3. **视图切换超时重传**
   - 添加视图切换超时计时器
   - 超时自动触发 View Change
   - 支持 Leader 选举

**验收标准**:
- [ ] 3 节点真实网络 PBFT 共识跑通
- [ ] 单节点宕机后共识继续
- [ ] 视图切换成功处理 Leader 故障

**预计工作量**: 2 周

**参考实现**:
- [libp2p gossipsub 示例](https://github.com/libp2p/rust-libp2p/tree/master/examples/gossipsub)
- [tendermint-rs](https://github.com/penumbra-zone/tendermint-rs)

---

### P0-2: Gossip 同步 libp2p 集成

**当前状态**: ⚠️ 原型（内存模拟）

**目标状态**: ✅ 生产就绪（真实 P2P 网络）

**待完成任务**:

1. **GossipSub 主题订阅**
   - KV 分片同步主题：`/gossip/kv_shard/{shard_id}`
   - 节点状态主题：`/gossip/node/status`

2. **Vector Clock 同步**
   - 通过 GossipSub 广播 Vector Clock 更新
   - 冲突检测与解决

3. **Anti-Sybil 机制**
   - 节点身份认证（PeerId 验证）
   - 消息签名验证
   - 速率限制（防止 DoS）

**验收标准**:
- [ ] 3 节点真实网络 Gossip 同步跑通
- [ ] 网络分区后数据最终一致
- [ ] 恶意节点消息被拒绝

**预计工作量**: 1.5 周

---

### P0-3: 状态持久化

**当前状态**: ❌ 内存存储

**目标状态**: ✅ 持久化到 RocksDB/Redis

**待完成任务**:

1. **PBFT 状态持久化**
   - 视图号、序列号、消息日志持久化
   - 节点重启后恢复状态

2. **Gossip Vector Clock 持久化**
   - Vector Clock 持久化
   - 同步状态持久化

3. **RocksDB 集成**
   - 添加 `rocksdb` 依赖
   - 实现 `PersistentState` trait

**验收标准**:
- [ ] 节点重启后 PBFT 状态不丢
- [ ] 节点重启后 Gossip 同步继续
- [ ] 持久化延迟 < 10ms

**预计工作量**: 1 周

**依赖添加**:
```toml
[dependencies]
rocksdb = "0.22"
```

---

### P1-1: 异步 IO 彻底化

**当前状态**: ✅ 已完成（v0.6.0）

**完成内容**:
- `storage.rs` 改用 `tokio::fs`
- 所有文件 IO 操作异步化
- 测试代码更新为 `#[tokio::test]`

**验收标准**:
- [x] 编译通过
- [x] 测试通过
- [x] 无阻塞 IO 调用

---

### P1-2: 监控指标

**当前状态**: ❌ 缺失

**目标状态**: ✅ Prometheus 指标导出

**待完成任务**:

1. **Prometheus 指标定义**
   ```rust
   // 推理延迟
   inference_latency_seconds (Histogram)
   
   // KV 缓存命中率
   kv_cache_hit_ratio (Gauge)
   
   // PBFT 共识耗时
   pbft_consensus_duration_seconds (Histogram)
   
   // Gossip 同步延迟
   gossip_sync_duration_seconds (Histogram)
   
   // 节点信誉评分
   node_reputation_score (Gauge)
   ```

2. **指标收集点**
   - `InferenceOrchestrator`: 推理延迟
   - `MemoryLayerManager`: KV 缓存命中率
   - `PBFTConsensus`: 共识耗时
   - `GossipProtocol`: 同步延迟

3. **HTTP 指标端点**
   ```rust
   GET /metrics
   ```

4. **Grafana 仪表盘模板**
   - 导入 JSON 模板
   - 关键指标告警规则

**验收标准**:
- [ ] `/metrics` 端点返回 Prometheus 格式指标
- [ ] Grafana 仪表盘显示关键指标
- [ ] 告警规则配置完成

**预计工作量**: 1 周

**依赖添加**:
```toml
[dependencies]
prometheus = "0.13"
```

---

### P2-1: Ring Attention 原型

**当前状态**: ❌ 伪代码

**目标状态**: ⚠️ 原型（2 节点模拟）

**待完成任务**:

1. **定义 Ring Attention 接口**
   ```rust
   pub struct RingAttentionConfig {
       pub num_nodes: usize,
       pub num_layers: usize,
       pub hidden_size: usize,
   }
   
   pub trait RingAttention {
       async fn forward(&self, q: Tensor, k: Tensor, v: Tensor) -> Tensor;
   }
   ```

2. **单机模拟 2 节点**
   - 通过 Channel 模拟网络通信
   - 实现 Ring 交换 KV

3. **集成 gRPC/RPC**
   - 定义 Protobuf 接口
   - 实现远程 KV 获取

**验收标准**:
- [ ] 2 节点 Ring Attention 跑通
- [ ] 注意力输出形状正确
- [ ] 性能基准测试

**预计工作量**: 2 周（推迟到 v0.7.0）

---

### P2-2: 清理文档

**当前状态**: ✅ 已完成（v0.6.0）

**完成内容**:
- 更新 `README.md`，标记 `建议.md` 为历史文档
- 创建 `docs/P11_FIXES.md`（本文档）
- 更新开发路线图为 v0.6.0/v0.7.0 两阶段

---

## 📊 时间线

| 周次 | 任务 | 负责人 | 状态 |
|------|------|--------|------|
| Week 1 | P1-1: 异步 IO 彻底化 | Auto | ✅ 完成 |
| Week 1 | P2-2: 清理文档 | Auto | ✅ 完成 |
| Week 2-3 | P0-1: PBFT libp2p 集成 | - | ⏳ 待开始 |
| Week 4 | P0-2: Gossip libp2p 集成 | - | ⏳ 待开始 |
| Week 5 | P0-3: 状态持久化 | - | ⏳ 待开始 |
| Week 6 | P1-2: 监控指标 | - | ⏳ 待开始 |
| Week 7-8 | P2-1: Ring Attention 原型 | - | ⏳ 待开始（v0.7.0） |

---

## 🎯 验收标准汇总

### v0.6.0 核心验收

- [ ] **3 节点真实网络 PBFT 共识**
- [ ] **3 节点真实网络 Gossip 同步**
- [ ] **节点重启后状态恢复**
- [ ] **Prometheus 指标导出**
- [ ] **异步 IO 无阻塞**

### v0.7.0 核心验收

- [ ] **2 节点 Ring Attention 原型**
- [ ] **P2P KV 共享**

---

## 📝 备注

1. **优先级调整**: Ring Attention 从 v0.6.0 推迟到 v0.7.0，优先完成 P0/P1 问题
2. **依赖建议**: 评估 tendermint-rs 而非重复造轮子
3. **测试策略**: 添加 Docker Compose 多节点测试环境

---

*最后更新：2026-03-06*
*版本：v0.6.0-alpha*
