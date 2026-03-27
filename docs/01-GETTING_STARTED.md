# 快速开始指南

> **项目版本**: v0.5.0  
> **最后更新**: 2026-03-26  
> **定位**: 分布式 KV 缓存系统 - 架构验证原型

---

## 1. 项目简介

### 1.1 项目定位

**分布式 KV 缓存系统**，专为大模型推理场景设计，带哈希审计日志功能。

> ⚠️ **重要声明**: 这是一个**架构验证原型**，不是生产就绪系统。
>
> 本项目展示了分布式 KV 缓存 + 审计日志的架构设计，核心概念已验证，但部分模块仍处于原型阶段。
> 生产环境使用请务必参阅 [`04-PRODUCTION_READINESS.md`](04-PRODUCTION_READINESS.md)。

### 1.2 核心功能

| 功能 | 说明 |
|------|------|
| **分布式 KV 存储** | 分片、压缩、多级缓存（L1 CPU + L2 Disk + L3 Remote） |
| **哈希审计日志** | KV 数据哈希存证，提供不可篡改的数据完整性验证 |
| **节点信誉系统** | 节点信誉管理，支持可信调度 |
| **真实 LLM 集成** | 支持 vLLM/SGLang HTTP API |
| **断路器模式** | 连续失败自动切换，指数退避重试 |

### 1.3 核心理念

> **数据本地存储 + 哈希全网存证**
>
> - 记忆层存储实际 KV 数据，支持本地高速访问
> - 审计日志记录 KV 哈希，提供全网存证验证

---

## 2. 环境要求

### 2.1 基础要求

| 依赖 | 版本 | 说明 |
|------|------|------|
| **Rust** | 1.70+ | 必须 |
| **Edition** | 2021 | 必须 |
| **protoc** | 3.0+ | gRPC 特性需要 |

### 2.2 安装 protoc

```bash
# Debian/Ubuntu
apt-get install protobuf-compiler

# macOS
brew install protobuf

# Arch Linux
pacman -S protobuf

# Windows
# 下载 https://github.com/protocolbuffers/protobuf/releases
```

---

## 3. 快速开始

### 3.1 克隆项目

```bash
git clone <repo-url>
cd block_chain_with_context
```

### 3.2 构建项目

```bash
# 默认构建（包含 HTTP + gRPC + 多级缓存）
cargo build

# 启用全部特性
cargo build --all-features

# 构建 Release 版本
cargo build --release
```

### 3.3 运行测试

```bash
# 运行所有测试
cargo test

# 运行单元测试
cargo test --lib

# 运行并发测试
cargo test --test concurrency_tests -- --nocapture

# 运行基准测试（需要 nightly）
cargo +nightly bench
```

### 3.4 运行示例

```bash
# 运行主程序
cargo run
```

---

## 4. 特性说明

### 4.1 默认特性

项目默认启用以下特性：

```toml
default = ["rpc", "grpc", "tiered-storage"]
```

| 特性 | 说明 | 依赖 |
|------|------|------|
| `rpc` | HTTP RPC 支持 | axum, tower, tower-http |
| `grpc` | gRPC 跨节点通信 | tonic, prost, prost-types |
| `tiered-storage` | 多级缓存支持 | bincode |

### 4.2 可选特性

| 特性 | 说明 | 依赖 |
|------|------|------|
| `remote-storage` | L3 Redis 缓存 | redis |
| `p2p` | P2P 网络支持 | libp2p |
| `persistence` | 状态持久化 | rocksdb |

### 4.3 构建配置示例

```bash
# 仅启用 HTTP RPC（无需 protoc）
cargo build --no-default-features --features "rpc,tiered-storage"

# 启用 L3 Redis 缓存
cargo build --features "remote-storage"

# 启用 P2P 网络
cargo build --features "p2p"
```

---

## 5. 使用示例

### 5.1 基本 KV 存储

```rust
use block_chain_with_context::{MemoryLayerManager, AccessCredential, AccessType};

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
memory.write_kv("key".to_string(), b"value".to_vec(), &credential).unwrap();

// 读取 KV 数据
let shard = memory.read_kv("key", &credential);
assert!(shard.is_some());
```

### 5.2 配置管理（Builder 模式）

```rust
use block_chain_with_context::BlockchainConfig;

// 使用 Builder 模式构建配置
let config = BlockchainConfig::builder()
    .trust_threshold(0.75)           // 可信阈值 0.75
    .inference_timeout_ms(30000)     // 推理超时 30 秒
    .commit_timeout_ms(10000)        // 上链超时 10 秒
    .max_retries(5)                  // 最大重试 5 次
    .log_level("info")               // 日志级别
    .build()
    .expect("配置验证失败");
```

### 5.3 审计日志（哈希存证）

```rust
use block_chain_with_context::{Blockchain, KvCacheProof};

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
```

### 5.4 服务层 API（推荐）

```rust
use std::sync::Arc;
use block_chain_with_context::services::{
    InferenceOrchestrator, CommitmentService, FailoverService,
};

// 创建各层管理器
let node_layer = Arc::new(NodeLayerManager::new("node_1".into(), "addr_1".into()));
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

// 执行推理流程
let provider_id = orchestrator.select_provider().unwrap();
let response = orchestrator.execute(&request, &credential, &provider_id).unwrap();

// 上链存证
commitment.commit_inference(metadata, &provider_id, &response, kv_proofs).unwrap();
```

---

## 6. 集成真实 LLM 服务

### 6.1 启动 vLLM 服务

```bash
# 使用 vLLM 启动 Llama 模型
python -m vllm.entrypoints.api_server \
    --model meta-llama/Llama-2-7b-chat-hf \
    --host 0.0.0.0 \
    --port 8000
```

### 6.2 在代码中使用

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

## 7. 启动 RPC 节点

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

### 7.1 使用 curl 测试

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

## 8. 性能指标

### 8.1 KV 操作延迟

| 操作 | L1 命中 | L2 命中 | L3 命中 |
|------|--------|--------|--------|
| 读取延迟 | < 1ms | 10-50ms | 100-500ms |
| 写入延迟 | < 1ms | 10-50ms | 100-500ms |
| 成本/GB | $0.05 | $0.01 | $0.001 |

### 8.2 并发性能

| 测试场景 | 线程数 | 吞吐量 | P99 延迟 |
|---------|--------|--------|---------|
| KV 并发写入 | 10 | ~10K ops/s | ~5ms |
| KV 并发写入 | 100 | ~50K ops/s | ~20ms |
| 审计日志读取 | 10 | ~100K ops/s | ~1ms |

**数据来源**: `cargo bench` 基准测试报告

---

## 9. 故障排查

### 9.1 protoc 未找到

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
cargo build --no-default-features --features rpc
```

### 9.2 编译警告错误

```text
error: unused variable: `x`
```

**解决方案**:
```bash
cargo clippy --all-features --all-targets -- -D warnings
```

### 9.3 测试失败

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

## 10. 下一步

- 📖 [架构设计文档](02-ARCHITECTURE.md) - 深入了解系统架构
- 🛠️ [开发者指南](03-DEVELOPER_GUIDE.md) - 开发环境和代码规范
- 📊 [生产就绪度评估](04-PRODUCTION_READINESS.md) - 生产环境适用性
- 📝 [变更日志](08-CHANGELOG.md) - 版本更新历史

---

*最后更新：2026-03-27*
*项目版本：v0.5.0*
