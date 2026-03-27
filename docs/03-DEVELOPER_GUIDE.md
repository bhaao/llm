# 开发者指南

> **适用对象**: 贡献者、维护者、高级用户  
> **最后更新**: 2026-03-26  
> **项目版本**: v0.5.0

---

## 1. 开发环境

### 1.1 安装 Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# 验证安装
rustc --version
cargo --version
```

### 1.2 安装开发工具

```bash
# rustfmt（代码格式化）
rustup component add rustfmt

# clippy（代码检查）
rustup component add clippy

# cargo-audit（安全审计）
cargo install cargo-audit
```

### 1.3 IDE 配置

**VS Code**:
- 安装扩展：rust-analyzer, crates, Error Lens
- 配置 `settings.json`:
```json
{
  "rust-analyzer.cargo.features": "all",
  "rust-analyzer.checkOnSave.command": "clippy"
}
```

---

## 2. 代码规范

### 2.1 命名约定

```rust
// 类型：PascalCase
pub struct InferenceRequest;
pub enum TransactionType { Transfer, Stake, Vote }

// 函数和变量：snake_case
pub fn execute_inference(request: &InferenceRequest) -> Result<InferenceResponse>;
let provider_id = "provider_1";

// 常量：UPPER_SNAKE_CASE
pub const MAX_TRANSACTIONS_PER_BLOCK: usize = 1000;

// Trait：PascalCase
pub trait Hashable {
    fn hash(&self) -> String;
}
```

### 2.2 错误处理

```rust
// ✅ 推荐：使用统一错误类型
use block_chain_with_context::AppError;

pub fn read_kv(&self, key: &str) -> AppResult<KvShard> {
    self.cache.get(key)
        .ok_or_else(|| AppError::kv_not_found(key))
}

// ❌ 不推荐：避免 .map_err(|e| format!(...))
.map_err(|e| format!("Error: {}", e))
```

### 2.3 异步编程

```rust
// ✅ 推荐：使用 async/await
pub async fn execute_inference(
    &self,
    request: &InferenceRequest,
) -> AppResult<InferenceResponse> {
    let response = self.client
        .post(url)
        .json(request)
        .send()
        .await?;

    Ok(response.json().await?)
}

// ❌ 不推荐：避免 tokio::spawn 包同步 IO
tokio::spawn(async move {
    // 同步操作
});
```

### 2.4 线程安全

```rust
// ✅ 推荐：使用 Arc<RwLock<T>>
pub struct ArchitectureCoordinator {
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub node_layer: Arc<NodeLayerManager>,
    pub memory_layer: Arc<MemoryLayerManager>,
}

// 访问时需要加锁
let mut bc = self.blockchain.write().unwrap();
bc.commit_inference(...);

// ❌ 不推荐：避免实现 Clone 导致深度克隆
impl Clone for Blockchain { ... }  // 已移除
```

---

## 3. 测试指南

### 3.1 测试分类

```bash
# 运行所有测试
cargo test

# 运行单元测试
cargo test --lib

# 运行集成测试
cargo test --test '*'

# 运行并发测试
cargo test --test concurrency_tests -- --nocapture

# 运行模糊测试
cargo test --test fuzz_tests -- --nocapture
```

### 3.2 编写单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_functionality() {
        // Arrange
        let blockchain = Blockchain::new("test".to_string());

        // Act
        blockchain.add_transaction(tx);

        // Assert
        assert_eq!(blockchain.chain.len(), 1);
    }

    #[tokio::test]
    async fn test_async_functionality() {
        // Arrange
        let manager = AsyncMemoryLayerManager::new("node_1");

        // Act
        manager.write_kv("key".to_string(), b"value".to_vec(), &cred).await.unwrap();

        // Assert
        let shard = manager.read_kv("key", &cred).await;
        assert!(shard.is_some());
    }
}
```

### 3.3 编写并发测试

```rust
#[tokio::test]
async fn test_100_threads_concurrent() {
    let blockchain = Arc::new(RwLock::new(Blockchain::new("test".to_string())));

    // 创建 100 个任务并发写入
    let handles: Vec<_> = (0..100)
        .map(|i| {
            let bc = blockchain.clone();
            tokio::spawn(async move {
                let mut bc = bc.write().await;
                bc.add_transaction(Transaction::new(...));
            })
        })
        .collect();

    // 等待所有任务完成
    for handle in handles {
        handle.await.unwrap();
    }

    // 验证
    let bc = blockchain.read().await;
    assert_eq!(bc.chain.len(), 101);
}
```

---

## 4. 预提交检查

```bash
# 格式化代码
cargo fmt

# 运行 clippy
cargo clippy --all-features --all-targets -- -D warnings

# 运行测试
cargo test --all-features

# 安全审计
cargo audit
```

---

## 5. 贡献流程

### 5.1 Fork 项目

```bash
# Fork 项目
gh repo fork <repo>

# 克隆到本地
git clone <your-fork>
cd block_chain_with_context
```

### 5.2 创建分支

```bash
# 创建功能分支
git checkout -b feature/your-feature-name

# 或修复分支
git checkout -b fix/issue-123
```

### 5.3 开发和测试

```bash
# 编写代码
# ...

# 格式化
cargo fmt

# 运行 clippy
cargo clippy --all-features --all-targets -- -D warnings

# 运行测试
cargo test --all-features

# 提交更改
git add .
git commit -m "feat: add your feature description"
```

### 5.4 提交 PR

```bash
# 推送到远程
git push origin feature/your-feature-name

# 创建 PR
gh pr create \
  --title "feat: your feature description" \
  --body "## Description\n\nDescribe your changes\n\n## Related Issues\n\nCloses #123"
```

---

## 6. 故障排查

### 6.1 protoc 未找到

```text
ERROR: protoc (protobuf compiler) not found
```

**解决方案**:
```bash
# Debian/Ubuntu
apt-get install protobuf-compiler

# macOS
brew install protobuf
```

### 6.2 编译警告错误

```text
error: unused variable: `x`
```

**解决方案**:
```bash
cargo clippy --all-features --all-targets -- -D warnings
```

### 6.3 线程死锁

```text
thread 'main' panicked at 'called `Result::unwrap()` on an `Err` value: PoisonError'
```

**解决方案**:
- 检查 `RwLock` 使用是否正确
- 避免在持有锁时执行耗时操作
- 使用带超时的锁获取方法

---

## 7. 相关文档

- [快速开始指南](01-GETTING_STARTED.md)
- [架构设计文档](02-ARCHITECTURE.md)
- [生产就绪度评估](04-PRODUCTION_READINESS.md)
- [测试指南](09-TESTING_GUIDE.md)

---

*最后更新：2026-03-26*  
*项目版本：v0.5.0*
