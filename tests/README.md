# 测试套件文档

## 测试架构概览

本项目的测试体系分为五个层次，覆盖从单元测试到集成测试、从功能验证到性能基准的全方位测试需求。

```
tests/
├── integration_tests.rs   # 集成测试：端到端流程验证
├── concurrency_tests.rs   # 并发测试：多线程/异步压力测试
├── property_tests.rs      # 属性测试：随机化输入验证
├── mock_tests.rs          # Mock 测试：模拟失败/边界场景
├── test_utils.rs          # 测试工具：数据生成器
└── README.md              # 本文档
```

## 测试分类说明

### 1. 集成测试 (`integration_tests.rs`)

**测试目标**：验证多个模块协同工作的正确性

**覆盖场景**：
- 完整区块链生命周期（创建→填充→提交→持久化→恢复）
- 多节点共识流程
- KV Cache 证明验证
- 跨块依赖验证
- 错误处理（重复交易、Gas 超限、存储损坏）
- 信誉系统状态转换
- 质量评估器模式切换

**运行命令**：
```bash
cargo test --test integration_tests
```

**关键测试用例**：
| 测试名称 | 测试意图 | 验证点 |
|---------|---------|-------|
| `test_full_blockchain_lifecycle` | 验证完整生命周期 | 创建、填充、提交、持久化、恢复 |
| `test_multi_node_consensus` | 多节点共识 | 3 个节点依次提交区块 |
| `test_kv_cache_proof_verification` | KV 证明验证 | 证明被正确记录到区块 |
| `test_duplicate_transaction_rejection` | 重复交易检测 | 同一交易不被重复添加 |
| `test_gas_limit_error_handling` | Gas 超限错误 | 超过限制时返回错误 |
| `test_corrupted_storage_detection` | 存储损坏检测 | 加载损坏文件返回错误 |

---

### 2. 并发测试 (`concurrency_tests.rs`)

**测试目标**：验证多线程/异步环境下的数据一致性和无死锁

**覆盖场景**：
- 10/100 线程并发写入
- 读写混合并发
- 并发区块提交
- 竞态条件检测
- 死锁检测
- 异步任务并发

**运行命令**：
```bash
cargo test --test concurrency_tests
```

**关键测试用例**：
| 测试名称 | 并发规模 | 验证点 |
|---------|---------|-------|
| `test_concurrent_writes_10_threads` | 10 线程 | 基础并发写入 |
| `test_concurrent_writes_100_threads` | 100 线程 | 高并发压力 |
| `test_concurrent_read_write_mixed` | 50 读 +50 写 | 读写混合 |
| `test_concurrent_block_commits` | 5 线程 | 并发提交区块 |
| `test_race_condition_double_commit` | 2 线程 | 竞态条件 |
| `test_deadlock_detection_rapid_access` | 20 线程×10 次 | 死锁检测 |

---

### 3. 属性测试 (`property_tests.rs`)

**测试目标**：使用 proptest 进行随机化输入验证

**覆盖场景**：
- SHA256 哈希一致性
- Transaction 哈希一致性
- Block 哈希一致性
- 区块链完整性
- Merkle Root 计算
- 序列化往返一致性
- 信誉分范围
- Gas 计算正确性

**运行命令**：
```bash
cargo test --test property_tests
```

**关键属性**：
| 属性名称 | 属性规则 | 测试方法 |
|---------|---------|---------|
| 哈希一致性 | 相同输入→相同输出 | `prop_assert_eq!(hash1, hash2)` |
| 哈希固定长度 | 任意输入→64 字符 | `assert_eq!(hash.len(), 64)` |
| 链完整性 | 每块 prev_hash=前块 hash | `verify_chain().is_ok()` |
| 序列化往返 | serialize→deserialize=原对象 | `assert_eq!(tx.hash(), deserialized.hash())` |
| 信誉分范围 | 0.0 ≤ score ≤ 1.0 | `prop_assert!(score >= 0.0 && score <= 1.0)` |

---

### 4. Mock 测试 (`mock_tests.rs`)

**测试目标**：模拟失败场景和边界条件

**覆盖场景**：
- 存储失败（路径不存在）
- 存储损坏检测
- 存储恢复机制
- 网络延迟模拟
- 节点掉线模拟
- 恶意节点假签名
- 超大区块
- 空链操作
- Gas 边界值
- 交易数边界值

**运行命令**：
```bash
cargo test --test mock_tests
```

**关键场景**：
| 场景名称 | 模拟条件 | 预期行为 |
|---------|---------|---------|
| 存储失败 | 路径不存在 | `save()` 返回错误 |
| 存储损坏 | 无效 JSON | `load()` 返回错误 |
| 存储恢复 | 从备份恢复 | 数据正确恢复 |
| 网络延迟 | 100ms 延迟 | 数据最终一致 |
| 节点掉线 | 离线节点 | 不影响活跃节点 |
| Gas 边界 | 刚好达到上限 | 允许添加 |
| Gas 超限 | 超过上限 1 | 拒绝添加 |

---

### 5. 测试工具 (`test_utils.rs`)

**设计目标**：消除硬编码，提供随机化数据生成

**核心组件**：
- `random_string()`: 随机字符串生成
- `random_hash()`: 随机哈希生成
- `TransactionBuilder`: 交易构建器
- `BlockBuilder`: 区块构建器
- `KvCacheProofBuilder`: KV 证明构建器
- `TestBlockchainBuilder`: 区块链构建器
- 预设场景函数

**使用示例**：
```rust
use test_utils::*;

// 简单用法
let blockchain = create_empty_blockchain();
let txs = generate_transactions(10);

// 构建器用法
let tx = TransactionBuilder::new()
    .from("alice".to_string())
    .to("bob".to_string())
    .gas_used(100)
    .build();

// 复杂场景
let blockchain = TestBlockchainBuilder::new()
    .with_user("test_user")
    .with_transactions(5)
    .with_gas_limit(1000)
    .with_quality_assessor()
    .build();
```

---

### 6. 基准测试 (`benches/blockchain_bench.rs`)

**测试目标**：测量性能指标和趋势

**覆盖场景**：
- 添加交易延迟
- 区块提交延迟（不同大小）
- 链验证延迟（不同长度）
- 哈希计算性能
- Merkle Root 计算性能
- 信誉操作性能
- 质量评估性能

**运行命令**：
```bash
cargo bench
```

**输出位置**：
- HTML 报告：`target/criterion/report/index.html`
- 原始数据：`target/criterion/<benchmark_name>/`

**关键指标**：
| 基准测试 | 测量对象 | 预期趋势 |
|---------|---------|---------|
| `add_pending_transaction` | 单交易添加 | < 10μs |
| `commit_inference_single_block` | 单块提交 | < 100μs |
| `verify_chain` | 链验证 | O(n) 线性增长 |
| `sha256_short_string` | 短字符串哈希 | < 1μs |
| `merkle_root` | Merkle 根计算 | O(log n) |

---

## 测试覆盖率要求

### 单元测试（源文件内）
- 所有公共函数必须有单元测试
- 所有 `Result` 类型的 `Ok` 和 `Err` 分支都要测
- 边界条件必须覆盖（空、单元素、最大值）

### 集成测试
- 端到端流程必须测
- 多模块交互必须测
- 持久化和恢复必须测

### 并发测试
- 多线程读写必须测
- 竞态条件必须测
- 死锁检测必须测

### 属性测试
- 哈希/签名必须用 proptest
- 序列化往返必须用 proptest
- 数值范围必须用 proptest

### Mock 测试
- 存储失败场景必须测
- 网络异常场景必须测
- 边界值必须测

---

## 测试运行命令汇总

```bash
# 运行所有测试
cargo test

# 运行特定测试文件
cargo test --test integration_tests
cargo test --test concurrency_tests
cargo test --test property_tests
cargo test --test mock_tests

# 运行基准测试
cargo bench

# 运行特定测试
cargo test test_full_blockchain_lifecycle
cargo test test_concurrent_writes_100_threads

# 显示测试输出
cargo test -- --nocapture

# 运行测试并生成覆盖率报告（需要 cargo-tarpaulin）
cargo tarpaulin --out Html
```

---

## 测试编写规范

### 1. 测试命名
```rust
// 格式：test_<功能>_<场景>_<预期>
#[test]
fn test_gas_limit_exceeded_returns_error() { }

#[test]
fn test_duplicate_transaction_rejected() { }
```

### 2. 测试文档
```rust
/// 简短说明测试意图
/// 
/// **测试场景**：描述测试的输入条件
/// **验证点**：描述测试验证的内容
/// **预期结果**：描述预期的输出
#[test]
fn test_example() { }
```

### 3. 断言质量
```rust
// ❌ 糟糕：只验证"没崩溃"
assert!(result.is_ok());

// ✅ 优秀：验证业务语义
assert_eq!(blockchain.chain.len(), 1);
assert_eq!(blockchain.pending_gas_used(), expected_gas);
```

### 4. 测试数据
```rust
// ❌ 糟糕：硬编码
let tx = Transaction::new_internal(
    "user_1".to_string(),
    "assistant_1".to_string(),
    // ...
);

// ✅ 优秀：使用生成器
let tx = TransactionBuilder::new()
    .from(random_user_id())
    .to(random_assistant_id())
    .build();
```

---

## 测试维护指南

### 添加新功能时的测试清单
- [ ] 添加单元测试（源文件内）
- [ ] 添加集成测试（如果涉及多模块）
- [ ] 添加属性测试（如果是核心算法）
- [ ] 添加错误路径测试
- [ ] 更新基准测试（如果影响性能）

### 修复 Bug 时的测试清单
- [ ] 添加回归测试（防止再次出现）
- [ ] 验证现有测试通过
- [ ] 检查测试覆盖率是否提升

### 重构代码时的测试清单
- [ ] 运行所有测试
- [ ] 验证基准测试性能无退化
- [ ] 更新受影响的测试

---

## 常见问题解答

### Q: 单元测试和集成测试的区别？
A: 单元测试测试单个函数/方法，集成测试测试多个模块协同工作。

### Q: 什么时候用属性测试？
A: 当需要验证"对于所有可能的输入都成立"的性质时，如哈希一致性、序列化往返等。

### Q: 并发测试总是失败怎么办？
A: 检查是否有数据竞争，使用 `Arc<RwLock>` 保护共享状态，增加适当延迟。

### Q: 基准测试结果波动大怎么办？
A: 运行多次取平均值，确保测试环境稳定（关闭其他程序，固定 CPU 频率）。

---

## 参考资源

- [Rust Book - Testing](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [proptest 文档](https://altsysrq.github.io/proptest-book/intro.html)
- [criterion 文档](https://bheisler.github.io/criterion.rs/book/index.html)
- [mockall 文档](https://docs.rs/mockall/latest/mockall/)
