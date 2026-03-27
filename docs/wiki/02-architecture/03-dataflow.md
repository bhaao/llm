# 数据流

> **阅读时间**: 15 分钟  
> **适用对象**: 开发者、架构师

---

## 1. 推理请求流程

### 1.1 完整流程

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

### 1.2 时序图

```
用户          Orchestrator    MemoryLayer    Provider    Blockchain
 │                │               │             │            │
 │──请求────────→│               │             │            │
 │                │               │             │            │
 │                │──选择提供商───│             │            │
 │                │               │             │            │
 │                │──读取 KV────→│             │            │
 │                │               │             │            │
 │                │←─返回 KV─────│             │            │
 │                │               │             │            │
 │                │───────────────│──推理请求──→│            │
 │                │               │             │            │
 │                │←──────────────│──响应───────│            │
 │                │               │             │            │
 │                │──写入 KV────→│             │            │
 │                │               │             │            │
 │                │──KV 哈希─────→│             │            │
 │                │               │             │            │
 │                │─────────────────────────────│──存证─────→│
 │                │               │             │            │
 │←─响应──────────│               │             │            │
 │                │               │             │            │
```

### 1.3 代码示例

```rust
use block_chain_with_context::services::InferenceOrchestrator;

async fn execute_inference(
    orchestrator: &InferenceOrchestrator,
    request: &InferenceRequest,
    credential: &AccessCredential,
) -> Result<InferenceResponse> {
    // 1. 选择提供商
    let provider_id = orchestrator.select_provider()?;

    // 2. 从记忆层读取 KV 上下文
    let context = orchestrator
        .memory_layer
        .read_kv(&request.context_key, credential);

    // 3. 执行 LLM 推理
    let response = orchestrator
        .provider_layer
        .execute_inference(&request, context)
        .await?;

    // 4. 向记忆层写入新 KV
    orchestrator
        .memory_layer
        .write_kv(response.key.clone(), response.value.clone(), credential)?;

    // 5. 提交存证到区块链
    orchestrator
        .commitment_service
        .commit_inference(metadata, &provider_id, &response, kv_proofs)?;

    Ok(response)
}
```

---

## 2. 共识流程

### 2.1 PBFT 共识流程

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

### 2.2 时序图

```
Node 1        Node 2        Node 3        Leader      Blockchain
  │             │             │             │             │
  │──Pre-prepare────────────→│             │             │
  │             │             │             │             │
  │←────────────Prepare──────│             │             │
  │             │             │             │             │
  │─────────────Prepare──────→│             │             │
  │             │             │             │             │
  │←────────────Commit────────│             │             │
  │             │             │             │             │
  │─────────────Commit────────│             │             │
  │             │             │             │             │
  │             │             │──聚合 G────→│             │
  │             │             │             │             │
  │             │             │──QaaS 验证─→│             │
  │             │             │             │             │
  │             │             │─────────────│──存证──────→│
  │             │             │             │             │
```

### 2.3 代码示例

```rust
use block_chain_with_context::consensus::PBFTConsensus;

async fn execute_consensus(
    consensus: &mut PBFTConsensus,
    elements: Vec<LieAlgebraElement>,
) -> Result<LieGroupElement> {
    // 1. Pre-prepare 阶段
    consensus.pre_prepare(elements.clone()).await?;

    // 2. Prepare 阶段
    let prepare_votes = consensus.prepare().await?;
    assert!(prepare_votes.len() >= 2 * consensus.f + 1);

    // 3. Commit 阶段
    let commit_votes = consensus.commit().await?;
    assert!(commit_votes.len() >= 2 * consensus.f + 1);

    // 4. 执行李群聚合
    let aggregated = consensus.aggregate(elements)?;

    // 5. QaaS 验证
    let distance = consensus.validate(&aggregated)?;
    assert!(distance < consensus.distance_threshold);

    Ok(aggregated)
}
```

---

## 3. KV 存储流程

### 3.1 写入流程

```text
写入请求
    ↓
验证访问凭证
    ↓
计算 KV 哈希
    ↓
写入 L1 缓存（内存）
    ↓
异步写入 L2 磁盘（可选）
    ↓
异步写入 L3 Redis（可选）
    ↓
更新 Bloom Filter 索引
    ↓
返回成功
```

### 3.2 读取流程

```text
读取请求
    ↓
验证访问凭证
    ↓
查询 Bloom Filter 索引
    ↓
L1 缓存命中？──是──→ 返回数据
    │
   否
    ↓
L2 磁盘命中？──是──→ 加载到 L1 → 返回数据
    │
   否
    ↓
L3 Redis 命中？──是──→ 加载到 L1/L2 → 返回数据
    │
   否
    ↓
返回 NotFound
```

### 3.3 代码示例

```rust
use block_chain_with_context::MemoryLayerManager;

async fn write_kv_example(
    memory: &mut MemoryLayerManager,
    key: String,
    value: Vec<u8>,
    credential: &AccessCredential,
) -> Result<()> {
    // 1. 验证凭证
    if !credential.is_valid() {
        return Err(MemoryError::AccessDenied("Invalid credential".into()));
    }

    // 2. 计算 KV 哈希
    let hash = compute_hash(&value);

    // 3. 写入 L1 缓存
    memory.l1_cache.insert(key.clone(), value.clone());

    // 4. 异步写入 L2/L3
    memory.tiered_storage.write_async(key.clone(), value.clone()).await?;

    // 5. 更新 Bloom Filter 索引
    memory.index.add(&key);

    // 6. 返回哈希（用于上链存证）
    Ok(hash)
}
```

---

## 4. 故障切换流程

### 4.1 断路器状态机

```text
Closed（正常）
    │
    │ 连续失败 >= threshold
    ↓
Open（熔断）
    │
    │ 等待 delay
    ↓
HalfOpen（半开）
    │
    ├─ 成功 ──────→ Closed
    │
    └─ 失败 ──────→ Open
```

### 4.2 故障切换流程

```text
请求
    ↓
当前提供商执行
    │
    ├─ 成功 ──────→ 返回响应
    │
    └─ 失败 ──────→ 失败计数 +1
                    │
                    │ 失败计数 >= threshold?
                    ├─ 否 ──────→ 重试（指数退避）
                    │
                    └─ 是 ──────→ 熔断
                                  │
                                  ↓
                            选择备用提供商
                                  │
                                  ↓
                            执行请求
```

### 4.3 代码示例

```rust
use block_chain_with_context::failover::{CircuitBreaker, FailoverService};

async fn execute_with_failover(
    failover: &FailoverService,
    request: &InferenceRequest,
) -> Result<InferenceResponse> {
    let mut attempts = 0;
    let mut delay = 100;  // 初始延迟 100ms

    loop {
        match failover.provider_layer.execute(request).await {
            Ok(response) => {
                // 成功：重置断路器
                failover.circuit_breaker.reset();
                return Ok(response);
            }
            Err(e) => {
                // 失败：记录失败
                failover.circuit_breaker.record_failure();

                attempts += 1;
                if attempts >= failover.max_retries {
                    return Err(e);
                }

                // 指数退避
                tokio::time::sleep(Duration::from_millis(delay)).await;
                delay *= 2;
            }
        }
    }
}
```

---

## 5. 李群验证流程

### 5.1 验证流程

```text
各节点提交局部李代数元素 A_i
    ↓
链上聚合：G = exp(1/N * Σlog(g_i))
    ↓
QaaS 验证：计算距离 d(G, G_true)
    │
    ├─ d < threshold ──→ 验证通过
    │
    └─ d >= threshold ──→ 验证失败，触发仲裁
```

### 5.2 代码示例

```rust
use block_chain_with_context::lie_algebra::{
    LieAlgebraMapper, LieGroupAggregator, LieGroupMetric,
};

fn validate_consensus(
    elements: Vec<LieAlgebraElement>,
    expected: &LieGroupElement,
) -> Result<bool> {
    // 1. 映射到李群
    let group_elements: Vec<LieGroupElement> = elements
        .iter()
        .map(|a| LieAlgebraMapper::exp(a))
        .collect();

    // 2. 链上聚合
    let aggregated = LieGroupAggregator::geometric_mean(&group_elements)?;

    // 3. QaaS 验证
    let distance = LieGroupMetric::distance(&aggregated, expected);

    // 4. 判断是否通过
    Ok(distance < 0.5)  // threshold = 0.5
}
```

---

## 6. 锁顺序示例

### 6.1 正确示例

```rust
async fn correct_lock_order(
    l1: &Arc<RwLock<Cache>>,
    l2: &Arc<RwLock<Disk>>,
    l3: &Arc<RwLock<Remote>>,
) -> Result<()> {
    // 按顺序获取锁：L1 → L2 → L3
    let l1_guard = l1.read().await;
    let l2_guard = l2.read().await;
    let l3_guard = l3.read().await;

    // 执行操作
    let data = l1_guard.get("key");
    if data.is_none() {
        let data = l2_guard.get("key").await?;
        // ...
    }

    Ok(())
}
```

### 6.2 错误示例（可能死锁）

```rust
async fn wrong_lock_order(
    l1: &Arc<RwLock<Cache>>,
    l2: &Arc<RwLock<Disk>>,
) -> Result<()> {
    // 错误：先获取 L2 锁
    let l2_guard = l2.read().await;

    // 再获取 L1 锁（可能死锁！）
    let l1_guard = l1.read().await;  // ⚠️ 警告

    // ...

    Ok(())
}
```

---

## 7. 相关文档

- [整体架构](01-overview.md) - 三层架构、双链设计
- [模块详解](02-modules.md) - 5 个核心模块详解
- [李群验证](04-lie-group.md) - 信任根上移、四层架构

---

*最后更新：2026-03-26*  
*项目版本：v0.5.0*
