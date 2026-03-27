# 测试指南

> **阅读时间**: 15 分钟  
> **适用对象**: 开发者、测试工程师

---

## 1. 测试分类

### 1.1 单元测试

测试单个函数或方法的功能。

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_kv() {
        let mut memory = MemoryLayerManager::new("node_1");
        let credential = create_test_credential();

        let result = memory.write_kv(
            "key".to_string(),
            b"value".to_vec(),
            &credential,
        );

        assert!(result.is_ok());
    }
}
```

### 1.2 集成测试

测试多个模块的协同工作。

```rust
// tests/integration_tests.rs

use block_chain_with_context::*;

#[test]
fn test_full_inference_flow() {
    let node_layer = NodeLayerManager::new("node_1".into(), "addr_1".into());
    let memory_layer = MemoryLayerManager::new("node_1");
    let provider_layer = ProviderLayerManager::new();

    let request = InferenceRequest::new(
        "req_1".into(),
        "Hello".into(),
        "model".into(),
        100,
    );

    let response = provider_layer.execute_inference(
        &request,
        &memory_layer,
        &credential,
    ).unwrap();

    assert!(!response.completion.is_empty());
}
```

### 1.3 并发测试

测试并发场景下的正确性。

```rust
#[tokio::test]
async fn test_concurrent_writes() {
    let memory = Arc::new(RwLock::new(MemoryLayerManager::new("node_1")));
    let credential = create_test_credential();

    let tasks: Vec<_> = (0..100)
        .map(|i| {
            let memory = memory.clone();
            let credential = credential.clone();
            tokio::spawn(async move {
                let mut mem = memory.write().await;
                mem.write_kv(
                    format!("key_{}", i),
                    format!("value_{}", i).into_bytes(),
                    &credential,
                ).unwrap();
            })
        })
        .collect();

    for task in tasks {
        task.await.unwrap();
    }

    let mem = memory.read().await;
    for i in 0..100 {
        assert!(mem.read_kv(&format!("key_{}", i), &credential).is_some());
    }
}
```

### 1.4 模糊测试

测试边界情况和异常输入。

```rust
// tests/fuzz_tests.rs

use arbitrary::Arbitrary;

#[derive(Arbitrary, Debug)]
struct FuzzInput {
    data: Vec<u8>,
    key: String,
}

#[test]
fn fuzz_write_read() {
    fuzz!(|input: FuzzInput| {
        let mut memory = MemoryLayerManager::new("fuzz_node");
        let credential = create_test_credential();

        let _ = memory.write_kv(input.key.clone(), input.data, &credential);
        let _ = memory.read_kv(&input.key, &credential);
    });
}
```

### 1.5 基准测试

测试性能指标。

```rust
// benches/performance_bench.rs

use criterion::{criterion_group, criterion_main, Criterion};

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("kv_write_1000", |b| {
        b.iter(|| {
            let mut memory = MemoryLayerManager::new("bench_node");
            let credential = create_test_credential();
            
            for i in 0..1000 {
                memory.write_kv(
                    format!("key_{}", i),
                    format!("value_{}", i).into_bytes(),
                    &credential,
                ).unwrap();
            }
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
```

---

## 2. 运行测试

### 2.1 基本命令

```bash
# 运行所有测试
cargo test

# 运行单元测试
cargo test --lib

# 运行集成测试
cargo test --test integration_tests

# 运行特定测试
cargo test test_write_kv

# 带输出运行
cargo test -- --nocapture

# 单线程运行（调试并发问题）
cargo test -- --test-threads=1
```

### 2.2 并发测试

```bash
# 运行并发测试
cargo test --test concurrency_tests -- --nocapture

# 增加测试次数
cargo test test_concurrent -- --test-threads=16
```

### 2.3 基准测试

```bash
# 运行基准测试（需要 nightly）
cargo +nightly bench

# 运行特定基准
cargo +nightly bench lie_group_aggregation

# 对比性能
cargo +nightly bench -- --save-baseline main
cargo +nightly bench -- --baseline main
```

---

## 3. 编写测试

### 3.1 测试辅助函数

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_credential() -> AccessCredential {
        AccessCredential {
            credential_id: "test_cred".to_string(),
            provider_id: "test_provider".to_string(),
            memory_block_ids: vec!["all".to_string()],
            access_type: AccessType::ReadWrite,
            expires_at: u64::MAX,
            issuer_node_id: "test_node".to_string(),
            signature: "test_sig".to_string(),
            is_revoked: false,
        }
    }

    fn create_test_memory() -> MemoryLayerManager {
        MemoryLayerManager::new("test_node")
    }
}
```

### 3.2 测试错误处理

```rust
#[test]
fn test_read_not_found() {
    let memory = create_test_memory();
    let credential = create_test_credential();

    let result = memory.read_kv("nonexistent", &credential);

    assert!(result.is_none());
}

#[test]
fn test_invalid_credential() {
    let mut memory = create_test_memory();
    let credential = create_invalid_credential();

    let result = memory.write_kv(
        "key".to_string(),
        b"value".to_vec(),
        &credential,
    );

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MemoryError::AccessDenied(_)));
}
```

### 3.3 测试异步代码

```rust
#[tokio::test]
async fn test_async_write() {
    let mut memory = create_test_memory();
    let credential = create_test_credential();

    let result = memory.write_kv_async(
        "key".to_string(),
        b"value".to_vec(),
        &credential,
    ).await;

    assert!(result.is_ok());
}
```

---

## 4. 测试最佳实践

### 4.1 AAA 模式

```rust
#[test]
fn test_write_and_read() {
    // Arrange（准备）
    let mut memory = MemoryLayerManager::new("node_1");
    let credential = create_test_credential();
    let key = "test_key".to_string();
    let value = b"test_value".to_vec();

    // Act（执行）
    memory.write_kv(key.clone(), value.clone(), &credential).unwrap();
    let result = memory.read_kv(&key, &credential);

    // Assert（断言）
    assert!(result.is_some());
    assert_eq!(result.unwrap().data, value);
}
```

### 4.2 测试命名

```rust
// ✅ 好的命名
#[test]
fn test_write_kv_success() { }

#[test]
fn test_read_kv_not_found() { }

#[test]
fn test_write_kv_invalid_credential() { }

// ❌ 不好的命名
#[test]
fn test1() { }

#[test]
fn test_write() { }  // 太模糊
```

### 4.3 测试隔离

```rust
// ✅ 每个测试独立
#[test]
fn test_write_1() {
    let mut memory = MemoryLayerManager::new("node_1");
    // ...
}

#[test]
fn test_write_2() {
    let mut memory = MemoryLayerManager::new("node_2");  // 独立实例
    // ...
}

// ❌ 测试间依赖
#[test]
fn test_write() {
    // 写入数据
}

#[test]
fn test_read() {
    // 依赖 test_write 写入的数据  // 危险！
}
```

---

## 5. 测试覆盖率

### 5.1 安装 cargo-tarpaulin

```bash
cargo install cargo-tarpaulin
```

### 5.2 运行覆盖率

```bash
# 生成覆盖率报告
cargo tarpaulin --out Html

# 生成 LCOV 格式
cargo tarpaulin --out Lcov

# 排除测试代码
cargo tarpaulin --exclude-tests
```

### 5.3 查看覆盖率

```bash
# 打开生成的 coverage.html 文件
# 查看哪些代码未被覆盖
```

---

## 6. 常见问题

### 6.1 测试失败

**问题**: 测试间歇性失败

**解决方案**:
```bash
# 1. 增加测试重复次数
for i in {1..100}; do cargo test test_name; done

# 2. 单线程运行
cargo test test_name -- --test-threads=1

# 3. 添加日志
cargo test test_name -- --nocapture
```

### 6.2 测试超时

**问题**: 测试运行时间过长

**解决方案**:
```rust
#[test]
#[timeout(Duration::from_secs(10))]
fn test_slow_operation() {
    // ...
}
```

### 6.3 测试污染

**问题**: 测试间相互影响

**解决方案**:
```rust
// 使用临时目录
#[test]
fn test_with_temp_dir() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    // ...
}
```

---

## 7. 相关文档

- [开发环境](01-setup.md) - IDE、工具链配置
- [编码规范](02-coding-style.md) - Rust 代码规范
- [调试技巧](03-debugging.md) - 调试工具、常见问题
- [贡献流程](05-contributing.md) - Git 工作流、PR 流程

---

*最后更新：2026-03-26*  
*项目版本：v0.5.0*
