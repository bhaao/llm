# 编码规范

> **阅读时间**: 15 分钟  
> **适用对象**: 开发者、贡献者

---

## 1. Rust 代码规范

### 1.1 命名约定

```rust
// 类型：PascalCase
struct MemoryLayerManager { }
enum AccessType { ReadOnly, ReadWrite }
trait InferenceProvider { }

// 函数、变量：snake_case
let node_id = "node_1";
fn read_kv(&self, key: &str) -> Result<Vec<u8>> { }

// 常量：SCREAMING_SNAKE_CASE
const MAX_RETRIES: u32 = 5;
const DEFAULT_TIMEOUT_MS: u64 = 30000;

// 泛型类型：PascalCase（通常单个字母）
fn process<T>(data: T) -> T { }
fn compare<A, B>(a: A, b: B) -> bool { }
```

### 1.2 文件组织

```rust
// 1. 模块声明
mod memory_layer;
mod blockchain;

// 2. use 导入（分组排序）
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use thiserror::Error;

use tokio::sync::RwLock;

// 3. 常量、静态变量
const DEFAULT_CAPACITY: usize = 1000;

// 4. 类型定义
pub struct MemoryLayerManager { }

// 5. impl 块
impl MemoryLayerManager { }

// 6. trait 实现
impl InferenceProvider for MemoryLayerManager { }

// 7. 测试模块
#[cfg(test)]
mod tests { }
```

### 1.3 错误处理

```rust
// 使用 thiserror 定义错误类型
#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("KV not found: {0}")]
    NotFound(String),

    #[error("Access denied: {0}")]
    AccessDenied(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// 使用 anyhow 处理应用层错误
use anyhow::Result;

fn process_request() -> Result<()> {
    // 使用 ? 传播错误
    let data = read_file("config.toml")?;
    Ok(())
}

// 错误转换
fn read_kv(key: &str) -> Result<Vec<u8>, MemoryError> {
    let value = get_value(key)
        .ok_or_else(|| MemoryError::NotFound(key.to_string()))?;
    Ok(value)
}
```

### 1.4 异步编程

```rust
use tokio::sync::RwLock;
use std::sync::Arc;

// 异步函数
pub async fn fetch_data(url: &str) -> Result<String> {
    let response = reqwest::get(url).await?;
    let data = response.text().await?;
    Ok(data)
}

// 异步锁
struct Cache {
    data: Arc<RwLock<HashMap<String, String>>>,
}

impl Cache {
    async fn get(&self, key: &str) -> Option<String> {
        let data = self.data.read().await;
        data.get(key).cloned()
    }

    async fn insert(&self, key: String, value: String) {
        let mut data = self.data.write().await;
        data.insert(key, value);
    }
}

// 并发执行
async fn fetch_all(urls: Vec<&str>) -> Result<Vec<String>> {
    let futures: Vec<_> = urls
        .iter()
        .map(|url| fetch_data(url))
        .collect();

    // 并发执行
    let results = futures::future::try_join_all(futures).await?;
    Ok(results)
}
```

### 1.5 生命周期

```rust
// 显式标注生命周期
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}

// 结构体中的生命周期
struct Borrowed<'a> {
    data: &'a str,
}

// 生命周期省略（常见情况）
impl MyStruct {
    // 编译器自动推断
    fn get_data(&self) -> &str { &self.data }
}
```

---

## 2. 文档注释

### 2.1 公共 API 文档

```rust
/// 记忆层管理器
///
/// 负责管理 KV 存储、分片、压缩和缓存。
///
/// # 示例
///
/// ```
/// use block_chain_with_context::MemoryLayerManager;
///
/// let mut memory = MemoryLayerManager::new("node_1");
/// memory.write_kv("key".to_string(), b"value".to_vec(), &credential)?;
/// ```
///
/// # 错误
///
/// 返回 [`MemoryError::NotFound`] 如果键不存在。
///
/// [`MemoryError::NotFound`]: crate::MemoryError::NotFound
pub struct MemoryLayerManager {
    /// 节点 ID
    node_id: String,
    /// KV 分片
    shards: HashMap<String, MemoryShard>,
}

impl MemoryLayerManager {
    /// 创建新的记忆层管理器
    ///
    /// # 参数
    ///
    /// * `node_id` - 节点唯一标识
    ///
    /// # 返回
    ///
    /// 返回新创建的 MemoryLayerManager 实例
    pub fn new(node_id: &str) -> Self {
        Self {
            node_id: node_id.to_string(),
            shards: HashMap::new(),
        }
    }

    /// 写入 KV 数据
    ///
    /// # 参数
    ///
    /// * `key` - 键
    /// * `value` - 值
    /// * `credential` - 访问凭证
    ///
    /// # 错误
    ///
    /// * `MemoryError::AccessDenied` - 凭证无效
    /// * `MemoryError::Io` - IO 错误
    pub fn write_kv(
        &mut self,
        key: String,
        value: Vec<u8>,
        credential: &AccessCredential,
    ) -> Result<(), MemoryError> {
        // ...
    }
}
```

### 2.2 内部注释

```rust
// 单行注释使用 //

// TODO: 优化 Bloom Filter 性能
// FIXME: 处理边界情况
// NOTE: 这里需要特殊处理

/*
 * 多行注释使用 /* 嵌套 */
 * 适用于较长的说明
 */

// 函数内注释：解释为什么，而不是做什么
let threshold = 0.67;  // 2/3 共识阈值，PBFT 要求
```

---

## 3. 测试规范

### 3.1 单元测试

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

    #[test]
    fn test_read_not_found() {
        let memory = MemoryLayerManager::new("node_1");
        let credential = create_test_credential();

        let result = memory.read_kv("nonexistent", &credential);

        assert!(result.is_none());
    }

    #[test]
    #[should_panic(expected = "Access denied")]
    fn test_invalid_credential() {
        let mut memory = MemoryLayerManager::new("node_1");
        let credential = create_invalid_credential();

        memory.write_kv("key".to_string(), b"value".to_vec(), &credential)
            .unwrap();
    }
}
```

### 3.2 集成测试

```rust
// tests/integration_tests.rs

use block_chain_with_context::*;

#[test]
fn test_full_inference_flow() {
    // 设置
    let node_layer = NodeLayerManager::new("node_1".into(), "addr_1".into());
    let memory_layer = MemoryLayerManager::new("node_1");
    let provider_layer = ProviderLayerManager::new();

    // 执行
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

    // 断言
    assert!(!response.completion.is_empty());
}
```

### 3.3 并发测试

```rust
#[tokio::test]
async fn test_concurrent_writes() {
    let memory = Arc::new(RwLock::new(MemoryLayerManager::new("node_1")));
    let credential = create_test_credential();

    // 创建 100 个并发写入任务
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

    // 等待所有任务完成
    for task in tasks {
        task.await.unwrap();
    }

    // 验证
    let mem = memory.read().await;
    for i in 0..100 {
        assert!(mem.read_kv(&format!("key_{}", i), &credential).is_some());
    }
}
```

---

## 4. 代码组织

### 4.1 模块划分

```rust
// lib.rs

// 核心模块
pub mod blockchain;
pub mod memory_layer;
pub mod node_layer;
pub mod provider_layer;

// 服务层
pub mod services;

// 子模块
mod consensus;
mod gossip;
mod lie_algebra;

// 内部模块（不公开）
mod metrics;
mod config;

// 公共 re-export
pub use memory_layer::MemoryLayerManager;
pub use blockchain::Blockchain;
pub use services::InferenceOrchestrator;
```

### 4.2 可见性控制

```rust
// 私有（默认）
fn internal_helper() { }

// 公开
pub fn public_api() { }

// 仅模块内可见
pub(crate) fn crate_internal() { }

// 仅特定子模块可见
pub(super) fn parent_module_only() { }
```

---

## 5. 性能最佳实践

### 5.1 避免不必要的克隆

```rust
// ❌ 不推荐
fn process(data: Vec<u8>) {
    let cloned = data.clone();  // 不必要的克隆
}

// ✅ 推荐
fn process(data: &[u8]) {
    // 使用引用
    let slice = &data[0..10];
}
```

### 5.2 使用迭代器

```rust
// ❌ 不推荐
let mut result = Vec::new();
for i in 0..vec.len() {
    result.push(vec[i] * 2);
}

// ✅ 推荐
let result: Vec<_> = vec.iter().map(|x| x * 2).collect();
```

### 5.3 预分配容量

```rust
// ❌ 不推荐
let mut vec = Vec::new();
for i in 0..1000 {
    vec.push(i);
}

// ✅ 推荐
let mut vec = Vec::with_capacity(1000);
for i in 0..1000 {
    vec.push(i);
}
```

---

## 6. 安全最佳实践

### 6.1 输入验证

```rust
// ✅ 验证输入
fn process_request(request: &Request) -> Result<()> {
    if request.data.len() > MAX_SIZE {
        return Err(Error::TooLarge);
    }
    if !is_valid_format(&request.data) {
        return Err(Error::InvalidFormat);
    }
    // ...
}
```

### 6.2 资源管理

```rust
// ✅ 使用 RAII
fn read_file(path: &str) -> Result<String> {
    let mut file = File::open(path)?;  // 自动关闭
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}
```

### 6.3 并发安全

```rust
// ✅ 使用 Arc<RwLock<T>>
struct SharedCache {
    data: Arc<RwLock<HashMap<String, String>>>,
}

// ❌ 避免裸指针
struct UnsafeCache {
    data: *mut HashMap<String, String>,  // 危险！
}
```

---

## 7. 相关文档

- [开发环境](01-setup.md) - IDE、工具链配置
- [调试技巧](03-debugging.md) - 调试工具、常见问题
- [测试指南](04-testing.md) - 单元测试、并发测试
- [贡献流程](05-contributing.md) - Git 工作流、PR 流程

---

*最后更新：2026-03-26*  
*项目版本：v0.5.0*
