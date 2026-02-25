# Block Chain with Context

**分布式大模型上下文可信存储系统** —— 区块链与分布式 LLM 的正确结合方式

## 📖 项目简介

本项目是 Memora 项目的核心子模块，采用 Rust 实现了一套创新的"区块链 + 分布式 LLM 推理"架构。不同于将区块链类比为计算过程的错误设计，本项目将区块链作为**可信增强工具**，通过 KV Cache 链上存证实现分布式推理的可信化。

### 核心理念

> **区块链仅存证 KV 哈希，不存储实际数据；记忆链存储实际 KV 数据，哈希上链存证。**

两条链配合实现"**数据本地存储 + 哈希全网共识**"的可信架构。

---

## 🏗️ 架构设计

### 双链架构

| 链类型 | 职责 | 存储内容 |
|--------|------|----------|
| **区块链 (Blockchain)** | 全局可信存证主链 | KV 哈希、元数据、信誉记录 |
| **记忆链 (MemoryChain)** | 分布式 KV 数据链 | 实际上下文数据 (KV Cache) |

### 三层解耦架构（企业级）

基于联盟链设计原则，实现节点、记忆、推理三层彻底解耦：

```
┌─────────────────────────────────────────────────────────────┐
│                    区块链节点层                              │
│  (Node Layer)                                               │
│  • 节点身份/公钥/信誉管理                                    │
│  • 推理提供商准入/调度/切换/惩罚                             │
│  • 记忆层哈希校验/存证上链                                   │
│  • 跨节点共识/仲裁                                           │
│  约束：无状态、轻量逻辑 (<5ms/次)、支持异步上链              │
└─────────────────────────────────────────────────────────────┘
                              ↑ ↓ 哈希校验/存证
┌─────────────────────────────────────────────────────────────┐
│                    分布式记忆层                              │
│  (Memory Layer)                                             │
│  • 以"区块"为单位存储 KV/上下文分片                          │
│  • 哈希链式串联（防篡改）                                    │
│  • 分布式多副本存储（容灾）                                  │
│  • 版本控制/访问授权                                         │
└─────────────────────────────────────────────────────────────┘
                              ↑ ↓ 读取/写入 KV
┌─────────────────────────────────────────────────────────────┐
│                  推理服务提供商层                            │
│  (Provider Layer)                                           │
│  • 从记忆层读取 KV/上下文                                    │
│  • 执行 LLM 推理计算                                         │
│  • 向记忆层写入新生成的 KV                                   │
│  • 向节点层上报推理指标                                      │
│  约束：无区块链能力、无记忆存储能力、标准化接口              │
└─────────────────────────────────────────────────────────────┘
```

### 依赖关系（单向依赖，杜绝递归/闭环）

```text
推理提供商 → 依赖 → 记忆层（读取/写入 KV）
推理提供商 → 依赖 → 节点层（获取访问授权/上报指标）
记忆层   → 依赖 → 节点层（哈希校验/存证上链）
节点层   → 不依赖 → 推理提供商/记忆层（仅做管控，不做执行）
```

---

## ✨ 核心创新

### 创新 A：KV Cache 链上存证

将 KV 块的哈希上链存证，设计简单但有效：

- **防篡改**：验证 KV 数据是否被篡改
- **跨节点一致性**：跨节点 KV 一致性校验
- **版本追溯**：追溯历史 KV 版本

### 创新 B：链上可信调度

通过区块链记录节点信誉和调度决策：

- **信誉系统**：基于历史表现动态计算节点信誉分
- **可信调度**：优先调度高信誉节点处理推理任务
- **惩罚机制**：异常节点自动降权/剔除

---

## 🚀 快速开始

### 环境要求

- Rust 1.70+
- Rust Edition 2021

### 安装依赖

```bash
cargo build
```

### 基本使用

#### 传统区块链 API

```rust
use block_chain_with_context::{
    Blockchain, Transaction, TransactionType, TransactionPayload,
    BlockMetadata, KvCacheProof
};

// 创建区块链
let mut blockchain = Blockchain::new("user_address".to_string());

// 注册推理节点
blockchain.register_node("node_1".to_string());

// 添加推理请求
let tx = Transaction::new(
    "user".to_string(),
    "assistant".to_string(),
    TransactionType::Transfer,
    TransactionPayload::None,
);
blockchain.add_pending_transaction(tx);

// 添加 KV Cache 存证
let kv_proof = KvCacheProof::new(
    "kv_001".to_string(),
    "kv_hash".to_string(),
    "node_1".to_string(),
    1024,
);
blockchain.add_kv_proof(kv_proof);

// 提交推理记录到链上
let metadata = BlockMetadata::default();
blockchain.commit_inference(metadata, "node_1".to_string());
```

#### 三层解耦架构 API

```rust
use block_chain_with_context::coordinator::ArchitectureCoordinator;
use block_chain_with_context::provider_layer::{
    InferenceEngineType, InferenceRequest
};

// 创建架构协调器
let mut coordinator = ArchitectureCoordinator::new("node_1".to_string());

// 注册推理提供商
coordinator.register_provider(
    "provider_1".to_string(),
    InferenceEngineType::Vllm,
    100, // 100 token/s
).unwrap();

// 执行完整推理流程
let request = InferenceRequest::new(
    "req_1".to_string(),
    "Hello, AI!".to_string(),
    "llama-7b".to_string(),
    100,
);
let response = coordinator.execute_inference(request).unwrap();

// 验证链完整性
assert!(coordinator.verify_memory_chain());
assert!(coordinator.verify_blockchain());
```

### 运行示例

```bash
cargo run
```

输出示例：
```text
=== 分布式推理记录已上链 ===
区块高度：1
区块哈希：0xabc123...
交易数量：1
KV 存证数量：1
总 Token 数：100
链验证：true

=== 节点信誉 ===
可信节点数：2/2
node_1 信誉分：100.00
node_1 完成任务数：1
node_1 处理 Token 数：100

=== KV Cache 存证 ===
KV 块：kv_001, 哈希：kv_hash_abc123, 节点：node_1
```

---

## 📦 功能特性

### 核心模块

| 模块 | 描述 |
|------|------|
| `blockchain` | 区块链核心实现（区块、交易、共识） |
| `memory_layer` | 分布式记忆层（KV 存储、分片、分层存储） |
| `node_layer` | 节点管理层（身份、信誉、调度） |
| `provider_layer` | 推理提供商层（引擎适配、HTTP 客户端） |
| `coordinator` | 架构协调器（三层解耦 orchestration） |
| `failover` | 故障转移（健康监控、自动切换） |
| `quality_assessment` | 质量评估（语义检查、完整性验证） |
| `reputation` | 信誉系统（节点评分、事件记录） |
| `storage` | 持久化存储（JSON 序列化） |

### 可选特性（Features）

```toml
[features]
default = []
async = ["tokio"]              # 异步运行时支持
http = ["reqwest", "tokio"]    # HTTP 客户端（调用推理服务）
tiered-storage = ["tokio", "bincode"]  # 分层存储（GPU/CPU/磁盘）
```

启用特性：
```bash
# 启用异步支持
cargo build --features async

# 启用 HTTP 客户端
cargo build --features http

# 启用分层存储
cargo build --features tiered-storage

# 启用全部特性
cargo build --features "async,http,tiered-storage"
```

---

## 🧪 测试

```bash
# 运行所有测试
cargo test

# 运行特定模块测试
cargo test --lib blockchain
cargo test --lib memory_layer

# 带输出运行测试
cargo test -- --nocapture
```

---

## 📊 性能基准

```bash
# 运行基准测试（需要 nightly）
cargo bench
```

---

## 🔧 开发路线图

### 已完成 ✅

- [x] 区块链核心数据结构（区块、交易、哈希链）
- [x] 双链架构设计（区块链 + 记忆链）
- [x] 三层解耦架构实现
- [x] 基础信誉系统
- [x] KV Cache 存证机制
- [x] 质量评估框架

### 进行中 🚧

- [ ] 真实 LLM 推理引擎集成（vLLM/SGLang HTTP API）
- [ ] 上下文分片与重组
- [ ] 基础版 Ring Attention
- [ ] 三层分层存储（GPU/CPU/磁盘）
- [ ] KV Cache 压缩（量化、稀疏化）

### 计划中 📅

- [ ] gRPC/RPC 通信层
- [ ] 分布式共识协议
- [ ] 跨节点 KV 同步
- [ ] 生产级故障转移
- [ ] 监控与可观测性

详细开发计划请参考 [`建议.md`](建议.md) 文件。

---

## 📁 项目结构

```
block_chain_with_context/
├── src/
│   ├── lib.rs              # 库入口，重新导出公共类型
│   ├── main.rs             # CLI 示例程序
│   ├── block.rs            # 区块定义
│   ├── blockchain.rs       # 区块链核心实现
│   ├── transaction.rs      # 交易定义
│   ├── metadata.rs         # 区块元数据
│   ├── error.rs            # 错误类型定义
│   ├── traits.rs           # 核心特征（Hashable, Serializable 等）
│   ├── coordinator.rs      # 三层架构协调器
│   ├── node_layer.rs       # 节点管理层
│   ├── memory_layer.rs     # 记忆层管理
│   ├── memory_layer/       # 记忆层子模块
│   │   ├── tiered_storage.rs   # 分层存储
│   │   └── kv_compression.rs   # KV 压缩
│   ├── provider_layer.rs   # 推理提供商层
│   ├── provider_layer/     # 提供商层子模块
│   │   └── http_client.rs      # HTTP 客户端
│   ├── failover.rs         # 故障转移
│   ├── quality_assessment.rs  # 质量评估
│   ├── reputation.rs       # 信誉系统
│   ├── storage.rs          # 持久化存储
│   └── utils.rs            # 工具函数
├── tests/                  # 集成测试
├── benches/                # 基准测试
├── Cargo.toml              # 项目配置
├── 建议.md                 # 详细开发计划
└── README.md               # 本文件
```

