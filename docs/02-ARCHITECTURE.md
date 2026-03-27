# 架构设计文档

> **版本**: 2.0  
> **最后更新**: 2026-03-26  
> **项目版本**: v0.5.0

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

## 3. 核心模块详解

### 3.1 服务层（services/）

**P11 锐评修复**: 原 `coordinator.rs` (1843 行上帝对象) 拆分为三个单一职责服务。

| 服务 | 职责 | 依赖 |
|------|------|------|
| `InferenceOrchestrator` | 推理编排：选择提供商、执行推理 | Node/Memory/Provider Layer |
| `CommitmentService` | 存证上链：KV 存证、交易记录 | Blockchain |
| `FailoverService` | 故障切换：健康监控、自动切换 | ProviderLayer + 断路器 |
| `QaaSService` | 质量验证：输出质量评估 | QualityAssessor |

**文件**: `src/services/mod.rs`

### 3.2 审计日志层（blockchain.rs）

**核心功能**:
- 区块定义（Block）：包含交易列表、前块哈希、时间戳
- 区块链管理（Blockchain）：添加区块、交易验证、共识引擎
- 配置管理（BlockchainConfig）：Builder 模式配置

**关键类型**:
```rust
pub struct Blockchain {
    chain: Vec<Block>,
    pending_transactions: Vec<Transaction>,
    nodes: HashMap<String, NodeInfo>,
    consensus_engine: ConsensusEngine,
}

pub enum ConsensusDecision {
    Unanimous { winner_id: String },
    Majority { winner_id: String, agreement_ratio: f64 },
    NoConsensus { requires_arbitration: bool },
}
```

**文件**: `src/blockchain.rs` (1222 行)

### 3.3 记忆层（memory_layer/）

**核心功能**:
- KV 存储管理（MemoryLayerManager）
- 分层存储（tiered_storage.rs）
- 多级缓存（multi_level_cache.rs）
- Chunk-level 存储（kv_chunk.rs）
- Bloom Filter 索引（kv_index.rs）
- 异步存储后端（async_storage.rs）
- zstd 压缩（kv_compressor.rs）
- 智能预取（prefetcher.rs）
- 上下文分片（context_sharding.rs）

**KV Cache 优化**:
| 优化维度 | 优化前 | 优化后 | 提升 |
|---------|--------|--------|------|
| 存储粒度 | Block-level | Chunk-level (256 tokens) | 细粒度 |
| 异步 IO | 同步 | 全异步 | 非阻塞 |
| 多级缓存 | 仅内存 | CPU + Disk + Remote | 分层 |
| 预取机制 | 无 | 智能预取 | 实现 |
| 压缩编码 | 无 | zstd 压缩 | 93% 空间节省 |
| 索引优化 | 无 | Bloom Filter | O(1) 查找 |

**文件**: `src/memory_layer.rs` (1157 行) + `src/memory_layer/*.rs` (~3000 行)

### 3.4 节点层（node_layer.rs）

**核心功能**:
- 节点管理（NodeLayerManager）
- 访问凭证（AccessCredential）
- 信誉系统（ReputationManager）
- RPC 服务器（rpc_server.rs）

**文件**: `src/node_layer.rs`

### 3.5 提供商层（provider_layer.rs）

**核心功能**:
- 推理提供商管理（ProviderLayerManager）
- 真实 LLM 集成（LLMProvider）
- HTTP 客户端（http_client.rs）
- 断路器模式（circuit_breaker.rs）

**文件**: `src/provider_layer.rs` + `src/failover/circuit_breaker.rs`

---

## 4. 共识与同步

### 4.1 PBFT 共识（原型）

**状态**: ⚠️ 原型（框架完整，使用内存消息传递）

**核心功能**:
- Pre-prepare → Prepare → Commit 三阶段提交
- 2f+1 签名收集，支持拜占庭容错
- 视图切换机制（leader 故障）
- Checkpoint 机制（日志垃圾回收）

**文件**: `src/consensus/pbft.rs`

### 4.2 Gossip 同步（原型）

**状态**: ⚠️ 原型（协议完整，使用内存模拟）

**核心功能**:
- Vector Clock：因果排序和冲突检测
- Merkle Tree：数据完整性验证
- Gossip push/pull 机制
- 冲突解决策略

**文件**: `src/gossip.rs`

---

## 5. 李群验证模块

### 5.1 四层架构映射

```
┌─────────────────────────────────────────────────────────┐
│  第一层：分布式上下文分片层 (不可信节点)                 │
│  • ContextShardManager • LieAlgebraMapper ← 新增        │
└─────────────────────────────────────────────────────────┘
                           ↓ 提交 A_i
┌─────────────────────────────────────────────────────────┐
│  第二层：李群链上聚合层 (系统核心，信任根)               │
│  • PBFTConsensus • LieGroupAggregator ← 信任根          │
└─────────────────────────────────────────────────────────┘
                           ↓ 生成 G
┌─────────────────────────────────────────────────────────┐
│  第三层：QaaS 质量验证层 (李群度量)                      │
│  • QaaSService • LieGroupMetric ← 新增                  │
└─────────────────────────────────────────────────────────┘
                           ↓ 输出 proof
┌─────────────────────────────────────────────────────────┐
│  第四层：区块链存证与激励层                              │
│  • Blockchain + KvCacheProof + ValidatorReputation      │
└─────────────────────────────────────────────────────────┘
```

### 5.2 核心创新：信任根上移

**旧架构**:
```
节点 → 哈希校验 → 上链存证 → 共识仲裁
↑ 信任根在节点（可能被攻破）
```

**新架构**:
```
节点 → 提交局部 A_i → 链上李群聚合 G → QaaS 验证
↑ 节点无法控制全局 G，信任根在聚合公式
```

### 5.3 性能基准（100 节点场景）

| 指标 | 生产要求 | 实测 | 评价 |
|------|----------|------|------|
| 聚合时间 | < 100ms | **53.19 µs** | ✅ 快 1880 倍 |
| 距离计算 | < 10ms | **137 ns** | ✅ 快 73000 倍 |
| 篡改检测 | ×5.47 | **∞** | ✅ 验证通过 |

**文件**: `src/lie_algebra/` (~600 行) + `benches/lie_group_bench.rs`

---

## 6. 锁顺序规范

为避免死锁，所有锁操作遵循以下顺序：

```
L1 缓存锁 → L2 磁盘锁 → L3 远程锁 → 审计日志锁 → 记忆层锁
```

违反顺序会在 debug 模式下触发警告。

---

## 7. 数据流

### 7.1 推理请求流程

```text
用户请求
    ↓
InferenceOrchestrator
    ↓
选择提供商 (FailoverService 监控健康)
    ↓
从记忆层读取 KV 上下文
    ↓
执行 LLM 推理 (vLLM/SGLang HTTP API)
    ↓
向记忆层写入新 KV
    ↓
计算 KV 哈希
    ↓
CommitmentService 提交存证到区块链
    ↓
返回响应给用户
```

### 7.2 共识流程

```text
节点提交李代数元素 A_i
    ↓
PBFT Pre-prepare：收集元素
    ↓
PBFT Prepare：验证元素有效性
    ↓
PBFT Commit：执行李群聚合 G = exp(1/N * Σlog(g_i))
    ↓
QaaS 验证：计算距离 d(G, G_true)
    ↓
区块链存证：KvCacheProof + LieGroupRoot
```

---

## 8. 监控与可观测性

### 8.1 核心指标

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

**文件**: `src/metrics.rs`

### 8.2 日志配置

```rust
use block_chain_with_context::{LogConfig, TimeoutConfig, RetryConfig};

let log_config = LogConfig {
    level: "info".to_string(),
    enable_file_logging: true,
    log_file_path: Some("/var/log/blockchain.log".to_string()),
    enable_rotation: true,
    rotation_days: 7,
};
```

---

## 9. 配置管理

### 9.1 配置文件示例

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

### 9.2 环境变量

```bash
# .env
PROVIDER_API_KEY=your_api_key
REDIS_URL=redis://localhost:6379
LOG_LEVEL=info
```

---

## 10. 部署架构

### 10.1 单节点部署

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

### 10.2 多节点部署（计划中）

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

## 11. 相关文档

- [快速开始指南](01-GETTING_STARTED.md)
- [开发者指南](03-DEVELOPER_GUIDE.md)
- [生产就绪度评估](04-PRODUCTION_READINESS.md)
- [P11 锐评与修复](05-P11_REVIEW_FIXES.md)
- [KV Cache 优化](06-KV_CACHE_OPTIMIZATION.md)
- [李群实现](07-LIE_GROUP_IMPLEMENTATION.md)

---

*最后更新：2026-03-26*  
*项目版本：v0.5.0*
