# 模块详解

> **阅读时间**: 30 分钟  
> **适用对象**: 开发者、高级用户

---

## 1. 模块总览

系统包含 5 个核心模块：

| 模块 | 代码位置 | 行数 | 状态 |
|------|----------|------|------|
| 服务层 | `src/services/` | ~800 | ✅ 生产就绪 |
| 审计日志层 | `src/blockchain.rs` | 1222 | ✅ 单节点生产就绪 |
| 记忆层 | `src/memory_layer.rs` + `src/memory_layer/` | ~4000 | ✅ 生产就绪 |
| 节点层 | `src/node_layer.rs` | ~600 | ✅ 生产就绪 |
| 提供商层 | `src/provider_layer.rs` | ~500 | ✅ 生产就绪 |

---

## 2. 服务层（services/）

**P11 锐评修复**: 原 `coordinator.rs` (1843 行上帝对象) 拆分为三个单一职责服务。

### 2.1 InferenceOrchestrator

**职责**: 推理编排

```rust
pub struct InferenceOrchestrator {
    node_layer: Arc<NodeLayerManager>,
    memory_layer: Arc<MemoryLayerManager>,
    provider_layer: Arc<ProviderLayerManager>,
}

impl InferenceOrchestrator {
    pub fn new(
        node_layer: Arc<NodeLayerManager>,
        memory_layer: Arc<MemoryLayerManager>,
        provider_layer: Arc<ProviderLayerManager>,
    ) -> Self;

    pub fn select_provider(&self) -> Result<String>;
    pub fn execute(&self, request: &InferenceRequest, credential: &AccessCredential) -> Result<InferenceResponse>;
}
```

**核心方法**:
- `select_provider()`: 选择最优提供商（基于信誉、健康度）
- `execute()`: 执行推理流程

### 2.2 CommitmentService

**职责**: 存证上链

```rust
pub struct CommitmentService {
    blockchain: Arc<RwLock<Blockchain>>,
    address: String,
}

impl CommitmentService {
    pub fn with_config(address: String, config: BlockchainConfig) -> Result<Self>;
    pub fn commit_inference(&self, metadata: InferenceMetadata, provider_id: &str, response: &InferenceResponse, kv_proofs: Vec<KvCacheProof>) -> Result<()>;
}
```

**核心方法**:
- `commit_inference()`: 提交推理存证到区块链

### 2.3 FailoverService

**职责**: 故障切换

```rust
pub struct FailoverService {
    provider_layer: Arc<ProviderLayerManager>,
    timeout_config: TimeoutConfig,
}

impl FailoverService {
    pub fn new(provider_layer: Arc<ProviderLayerManager>, timeout_config: TimeoutConfig) -> Self;
    pub fn execute_with_failover(&self, request: &InferenceRequest) -> Result<InferenceResponse>;
}
```

**核心方法**:
- `execute_with_failover()`: 带故障切换的执行
- 断路器模式：连续失败自动切换
- 指数退避：智能重试机制

### 2.4 QaaSService

**职责**: 质量验证

```rust
pub struct QaaSService {
    metric: LieGroupMetric,
}

impl QaaSService {
    pub fn new() -> Self;
    pub fn validate(&self, output: &InferenceResponse) -> Result<QualityScore>;
}
```

**核心方法**:
- `validate()`: 输出质量评估

---

## 3. 审计日志层（blockchain.rs）

### 3.1 核心结构

```rust
pub struct Blockchain {
    chain: Vec<Block>,
    pending_transactions: Vec<Transaction>,
    nodes: HashMap<String, NodeInfo>,
    consensus_engine: ConsensusEngine,
}

pub struct Block {
    index: u64,
    timestamp: u128,
    transactions: Vec<Transaction>,
    previous_hash: String,
    hash: String,
    nonce: u64,
}

pub struct Transaction {
    from: String,
    to: String,
    transaction_type: TransactionType,
    data: Option<Value>,
    timestamp: u128,
    signature: String,
}
```

### 3.2 核心功能

| 功能 | 方法 | 说明 |
|------|------|------|
| 添加区块 | `add_block()` | 创建新区块并添加到链 |
| 添加交易 | `add_transaction()` | 添加交易到待处理队列 |
| 注册节点 | `register_node()` | 注册新节点 |
| 共识决策 | `make_consensus_decision()` | 执行共识算法 |

### 3.3 共识引擎

```rust
pub enum ConsensusDecision {
    Unanimous { winner_id: String },
    Majority { winner_id: String, agreement_ratio: f64 },
    NoConsensus { requires_arbitration: bool },
}
```

---

## 4. 记忆层（memory_layer/）

### 4.1 核心模块

| 模块 | 文件 | 说明 |
|------|------|------|
| KV 存储管理 | `memory_layer.rs` | MemoryLayerManager 主模块 |
| 分层存储 | `tiered_storage.rs` | L1/L2/L3 多级存储 |
| 多级缓存 | `multi_level_cache.rs` | 缓存管理 |
| Chunk 存储 | `kv_chunk.rs` | 256 tokens/chunk |
| Bloom Filter | `kv_index.rs` | O(1) 查找索引 |
| 异步存储 | `async_storage.rs` | 异步 IO |
| zstd 压缩 | `kv_compressor.rs` | 93% 空间节省 |
| 智能预取 | `prefetcher.rs` | N-gram 预取 |
| 上下文分片 | `context_sharding.rs` | 100K+ tokens 跨节点 |

### 4.2 MemoryLayerManager

```rust
pub struct MemoryLayerManager {
    node_id: String,
    shards: HashMap<String, MemoryShard>,
    tiered_storage: TieredStorage,
    index: BloomFilterIndex,
}

impl MemoryLayerManager {
    pub fn new(node_id: &str) -> Self;
    pub fn write_kv(&mut self, key: String, value: Vec<u8>, credential: &AccessCredential) -> Result<()>;
    pub fn read_kv(&self, key: &str, credential: &AccessCredential) -> Option<MemoryShard>;
    pub fn delete_kv(&mut self, key: &str, credential: &AccessCredential) -> Result<()>;
}
```

### 4.3 KV Cache 优化

| 优化维度 | 优化前 | 优化后 | 提升 |
|---------|--------|--------|------|
| 存储粒度 | Block-level | Chunk-level (256 tokens) | 细粒度 |
| 异步 IO | 同步 | 全异步 | 非阻塞 |
| 多级缓存 | 仅内存 | CPU + Disk + Remote | 分层 |
| 预取机制 | 无 | 智能预取 | 实现 |
| 压缩编码 | 无 | zstd 压缩 | 93% 空间节省 |
| 索引优化 | 无 | Bloom Filter | O(1) 查找 |

---

## 5. 节点层（node_layer.rs）

### 5.1 核心结构

```rust
pub struct NodeLayerManager {
    node_id: String,
    address: String,
    reputation_manager: ReputationManager,
    credentials: HashMap<String, AccessCredential>,
}

pub struct AccessCredential {
    credential_id: String,
    provider_id: String,
    memory_block_ids: Vec<String>,
    access_type: AccessType,
    expires_at: u64,
    issuer_node_id: String,
    signature: String,
    is_revoked: bool,
}

pub enum AccessType {
    ReadOnly,
    ReadWrite,
}
```

### 5.2 核心功能

| 功能 | 方法 | 说明 |
|------|------|------|
| 节点管理 | `NodeLayerManager::new()` | 创建节点管理器 |
| 凭证管理 | `create_credential()` | 创建访问凭证 |
| 信誉管理 | `ReputationManager` | 节点信誉评分 |
| RPC 服务 | `RpcServer` | HTTP/gRPC 服务 |

---

## 6. 提供商层（provider_layer.rs）

### 6.1 核心结构

```rust
pub struct ProviderLayerManager {
    providers: HashMap<String, Box<dyn InferenceProvider>>,
    current_provider: Option<String>,
}

pub struct LLMProvider {
    provider_id: String,
    engine_type: InferenceEngineType,
    endpoint: String,
    capacity: u64,  // token/s
}

pub enum InferenceEngineType {
    Vllm,
    Sglang,
    Mock,
}
```

### 6.2 核心功能

| 功能 | 方法 | 说明 |
|------|------|------|
| 提供商管理 | `register_provider()` | 注册提供商 |
| 推理执行 | `execute_inference()` | 执行 LLM 推理 |
| 健康检查 | `check_health()` | 提供商健康监控 |
| 断路器 | `CircuitBreaker` | 故障切换保护 |

### 6.3 断路器模式

```rust
pub struct CircuitBreaker {
    failure_count: u32,
    last_failure_time: Option<Instant>,
    state: CircuitState,
}

pub enum CircuitState {
    Closed,     // 正常
    Open,       // 熔断
    HalfOpen,   // 半开（试探）
}
```

---

## 7. 李群验证模块

### 7.1 四层架构映射

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

### 7.2 核心创新：信任根上移

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

### 7.3 性能基准（100 节点场景）

| 指标 | 生产要求 | 实测 | 评价 |
|------|----------|------|------|
| 聚合时间 | < 100ms | **53.19 µs** | ✅ 快 1880 倍 |
| 距离计算 | < 10ms | **137 ns** | ✅ 快 73000 倍 |
| 篡改检测 | ×5.47 | **∞** | ✅ 验证通过 |

---

## 8. 相关文档

- [整体架构](01-overview.md) - 三层架构、双链设计
- [数据流](03-dataflow.md) - 推理流程、共识流程
- [李群验证](04-lie-group.md) - 信任根上移、四层架构

---

*最后更新：2026-03-26*  
*项目版本：v0.5.0*
