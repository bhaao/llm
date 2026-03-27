# 快速开始

> **阅读时间**: 15 分钟  
> **适用对象**: 新用户

---

## 1. 克隆项目

```bash
git clone <repo-url>
cd block_chain_with_context
```

---

## 2. 构建项目

### 2.1 默认构建

```bash
# 默认构建（包含 HTTP + gRPC + 多级缓存）
cargo build

# 构建 Release 版本（优化性能）
cargo build --release
```

### 2.2 构建配置

```bash
# 启用全部特性
cargo build --all-features

# 仅启用 HTTP RPC（无需 protoc）
cargo build --no-default-features --features "rpc,tiered-storage"

# 启用 L3 Redis 缓存
cargo build --features "remote-storage"

# 启用 P2P 网络
cargo build --features "p2p"
```

### 2.3 特性说明

| 特性 | 说明 | 依赖 |
|------|------|------|
| `rpc` (默认) | HTTP RPC 支持 | axum, tower, tower-http |
| `grpc` (默认) | gRPC 跨节点通信 | tonic, prost, prost-types |
| `tiered-storage` (默认) | 多级缓存支持 | bincode |
| `remote-storage` | L3 Redis 缓存 | redis |
| `p2p` | P2P 网络支持 | libp2p |
| `persistence` | 状态持久化 | rocksdb |

---

## 3. 运行测试

### 3.1 基本测试

```bash
# 运行所有测试
cargo test

# 运行单元测试
cargo test --lib

# 运行特定测试
cargo test --lib test_name
```

### 3.2 并发测试

```bash
# 运行并发测试
cargo test --test concurrency_tests -- --nocapture

# 运行模糊测试
cargo test --test fuzz_tests -- --nocapture
```

### 3.3 基准测试

```bash
# 运行基准测试（需要 nightly）
cargo +nightly bench

# 运行特定基准测试
cargo +nightly bench lie_group_aggregation
```

---

## 4. 运行示例

### 4.1 基本 KV 存储

创建示例文件 `examples/basic_kv.rs`:

```rust
use block_chain_with_context::{MemoryLayerManager, AccessCredential, AccessType};

#[tokio::main]
async fn main() {
    // 创建记忆层管理器
    let mut memory = MemoryLayerManager::new("node_1");

    // 创建访问凭证
    let credential = AccessCredential {
        credential_id: "cred_1".to_string(),
        provider_id: "provider_1".to_string(),
        memory_block_ids: vec!["all".to_string()],
        access_type: AccessType::ReadWrite,
        expires_at: u64::MAX,
        issuer_node_id: "node_1".to_string(),
        signature: "sig".to_string(),
        is_revoked: false,
    };

    // 写入 KV 数据
    memory.write_kv("key".to_string(), b"value".to_vec(), &credential)
        .expect("写入失败");

    // 读取 KV 数据
    let shard = memory.read_kv("key", &credential);
    assert!(shard.is_some());

    println!("KV 操作成功！");
}
```

运行示例：
```bash
cargo run --example basic_kv
```

### 4.2 配置管理（Builder 模式）

```rust
use block_chain_with_context::BlockchainConfig;

fn main() {
    // 使用 Builder 模式构建配置
    let config = BlockchainConfig::builder()
        .trust_threshold(0.75)           // 可信阈值 0.75
        .inference_timeout_ms(30000)     // 推理超时 30 秒
        .commit_timeout_ms(10000)        // 上链超时 10 秒
        .max_retries(5)                  // 最大重试 5 次
        .log_level("info")               // 日志级别
        .build()
        .expect("配置验证失败");

    println!("配置构建成功：{:?}", config);
}
```

### 4.3 审计日志（哈希存证）

```rust
use block_chain_with_context::{Blockchain, KvCacheProof};

fn main() {
    // 创建区块链（审计日志）
    let mut blockchain = Blockchain::new("node_1".to_string());

    // 注册节点
    blockchain.register_node("node_1".to_string());

    // 添加 KV 存证
    let kv_proof = KvCacheProof::new(
        "kv_001".to_string(),
        "hash_123".to_string(),
        "node_1".to_string(),
        1024,
    );
    blockchain.add_kv_proof(kv_proof);

    println!("KV 存证添加成功！");
}
```

---

## 5. 服务层 API（推荐）

服务层是推荐的使用方式，提供完整的功能封装：

```rust
use std::sync::Arc;
use block_chain_with_context::services::{
    InferenceOrchestrator, CommitmentService, FailoverService,
};
use block_chain_with_context::{
    NodeLayerManager, MemoryLayerManager, ProviderLayerManager,
    Blockchain, BlockchainConfig,
};
use tokio::sync::RwLock;

#[tokio::main]
async fn main() {
    // 创建各层管理器
    let node_layer = Arc::new(NodeLayerManager::new(
        "node_1".into(), "addr_1".into()
    ));
    let memory_layer = Arc::new(MemoryLayerManager::new("node_1"));
    let provider_layer = Arc::new(ProviderLayerManager::new());

    // 创建三个服务
    let orchestrator = InferenceOrchestrator::new(
        node_layer.clone(),
        memory_layer.clone(),
        provider_layer.clone(),
    );

    let commitment = CommitmentService::with_config(
        "addr_1".into(),
        BlockchainConfig::default(),
    ).unwrap();

    let failover = FailoverService::new(
        provider_layer.clone(),
        TimeoutConfig::default(),
    );

    println!("服务层初始化成功！");
}
```

---

## 6. 启动 RPC 节点

### 6.1 示例代码

```rust
use block_chain_with_context::{
    RpcServer, NodeLayerManager, MemoryLayerManager,
    Blockchain, BlockchainConfig,
};
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() {
    // 初始化组件
    let node_layer = Arc::new(NodeLayerManager::new(
        "node_1".to_string(),
        "address_1".to_string(),
    ));

    let memory_layer = Arc::new(MemoryLayerManager::new("node_1"));

    let blockchain = Arc::new(RwLock::new(
        Blockchain::with_config(
            "address_1".to_string(),
            BlockchainConfig::default(),
        ).unwrap()
    ));

    // 创建并启动 RPC 服务器
    let server = RpcServer::new(
        node_layer,
        memory_layer,
        blockchain,
        "0.0.0.0:3000",
    );

    println!("Starting RPC server on 0.0.0.0:3000");
    server.run().await.unwrap();
}
```

### 6.2 使用 curl 测试

```bash
# 健康检查
curl http://localhost:3000/health

# 读取 KV 分片
curl "http://localhost:3000/get_kv_shard?key=my_key&shard=context"

# 提交交易
curl -X POST http://localhost:3000/submit_transaction \
  -H "Content-Type: application/json" \
  -d '{
    "from": "user_1",
    "to": "assistant_1",
    "transaction_type": "transfer",
    "data": null
  }'
```

---

## 7. 集成真实 LLM 服务

### 7.1 启动 vLLM 服务

```bash
# 使用 vLLM 启动 Llama 模型
python -m vllm.entrypoints.api_server \
    --model meta-llama/Llama-2-7b-chat-hf \
    --host 0.0.0.0 \
    --port 8000
```

### 7.2 在代码中使用

```rust
use block_chain_with_context::{
    LLMProvider, InferenceEngineType,
    ProviderLayerManager, InferenceRequest,
};

// 创建 LLM 提供商
let provider = LLMProvider::new(
    "vllm".to_string(),
    InferenceEngineType::Vllm,
    "http://localhost:8000",
    100,  // 算力容量 (token/s)
);

// 注册并执行推理
let mut manager = ProviderLayerManager::new();
manager.register_provider(Box::new(provider)).unwrap();
manager.set_current_provider("vllm").unwrap();

let request = InferenceRequest::new(
    "req_1".to_string(),
    "Hello, AI!".to_string(),
    "llama-7b".to_string(),
    100,
);

let response = manager.execute_inference(
    &request,
    &memory_layer,
    &credential,
).unwrap();

println!("Response: {}", response.completion);
```

---

## 8. 常见问题

### 8.1 protoc 未找到

```text
ERROR: protoc (protobuf compiler) not found
```

**解决方案**:
```bash
# Debian/Ubuntu
apt-get install protobuf-compiler

# macOS
brew install protobuf

# 或禁用 gRPC 特性
cargo build --no-default-features --features rpc,tiered-storage
```

### 8.2 编译警告错误

```text
error: unused variable: `x`
```

**解决方案**:
```bash
cargo clippy --all-features --all-targets -- -D warnings
```

### 8.3 测试失败

```text
test result: FAILED. 125 passed; 3 failed
```

**解决方案**:
```bash
# 带输出运行失败测试
cargo test -- --nocapture

# 运行单个失败测试
cargo test --lib specific_test_name -- --nocapture
```

---

## 9. 下一步

- ⚙️ [配置指南](04-configuration.md) - 配置文件、环境变量
- 🏗️ [整体架构](../02-architecture/01-overview.md) - 深入了解系统架构
- 🛠️ [开发环境](../03-development/01-setup.md) - IDE、工具链配置

---

*最后更新：2026-03-26*  
*项目版本：v0.5.0*
