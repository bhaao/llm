# 测试指南

> **最后更新**: 2026-03-26  
> **项目版本**: v0.5.0

---

## 1. 测试分类

### 1.1 测试文件结构

```
tests/
├── concurrency_tests.rs          # 并发测试（100 线程）
├── property_tests.rs             # 属性测试（proptest）
├── chaos_tests.rs                # 混沌测试（故障注入）
├── integration_tests.rs          # 集成测试
├── pbft_integration_tests.rs     # PBFT 共识集成测试
├── gossip_integration_tests.rs   # Gossip 同步集成测试
├── async_commit_stress_tests.rs  # 异步提交压力测试
├── ollama_integration_tests.rs   # Ollama 集成测试
└── architecture_tests.rs         # 架构验证测试
```

### 1.2 测试类型

| 测试类型 | 文件 | 测试数量 | 说明 |
|---------|------|---------|------|
| 单元测试 | `src/*.rs` | ~50 | 模块内测试 |
| 并发测试 | `concurrency_tests.rs` | 3 | 100 线程并发 |
| 属性测试 | `property_tests.rs` | 11 | proptest 模糊测试 |
| 混沌测试 | `chaos_tests.rs` | 6 | 故障注入测试 |
| 集成测试 | `integration_tests.rs` | 10+ | 端到端测试 |
| PBFT 测试 | `pbft_integration_tests.rs` | 10+ | 共识集成测试 |
| Gossip 测试 | `gossip_integration_tests.rs` | 15+ | 同步集成测试 |

---

## 2. 运行测试

### 2.1 基本命令

```bash
# 运行所有测试
cargo test

# 运行单元测试
cargo test --lib

# 运行集成测试
cargo test --test '*'

# 运行特定模块测试
cargo test --lib blockchain

# 带输出运行测试
cargo test -- --nocapture
```

### 2.2 并发测试

```bash
# 运行并发测试
cargo test --test concurrency_tests -- --nocapture

# 运行单个并发测试
cargo test --test concurrency_tests test_100_threads_concurrent_read_write -- --nocapture
```

### 2.3 属性测试

```bash
# 运行属性测试
cargo test --test property_tests -- --nocapture

# 运行模糊测试
cargo test --test property_tests fuzz_large_transaction -- --nocapture
```

### 2.4 混沌测试

```bash
# 运行混沌测试
cargo test --test chaos_tests -- --nocapture
```

### 2.5 基准测试

```bash
# 运行基准测试（需要 nightly）
cargo +nightly bench

# 运行李群基准测试
cargo +nightly bench --bench lie_group_bench
```

---

## 3. 测试示例

### 3.1 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blockchain_creation() {
        // Arrange
        let blockchain = Blockchain::new("test".to_string());

        // Act & Assert
        assert_eq!(blockchain.chain.len(), 1);  // 创世区块
        assert_eq!(blockchain.chain[0].index, 0);
    }

    #[tokio::test]
    async fn test_async_memory_layer() {
        // Arrange
        let manager = AsyncMemoryLayerManager::new("node_1");
        let credential = create_test_credential();

        // Act
        manager
            .write_kv("key".to_string(), b"value".to_vec(), &credential)
            .await
            .unwrap();

        // Assert
        let shard = manager.read_kv("key", &credential).await;
        assert!(shard.is_some());
    }
}
```

### 3.2 并发测试

```rust
#[tokio::test]
async fn test_100_threads_concurrent_read_write() {
    // Arrange
    let blockchain = Arc::new(RwLock::new(Blockchain::new("test".to_string())));

    // Act: 创建 100 个任务并发写入
    let handles: Vec<_> = (0..100)
        .map(|i| {
            let bc = blockchain.clone();
            tokio::spawn(async move {
                let mut bc = bc.write().await;
                let tx = Transaction::new(
                    format!("node_{}", i),
                    format!("data_{}", i),
                    100,
                );
                bc.add_transaction(tx);
            })
        })
        .collect();

    // Wait: 等待所有任务完成
    for handle in handles {
        handle.await.unwrap();
    }

    // Assert: 验证
    let bc = blockchain.read().await;
    assert_eq!(bc.chain.len(), 101);  // 创世区块 + 100 个交易
}
```

### 3.3 属性测试（proptest）

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn prop_hash_consistency(data in any::<Vec<u8>>()) {
        // 相同数据产生相同哈希
        let hash1 = compute_hash(&data);
        let hash2 = compute_hash(&data);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn prop_hash_uniqueness(data1: Vec<u8>, data2: Vec<u8>) {
        prop_assume!(data1 != data2);

        // 不同数据产生不同哈希（概率极高）
        let hash1 = compute_hash(&data1);
        let hash2 = compute_hash(&data2);
        prop_assert_ne!(hash1, hash2);
    }

    #[test]
    fn prop_blockchain_chain_validity(
        block_count in 1..100usize,
        tx_count in 0..10usize
    ) {
        // 随机生成区块和交易
        let mut blockchain = Blockchain::new("test".to_string());
        
        for _ in 0..block_count {
            for _ in 0..tx_count {
                let tx = Transaction::new(
                    "from".to_string(),
                    "to".to_string(),
                    100,
                );
                blockchain.add_transaction(tx);
            }
            blockchain.mine_block();
        }

        // 验证链有效性
        prop_assert!(blockchain.is_chain_valid());
    }
}
```

### 3.4 混沌测试

```rust
#[tokio::test]
async fn test_provider_failover() {
    // Arrange: 模拟提供商故障
    let mut provider_layer = ProviderLayerManager::new();
    let provider = MockProvider::new("mock".to_string());
    provider_layer.register_provider(Box::new(provider)).unwrap();

    // Act: 触发故障
    provider_layer.simulate_failure("mock").unwrap();

    // Assert: 验证故障切换
    let healthy = provider_layer.get_healthy_providers();
    assert!(healthy.is_empty());
}

#[tokio::test]
async fn test_network_partition_recovery() {
    // Arrange: 模拟网络分区
    let mut gossip = GossipProtocol::new("node_1".to_string());
    
    // Act: 触发分区
    gossip.simulate_partition().unwrap();
    
    // Assert: 验证恢复
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(gossip.is_connected());
}
```

---

## 4. 测试覆盖

### 4.1 覆盖率统计

```bash
# 安装 cargo-tarpaulin
cargo install cargo-tarpaulin

# 运行覆盖率
cargo tarpaulin --out Html
```

### 4.2 模块覆盖率

| 模块 | 覆盖率 | 说明 |
|------|--------|------|
| `blockchain.rs` | ~85% | 核心逻辑 |
| `memory_layer.rs` | ~80% | KV 操作 |
| `services/` | ~75% | 业务编排 |
| `provider_layer.rs` | ~70% | LLM 集成 |
| `lie_algebra/` | ~90% | 数学验证 |

---

## 5. 基准测试

### 5.1 运行基准测试

```bash
# 运行所有基准测试
cargo +nightly bench

# 运行特定基准
cargo +nightly bench --bench lie_group_bench
```

### 5.2 基准测试结果

#### 李群性能（100 节点）

| 指标 | 实测 | 单位 |
|------|------|------|
| 聚合时间 | 53.19 | µs |
| 距离计算 | 137 | ns |
| 篡改检测 | ∞ | - |

#### KV 操作性能

| 操作 | 线程数 | P99 延迟 | 吞吐量 |
|------|--------|---------|--------|
| 并发写入 | 10 | ~5ms | ~10K ops/s |
| 并发写入 | 100 | ~20ms | ~50K ops/s |
| 审计日志读取 | 10 | ~1ms | ~100K ops/s |

---

## 6. 测试最佳实践

### 6.1 测试命名

```rust
// ✅ 推荐：清晰的测试名称
#[test]
fn test_blockchain_creation() {}

#[test]
fn test_100_threads_concurrent_read_write() {}

#[test]
fn test_provider_failover_on_timeout() {}

// ❌ 不推荐：模糊的测试名称
#[test]
fn test1() {}

#[test]
fn test_stuff() {}
```

### 6.2 测试结构

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // 辅助函数
    fn create_test_credential() -> AccessCredential {
        AccessCredential {
            credential_id: "test_cred".to_string(),
            // ...
        }
    }

    // 测试用例
    #[test]
    fn test_basic_functionality() {
        // Arrange
        let blockchain = Blockchain::new("test".to_string());

        // Act
        blockchain.add_transaction(tx);

        // Assert
        assert_eq!(blockchain.chain.len(), 1);
    }
}
```

### 6.3 异步测试

```rust
// ✅ 推荐：使用 #[tokio::test]
#[tokio::test]
async fn test_async_operation() {
    let result = async_operation().await;
    assert!(result.is_ok());
}

// ❌ 不推荐：使用 block_on
#[test]
fn test_async_operation() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(async_operation());  // 避免！
    assert!(result.is_ok());
}
```

---

## 7. 故障排查

### 7.1 测试失败

```bash
# 带输出运行失败测试
cargo test -- --nocapture

# 运行单个失败测试
cargo test --lib specific_test_name -- --nocapture

# 查看完整错误
RUST_BACKTRACE=1 cargo test -- --nocapture
```

### 7.2 测试超时

```bash
# 增加测试超时时间
cargo test -- --test-threads=1

# 运行单个超时测试
cargo test --lib slow_test -- --nocapture
```

### 7.3 并发测试失败

```bash
# 单线程运行（排查竞态条件）
cargo test -- --test-threads=1

# 多次运行（发现偶发问题）
for i in {1..10}; do cargo test --test concurrency_tests; done
```

---

## 8. 相关文档

- [开发者指南](03-DEVELOPER_GUIDE.md)
- [生产就绪度评估](04-PRODUCTION_READINESS.md)
- [快速开始指南](01-GETTING_STARTED.md)

---

*最后更新：2026-03-26*  
*项目版本：v0.5.0*
