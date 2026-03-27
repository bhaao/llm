# 区块链驱动的分布式推理验证平台

[![Build Status](https://img.shields.io/github/actions/workflow/status/user/block_chain_with_context/ci.yml)](https://github.com/user/block_chain_with_context/actions)
[![Crates.io](https://img.shields.io/crates/v/block_chain_with_context.svg)](https://crates.io/crates/block_chain_with_context)
[![Documentation](https://docs.rs/block_chain_with_context/badge.svg)](https://docs.rs/block_chain_with_context)
[![License](https://img.shields.io/crates/l/block_chain_with_context.svg)](https://crates.io/crates/block_chain_with_context)

一个**区块链驱动的分布式推理验证平台**，专为大模型推理场景设计，通过李群验证和区块链存证解决"如何验证分布式推理结果可信"的核心问题。

> [!WARNING]
> **项目状态：v0.5.0 - 架构验证原型**
>
> **这是一个架构验证原型，不是生产就绪系统。**
>
> 本项目展示了**区块链 + 李群验证 + 分布式推理**的创新架构，核心概念已验证，但部分模块仍处于原型阶段。
> 生产环境使用请务必参阅 [`docs/04-PRODUCTION_READINESS.md`](docs/04-PRODUCTION_READINESS.md)。

---

## 📖 项目简介

本项目采用 Rust 实现了一套**区块链驱动的分布式推理验证平台**，专为大模型推理场景优化：

- **核心创新**：李群验证 + 区块链存证，解决分布式推理结果的可信验证问题
- **KV 缓存**：分布式 KV 上下文存储，支持分片、压缩、多级缓存（李群验证的载体）
- **审计日志**：KV 哈希存证 + 李群聚合根，提供不可篡改的数据完整性验证
- **信誉系统**：节点信誉管理，支持可信调度

### 核心理念

> **数据本地存储 + 哈希全网存证 + 李群验证**
>
> - 记忆层存储实际 KV 数据，支持本地高速访问
> - 审计日志记录 KV 哈希，提供全网存证验证
> - **李群聚合**：节点提交局部李代数，链上聚合全局李群状态，信任根从"信任节点"上移到"信任数学公式"

### 为什么不是传统 KV 缓存系统？

| 维度 | 传统 KV 缓存 | 本项目（推理验证平台） |
|------|------------|---------------------|
| **核心问题** | 如何高效存储/读取 KV | 如何验证分布式推理结果可信 |
| **KV 缓存** | 核心功能 | 载体/应用场景 |
| **区块链** | 可选/无 | 核心基础设施（信任根） |
| **李群验证** | 无 | 核心竞争力（区别于其他项目） |
| **信任根** | 信任节点 | 信任数学公式（李群聚合） |

### 架构设计

**三层架构 + 李群验证**：

```
┌─────────────────────────────────────────────────────────────┐
│                    推理提供商层 (Provider Layer)             │
│  • 从记忆层读取 KV/上下文                                    │
│  • 执行 LLM 推理计算（vLLM/SGLang API）                      │
│  • 向记忆层写入新生成的 KV                                   │
│  • 向审计日志层上报推理指标                                  │
└─────────────────────────────────────────────────────────────┘
                              ↑ ↓ 读取/写入 KV
┌─────────────────────────────────────────────────────────────┐
│                    记忆层 (Memory Layer)                     │
│  • KV Cache 存储（分片、分层、压缩）                         │
│  • 哈希链式校验（防篡改）                                    │
│  • 分布式多副本存储（容灾）                                  │
│  • 版本控制/访问授权                                         │
└─────────────────────────────────────────────────────────────┘
                              ↑ ↓ 哈希存证 + 李代数提交
┌─────────────────────────────────────────────────────────────┐
│                    审计日志层 (Audit Layer)                  │
│  • KV 哈希存证（不可篡改）                                   │
│  • 李群聚合（信任根：G = exp(1/N * Σlog(g_i))）             │
│  • PBFT 共识（拜占庭容错）                                   │
│  • 节点信誉管理                                              │
└─────────────────────────────────────────────────────────────┘
```

**四层验证架构**（李群驱动）：
```
第一层：分布式上下文分片层（不可信节点） → 提交局部李代数 A_i
    ↓
第二层：李群链上聚合层（系统核心，信任根） → 生成全局李群状态 G
    ↓
第三层：QaaS 质量验证层（李群度量） → 输出验证结果
    ↓
第四层：区块链存证与激励层 → 记录 KvCacheProof + LieGroupRoot
```

详细架构说明请参阅 [`docs/02-ARCHITECTURE.md`](docs/02-ARCHITECTURE.md)。

---

## ✨ 核心特性

### 已完成特性

| 特性 | 状态 | 说明 |
|------|------|------|
| **李群验证** | ✅ | 100 节点聚合 53µs，信任根上移到数学公式 |
| **区块链存证** | ✅ | KV 哈希 + 李群根不可篡改记录 |
| **PBFT 共识** | ⚠️ 原型 | 三阶段提交 + 视图切换，libp2p stub |
| **真实 LLM 集成** | ✅ | 支持 vLLM/SGLang HTTP API |
| **断路器模式** | ✅ | 连续失败自动切换，指数退避重试 |
| **异步 I/O** | ✅ | 全链路 async/await |
| **线程安全** | ✅ | 100 线程并发测试通过 |
| **KV Cache 优化** | ✅ | Chunk-level + 压缩 + 预取 + 分片 |
| **多级缓存** | ✅ | L1 CPU + L2 Disk + L3 Remote(Redis) |
| **gRPC 通信** | ✅ | 跨节点 RPC 支持 |
| **Gossip 同步** | ⚠️ 原型 | Vector Clock + Merkle Tree |

### 生产就绪度

| 模块 | 状态 | 说明 |
|------|------|------|
| **李群验证模块** | ✅ 生产就绪 | 100 节点聚合 53µs，篡改检测∞ |
| **服务层** | ✅ 生产就绪 | 推理编排、存证、故障切换服务 |
| **审计日志（单节点）** | ✅ 生产就绪 | KV 哈希存证、李群根记录 |
| **记忆层** | ✅ 生产就绪 | KV Cache 存储、分片、压缩、L3 Redis |
| **节点层** | ✅ 生产就绪 | 节点管理、信誉系统、访问凭证 |
| **提供商层** | ✅ 生产就绪 | 真实 LLM API 集成（vLLM/SGLang） |
| **PBFT 共识** | ⚠️ 原型 | 共识框架完整，待 libp2p 完整集成 |
| **Gossip 同步** | ⚠️ 原型 | 协议完整，待 libp2p 完整集成 |
| **P2P 网络层** | ⚠️ 原型 | libp2p stub 实现 |

详细评估请参阅 [`docs/04-PRODUCTION_READINESS.md`](docs/04-PRODUCTION_READINESS.md)。

---

## 🚀 快速开始

### 环境要求

- **Rust**: 1.70+
- **protoc**: 3.0+（gRPC 特性需要）

### 安装依赖

```bash
# 安装 protoc（如未安装）
apt-get install protobuf-compiler  # Debian/Ubuntu
brew install protobuf              # macOS

# 构建项目（默认特性）
cargo build

# 构建项目（启用 P2P 网络层）
cargo build --features "p2p"

# 构建项目（所有特性）
cargo build --all-features

# 运行测试
cargo test
```

### 特性说明

| 特性 | 说明 |
|------|------|
| `rpc` (默认) | HTTP RPC + gRPC 跨节点通信 |
| `grpc` (默认) | gRPC 支持（需 protoc） |
| `tiered-storage` (默认) | 分层存储支持（KV Cache 优化） |
| `remote-storage` | 远程存储支持（L3 Redis） |
| `p2p` | P2P 网络支持（libp2p） |
| `persistence` | 状态持久化（RocksDB） |

### 使用示例

#### 1. 基本 KV 存储

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

#### 2. 李群验证（核心创新）

```rust
use block_chain_with_context::lie_algebra::{
    LieGroupAggregator, LieAlgebraElement, LieGroupType,
};

// 创建聚合器（信任根：硬编码聚合公式）
let aggregator = LieGroupAggregator::default_with_type(LieGroupType::SE3);

// 准备李代数元素列表（来自多个节点）
let algebra_elements = vec![
    LieAlgebraElement::new("node_1", vec![0.1, 0.2, 0.3, 1.0, 2.0, 3.0], LieGroupType::SE3),
    LieAlgebraElement::new("node_2", vec![0.2, 0.3, 0.4, 1.5, 2.5, 3.5], LieGroupType::SE3),
    LieAlgebraElement::new("node_3", vec![0.15, 0.25, 0.35, 1.2, 2.2, 3.2], LieGroupType::SE3),
];

// 执行聚合：G = exp(1/N * Σlog(g_i))
let result = aggregator.aggregate(&algebra_elements).unwrap();

// 获取全局李群状态 G（信任根）
let global_group = result.global_state;
assert!(result.is_valid);
assert_eq!(result.contributor_count, 3);
```

#### 3. 配置管理（Builder 模式）

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

#### 4. 审计日志（哈希存证 + 李群根）

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
```

---

## 📊 性能指标

### 李群验证性能（核心创新）

| 指标 | 生产要求 | 实测 | 评价 |
|------|----------|------|------|
| **聚合时间** | < 100ms | **53.19 µs** | ✅ 快 1880 倍 |
| **距离计算** | < 10ms | **137 ns** | ✅ 快 73000 倍 |
| **篡改检测** | ×5.47 | **∞** | ✅ 验证通过 |

**测试场景**：100 节点，SE(3) 李群类型，6 维特征

### KV 操作延迟

| 操作 | L1 命中 | L2 命中 | L3 命中 |
|------|--------|--------|--------|
| 读取延迟 | < 1ms | 10-50ms | 100-500ms |
| 写入延迟 | < 1ms | 10-50ms | 100-500ms |
| 成本/GB | $0.05 | $0.01 | $0.001 |

### 并发性能

| 测试场景 | 线程数 | 吞吐量 | P99 延迟 |
|---------|--------|--------|---------|
| KV 并发写入 | 10 | ~10K ops/s | ~5ms |
| KV 并发写入 | 100 | ~50K ops/s | ~20ms |
| 审计日志读取 | 10 | ~100K ops/s | ~1ms |
| 李群聚合 | 100 节点 | 1 次聚合 | 53µs |

**数据来源**：`cargo +nightly bench` 基准测试报告

运行基准测试：
```bash
cargo +nightly bench
```

---

## 🧪 测试

```bash
# 运行所有测试
cargo test

# 运行单元测试
cargo test --lib

# 运行并发测试
cargo test --test concurrency_tests -- --nocapture

# 运行模糊测试
cargo test --test fuzz_tests -- --nocapture

# 运行基准测试（需要 nightly）
cargo +nightly bench
```

### 测试覆盖

| 测试类型 | 文件 | 测试数量 |
|---------|------|---------|
| 单元测试 | `src/*.rs` | ~50 |
| 并发测试 | `tests/concurrency_tests.rs` | ~10 |
| 模糊测试 | `tests/fuzz_tests.rs` | ~15 |
| 基准测试 | `benches/performance_bench.rs` | ~15 |

---

## 📚 文档导航

本项目文档分为三个层次，请根据需求选择阅读：

### 核心文档（推荐）

| 文档 | 说明 | 适合人群 |
|------|------|----------|
| [**快速开始**](docs/01-GETTING_STARTED.md) | 环境安装、构建运行、使用示例 | 新用户 |
| [**架构设计**](docs/02-ARCHITECTURE.md) | 三层架构、双链设计、数据流 | 架构师、开发者 |
| [**开发者指南**](docs/03-DEVELOPER_GUIDE.md) | 开发环境、代码规范、调试技巧 | 贡献者 |
| [**生产就绪度**](docs/04-PRODUCTION_READINESS.md) | 模块成熟度评估、生产部署建议 | 技术决策者 |
| [**路线图**](docs/10-ROADMAP.md) | 版本历史、未来规划 | 所有人 |

### Wiki 文档

适合团队协作维护和快速查阅：

- [**Wiki 首页**](docs/wiki/README.md) - Wiki 导航入口
- [**入门篇**](docs/wiki/INDEX.md) - 项目介绍、环境安装、快速开始
- [**架构篇**](docs/wiki/INDEX.md) - 架构详解、模块说明、李群验证
- [**开发篇**](docs/wiki/INDEX.md) - 开发环境、编码规范、测试指南
- [**运维篇**](docs/wiki/INDEX.md) - 部署指南、监控告警、故障排查
- [**参考篇**](docs/wiki/INDEX.md) - API 速查、配置项、FAQ

### 内部参考

- [**内部文档首页**](docs/internal/项目总结.md) - 历史文档、实现细节、技术报告
- [**项目总结**](docs/internal/项目总结.md) - 全面技术总结
- [**李群实现**](docs/07-LIE_GROUP_IMPLEMENTATION.md) - 李群验证实现总结

### 外部资源

- [**API 文档**](https://docs.rs/block_chain_with_context) - 完整 Rust API 文档
- [**Crates.io**](https://crates.io/crates/block_chain_with_context) - Cargo 包页面

---

## 🏗️ 架构说明

### 依赖关系

```text
推理提供商 → 依赖 → 记忆层（读取/写入 KV）
推理提供商 → 依赖 → 审计日志层（上报指标）
记忆层   → 依赖 → 审计日志层（哈希存证）
审计日志层 → 不依赖 → 推理提供商/记忆层
```

### 锁顺序规范

为避免死锁，所有锁操作遵循以下顺序：

```
L1 缓存锁 → L2 磁盘锁 → L3 远程锁 → 审计日志锁 → 记忆层锁
```

违反顺序会在 debug 模式下触发警告。

---

## 🔧 故障排查

### 常见问题

#### 1. protoc 未找到

```text
ERROR: protoc (protobuf compiler) not found
```

**解决方案**：
```bash
# Debian/Ubuntu
apt-get install protobuf-compiler

# macOS
brew install protobuf
```

#### 2. 编译警告错误

```text
error: unused variable: `x`
```

**解决方案**：
```bash
cargo clippy --all-features --all-targets -- -D warnings
```

详细故障排查请参阅 [`docs/03-DEVELOPER_GUIDE.md`](docs/03-DEVELOPER_GUIDE.md#6-故障排查)。

---

## 🤝 贡献

欢迎贡献代码、报告问题或提出建议！

1. Fork 项目
2. 创建功能分支 (`git checkout -b feature/AmazingFeature`)
3. 提交更改 (`git commit -m 'Add some AmazingFeature'`)
4. 推送到分支 (`git push origin feature/AmazingFeature`)
5. 开启 Pull Request

详细贡献流程请参阅 [`docs/03-DEVELOPER_GUIDE.md`](docs/03-DEVELOPER_GUIDE.md#5-贡献流程)。

---

## 📄 许可证

本项目采用 MIT 许可证 - 参阅 [Crates.io 许可证页面](https://crates.io/crates/block_chain_with_context)。

---

## 🙏 致谢

感谢业内专家的 P11 锐评，帮助我们改进了项目。

详见 [`docs/05-P11_REVIEW_FIXES.md`](docs/05-P11_REVIEW_FIXES.md)。

---

*最后更新：2026-03-27*
*项目版本：v0.5.0*
