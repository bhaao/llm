# 分布式 KV 缓存系统

[![Build Status](https://img.shields.io/github/actions/workflow/status/user/block_chain_with_context/ci.yml)](https://github.com/user/block_chain_with_context/actions)
[![Crates.io](https://img.shields.io/crates/v/block_chain_with_context.svg)](https://crates.io/crates/block_chain_with_context)
[![Documentation](https://docs.rs/block_chain_with_context/badge.svg)](https://docs.rs/block_chain_with_context)
[![License](https://img.shields.io/crates/l/block_chain_with_context.svg)](LICENSE)

一个高性能的分布式 KV 缓存系统，专为大模型推理场景设计，带哈希审计日志功能。

> [!WARNING]
> **项目状态：v0.5.0 - 架构验证原型**
>
> **这是一个架构验证原型，不是生产就绪系统。**
>
> 本项目展示了分布式 KV 缓存 + 审计日志的架构设计，核心概念已验证，但部分模块仍处于原型阶段。
> 生产环境使用请务必参阅 [`docs/limitations.md`](docs/limitations.md)。

---

## 📖 项目简介

本项目采用 Rust 实现了一套高性能的分布式 KV 缓存系统，专为大模型推理场景优化：

- **核心功能**：分布式 KV 上下文存储，支持分片、压缩、多级缓存
- **审计日志**：KV 哈希存证，提供不可篡改的数据完整性验证
- **信誉系统**：节点信誉管理，支持可信调度

### 核心理念

> **数据本地存储 + 哈希全网存证**
>
> - 记忆层存储实际 KV 数据，支持本地高速访问
> - 审计日志记录 KV 哈希，提供全网存证验证

### 架构设计

**三层架构**：

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
                              ↑ ↓ 哈希存证
┌─────────────────────────────────────────────────────────────┐
│                    审计日志层 (Audit Layer)                  │
│  • KV 哈希存证（不可篡改）                                   │
│  • 节点信誉管理                                              │
│  • 共识结果记录                                              │
└─────────────────────────────────────────────────────────────┘
```

详细架构说明请参阅 [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)。

---

## ✨ 核心特性

### 已完成特性

| 特性 | 状态 | 说明 |
|------|------|------|
| 真实 LLM 集成 | ✅ | 支持 vLLM/SGLang HTTP API |
| 断路器模式 | ✅ | 连续失败自动切换 |
| 异步 I/O | ✅ | 全链路 async/await |
| 线程安全 | ✅ | 100 线程并发测试通过 |
| KV Cache 优化 | ✅ | Chunk-level + 压缩 + 预取 |
| 上下文分片 | ✅ | 支持 100K+ tokens 跨节点 |
| 多级缓存 | ✅ | L1 CPU + L2 Disk + L3 Remote(Redis) |
| gRPC 通信 | ✅ | 跨节点 RPC 支持 |
| P2P 网络层 | ⚠️ 原型 | libp2p stub 实现 |
| PBFT 共识 | ⚠️ 原型 | 框架完整，libp2p stub 已实现 |

### 生产就绪度

| 模块 | 状态 | 说明 |
|------|------|------|
| 服务层 | ✅ 生产就绪 | 推理编排、存证、故障切换服务 |
| 审计日志（单节点） | ✅ 生产就绪 | KV 哈希存证、交易记录 |
| 记忆层 | ✅ 生产就绪 | KV Cache 存储、分片、压缩、L3 Redis |
| 节点层 | ✅ 生产就绪 | 节点管理、信誉系统 |
| 提供商层 | ✅ 生产就绪 | 真实 LLM API 集成（vLLM/SGLang） |
| P2P 网络层 | ⚠️ 原型 | libp2p stub 实现，待完整集成 |
| PBFT 共识 | ⚠️ 原型 | 共识框架完整 |

详细评估请参阅 [`docs/limitations.md`](docs/limitations.md)。

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

#### 2. 配置管理（Builder 模式）

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

#### 3. 审计日志（哈希存证）

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

---

## 📊 性能指标

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

**数据来源**：`cargo bench` 基准测试报告

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

## 📚 文档

- [**开发者指南**](docs/DEVELOPER_GUIDE.md) - 开发环境、代码规范、贡献流程
- [**架构文档**](docs/ARCHITECTURE.md) - 系统架构、数据流、监控
- [**P11 锐评与修复**](docs/P11_REVIEW.md) - 业内专家锐评及修复记录
- [**修复总结**](docs/REMEDIATION_SUMMARY.md) - 修复进度总结
- [**API 文档**](https://docs.rs/block_chain_with_context) - 完整 API 文档

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

详细故障排查请参阅 [`docs/DEVELOPER_GUIDE.md`](docs/DEVELOPER_GUIDE.md#故障排查)。

---

## 🤝 贡献

欢迎贡献代码、报告问题或提出建议！

1. Fork 项目
2. 创建功能分支 (`git checkout -b feature/AmazingFeature`)
3. 提交更改 (`git commit -m 'Add some AmazingFeature'`)
4. 推送到分支 (`git push origin feature/AmazingFeature`)
5. 开启 Pull Request

详细贡献流程请参阅 [`docs/DEVELOPER_GUIDE.md`](docs/DEVELOPER_GUIDE.md#贡献流程)。

---

## 📄 许可证

本项目采用 MIT 许可证 - 参阅 [LICENSE](LICENSE) 文件。

---

## 🙏 致谢

感谢业内专家的 P11 锐评，帮助我们改进了项目。

详见 [`docs/P11_REVIEW.md`](docs/P11_REVIEW.md)。

---

*最后更新：2026-03-11*
*项目版本：v0.5.0*
