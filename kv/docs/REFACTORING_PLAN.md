# 重构实施计划

> **项目版本**：v0.6.0
> **开始日期**：2026-03-17
> **预计完成**：2026-04-14

---

## 一、重构目标

根据 P11 锐评，将项目从"区块链 KV 缓存"重构为"高性能分布式 KV 缓存系统，带可选审计日志插件"。

### 核心原则

1. **去噱头化**：移除所有"区块链"相关营销词汇
2. **插件化**：审计功能降级为可选插件
3. **依赖注入**：用 trait 抽象层解耦模块
4. **术语准确**：使用准确的技术术语

---

## 二、术语映射表

| 旧术语 | 新术语 | 英文 | 说明 |
|--------|--------|------|------|
| `Blockchain` | `AuditLog` | Audit Log | 审计日志，不是区块链 |
| `Block` | `AuditEntry` | Audit Entry | 审计条目，不是区块 |
| `ConsensusEngine` | `ResultArbiter` | Result Arbiter | 结果仲裁器，不是共识引擎 |
| `ReputationManager` | `NodeQualityTracker` | Node Quality Tracker | 节点质量追踪 |
| `Reputation` | `Quality` | Quality | 质量，不是信誉 |
| `共识` | `仲裁` | Arbitration | 准确描述功能 |
| `信誉系统` | `质量追踪` | Quality Tracking | 准确描述功能 |
| `双链架构` | `KV 存储 + 审计插件` | KV Store + Audit Plugin | 移除营销词汇 |
| `上链` | `存证` | Attestation | 准确描述功能 |
| `存证链` | `存证日志` | Attestation Log | 准确描述功能 |

---

## 三、文件重命名计划

### 3.1 核心模块

| 旧文件名 | 新文件名 | 说明 |
|----------|----------|------|
| `src/blockchain.rs` | `src/audit_log.rs` | 审计日志模块 |
| `src/block.rs` | `src/audit_entry.rs` | 审计条目模块 |
| `src/quality_assessment.rs` | `src/quality.rs` | 质量评估模块 |
| `src/memory_layer.rs` | `src/kv_cache.rs` | KV 缓存核心 |
| `src/concurrency.rs` | `src/concurrency.rs` | 保持不变 |
| `src/error.rs` | `src/error.rs` | 更新错误类型 |

### 3.2 子模块

| 旧目录 | 新目录 | 说明 |
|--------|--------|------|
| `memory_layer/` | `kv_cache/` | KV 缓存子模块 |

---

## 四、代码重构任务

### 任务 1：更新错误类型（P0）

**文件**：`src/error.rs`

**修改内容**：
```rust
// 旧
pub enum AppError {
    Blockchain { ... },
    Consensus { ... },
    Reputation { ... },
    ...
}

// 新
pub enum AppError {
    AuditLog { ... },
    ResultArbitration { ... },
    NodeQuality { ... },
    ...
}
```

**状态**：⏳ 待执行

---

### 任务 2：重命名 Blockchain 为 AuditLog（P0）

**文件**：`src/blockchain.rs` → `src/audit_log.rs`

**修改内容**：
```rust
// 旧
pub struct Blockchain {
    consensus_engine: ConsensusEngine,
    reputation_manager: ReputationManager,
    ...
}

// 新
pub struct AuditLog {
    arbiter: ResultArbiter,
    quality_tracker: NodeQualityTracker,
    ...
}
```

**状态**：⏳ 待执行

---

### 任务 3：重命名 ConsensusEngine 为 ResultArbiter（P0）

**文件**：`src/blockchain.rs` → `src/arbiter.rs`

**修改内容**：
```rust
// 旧
pub struct ConsensusEngine {
    threshold: f64,
    min_nodes: usize,
}

impl ConsensusEngine {
    pub fn vote(&self, ...) -> ConsensusDecision { ... }
}

// 新
pub struct ResultArbiter {
    threshold: f64,
    min_nodes: usize,
}

impl ResultArbiter {
    pub fn arbitrate(&self, ...) -> ArbitrationResult { ... }
}
```

**状态**：⏳ 待执行

---

### 任务 4：重命名 ReputationManager 为 NodeQualityTracker（P0）

**文件**：内联到 `src/audit_log.rs` 或独立为 `src/node_quality.rs`

**修改内容**：
```rust
// 旧
pub struct ReputationManager {
    nodes: HashMap<String, NodeReputation>,
    trust_threshold: f64,
}

// 新
pub struct NodeQualityTracker {
    nodes: HashMap<String, NodeQuality>,
    trust_threshold: f64,
}
```

**状态**：⏳ 待执行

---

### 任务 5：更新配置结构（P1）

**文件**：`src/blockchain.rs`（配置部分）

**修改内容**：
```rust
// 旧
pub struct BlockchainConfig {
    trust_threshold: f64,
    consensus: ConsensusConfig,
    ...
}

// 新
pub struct AuditConfig {
    trust_threshold: f64,
    arbitration: ArbitrationConfig,
    ...
}
```

**状态**：⏳ 待执行

---

### 任务 6：更新 memory_layer 模块（P1）

**文件**：`src/memory_layer.rs`

**修改内容**：
- 移除"记忆链"、"双链架构"等营销词汇
- 更新模块文档为"KV 缓存存储"
- 解耦对 `node_layer::AccessCredential` 的依赖

**状态**：⏳ 待执行

---

### 任务 7：实现依赖注入（P2）

**文件**：新建 `src/traits.rs`

**修改内容**：
```rust
/// 审计日志 trait
pub trait AuditLogger: Send + Sync {
    fn record_attestation(&self, data: &AttestationData) -> Result<()>;
    fn verify_attestation(&self, hash: &str) -> Result<bool>;
}

/// 结果仲裁 trait
pub trait ResultArbiter: Send + Sync {
    fn arbitrate(&self, results: &[NodeResult]) -> Result<ArbitrationResult>;
}

/// 节点质量存储 trait
pub trait NodeQualityStore: Send + Sync {
    fn get_quality(&self, node_id: &str) -> f64;
    fn update_quality(&self, node_id: &str, success: bool);
}

/// KV 存储 trait
pub trait KVStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;
    fn put(&self, key: &str, value: &[u8]) -> Result<()>;
}
```

**状态**：⏳ 待执行

---

### 任务 8：更新测试文件（P1）

**文件**：`tests/*.rs`

**修改内容**：
- 更新导入路径
- 更新术语使用
- 添加审计插件可选性测试

**状态**：⏳ 待执行

---

### 任务 9：更新文档（P1）

**文件**：`docs/*.md`

**修改内容**：
- ✅ `ARCHITECTURE.md` - 已更新
- ✅ `P11_REVIEW.md` - 已更新
- ⏳ `DEVELOPER_GUIDE.md` - 待更新
- ⏳ `README.md` - 待更新

**状态**：🔄 进行中

---

### 任务 10：添加 Feature Flag（P2）

**文件**：`Cargo.toml`（需要创建）

**修改内容**：
```toml
[features]
default = ["audit-plugin"]
audit-plugin = []
result-arbiter = []
```

**状态**：⏳ 待执行

---

## 五、执行顺序

### 第一阶段：正名（2026-03-17 ~ 2026-03-24）

1. ✅ 更新架构文档
2. ✅ 更新 P11_REVIEW.md
3. ⏳ 更新错误类型（任务 1）
4. ⏳ 重命名核心模块（任务 2-4）
5. ⏳ 更新配置结构（任务 5）
6. ⏳ 更新文档（任务 9）

### 第二阶段：架构重构（2026-03-25 ~ 2026-04-07）

1. ⏳ 实现依赖注入（任务 7）
2. ⏳ 更新 memory_layer 模块（任务 6）
3. ⏳ 更新测试文件（任务 8）
4. ⏳ 添加 Feature Flag（任务 10）

### 第三阶段：功能精简（2026-04-08 ~ 2026-04-14）

1. ⏳ 决定 LMCache 集成策略
2. ⏳ 实现更复杂的仲裁策略
3. ⏳ 节点质量持久化方案

---

## 六、风险评估

| 风险 | 影响 | 概率 | 缓解措施 |
|------|------|------|----------|
| 破坏性变更 | 高 | 高 | 保留旧 API 作为 deprecated |
| 测试失败 | 中 | 高 | 逐步重构，每步验证 |
| 依赖问题 | 中 | 中 | 先定义 trait，再重构实现 |
| 文档滞后 | 低 | 高 | 文档与代码同步更新 |

---

## 七、验收标准

### 代码层面

- [ ] 所有"区块链"相关术语已移除
- [ ] 审计功能为可选插件
- [ ] 核心 KV 缓存不依赖审计插件
- [ ] 所有测试通过
- [ ] 性能基准测试通过

### 文档层面

- [ ] 架构文档已更新
- [ ] API 文档已更新
- [ ] 开发者指南已更新
- [ ] README 已更新

### 功能层面

- [ ] KV 缓存核心功能正常
- [ ] 审计插件可选启用
- [ ] 结果仲裁功能正常
- [ ] 节点质量追踪正常

---

## 八、进度追踪

| 任务 | 计划完成 | 实际完成 | 状态 |
|------|----------|----------|------|
| 文档更新（架构） | 2026-03-17 | 2026-03-17 | ✅ 已完成 |
| 文档更新（P11） | 2026-03-17 | 2026-03-17 | ✅ 已完成 |
| 错误类型更新 | 2026-03-20 | - | ⏳ 待开始 |
| 模块重命名 | 2026-03-24 | - | ⏳ 待开始 |
| 依赖注入实现 | 2026-04-07 | - | ⏳ 待开始 |
| Feature Flag | 2026-04-14 | - | ⏳ 待开始 |

---

*创建日期：2026-03-17*
*最后更新：2026-03-17*
