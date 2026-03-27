# 项目变更日志

> 记录项目的所有版本更新和重大变更

---

## [v0.5.0] - 2026-03-05

### 主题
李群验证 + Redis 集成 + libp2p + 混沌测试

### 核心功能

#### 1. 李群验证模块
- ✅ `src/lie_algebra/types.rs` - 核心数据结构
- ✅ `src/lie_algebra/mapper.rs` - 李代数映射器（可插拔）
- ✅ `src/lie_algebra/aggregator.rs` - 李群聚合器（信任根）
- ✅ `src/lie_algebra/metric.rs` - 李群度量器（可插拔）
- ✅ `benches/lie_group_bench.rs` - 性能基准测试

**性能基准**（100 节点）:
- 聚合时间：53.19 µs（快 1880 倍）
- 距离计算：137 ns（快 73000 倍）

#### 2. Redis 集成（L3 缓存）
- ✅ `src/memory_layer/redis_backend.rs` - Redis 后端
- ✅ `remote-storage` feature 启用

#### 3. libp2p 简化版
- ✅ `src/network/libp2p.rs` - P2P 网络 stub
- ✅ `p2p` feature 启用

#### 4. 混沌测试
- ✅ `tests/chaos_tests.rs` - 6 种混沌测试场景

### 综合评分
**4.5/5 ⭐⭐⭐⭐⭐**（保持）

---

## [v0.4.1] - 2026-03-01

### 主题
Feature 设计修复 + 集成测试 + 构建体验改进

### 核心修复

#### Feature 设计修复
- ✅ 删除空 feature `async-runtime`
- ✅ gRPC 和 tiered-storage 加入默认 features
- ✅ 默认 features: `["rpc", "grpc", "tiered-storage"]`

#### 构建体验改进
- ✅ protoc 检测增强，提供友好的错误信息
- ✅ 使用 `eprintln!` 输出到 stderr

#### 集成测试
- ✅ `tests/pbft_integration_tests.rs` - PBFT 共识集成测试（10+ 测试）
- ✅ `tests/gossip_integration_tests.rs` - Gossip 同步集成测试（15+ 测试）
- ✅ `tests/async_commit_stress_tests.rs` - 异步提交压力测试（10+ 测试）

#### 代码质量修复
- ✅ 编译警告修复（`#![deny(warnings)]` 启用）

### 综合评分
**4.5/5 ⭐⭐⭐⭐⭐**（保持）

---

## [v0.4.0] - 2026-02-26

### 主题
多级缓存 + gRPC 支持 + PBFT 共识 + Gossip 同步 + 异步提交

### 新增功能

#### 1. 多级缓存架构（L1 CPU + L2 Disk + L3 Remote）
- ✅ `src/memory_layer/multi_level_cache.rs` (1002 行)
- L1 CPU 缓存：LRU 实现，1000 条目容量，< 1ms 延迟
- L2 磁盘缓存：持久化存储，100GB+ 容量，10-50ms 延迟
- L3 远程缓存：Redis/S3 支持（预留接口），TB+ 容量，100-500ms 延迟

#### 2. gRPC 支持
- ✅ `src/grpc/mod.rs` + `proto/node_rpc.proto`
- 完整的 gRPC 服务定义
- 跨节点 RPC 服务实现

#### 3. PBFT 共识框架
- ✅ `src/consensus/pbft.rs`
- Pre-prepare → Prepare → Commit 三阶段提交
- 2f+1 签名收集，支持拜占庭容错
- 视图切换机制（leader 故障）

**状态**: ⚠️ 原型（使用内存消息传递）

#### 4. Gossip 同步协议
- ✅ `src/gossip.rs`
- Vector Clock：因果排序和冲突检测
- Merkle Tree：数据完整性验证
- Gossip push/pull 机制

**状态**: ⚠️ 原型（使用内存模拟）

#### 5. 异步提交服务
- ✅ `src/services/async_commit_service.rs`
- Channel-based 异步提交
- 批处理优化
- 背压控制

### 综合评分
**4.5/5 ⭐⭐⭐⭐⭐**（从 4.0/5 提升）

---

## [v0.3.0] - 2026-02-26

### 主题
Async Memory Layer + Context Sharding + KV Cache 优化

### 新增功能

#### 1. Memory Layer 异步化
- ✅ `AsyncMemoryLayerManager` - 使用 `tokio::sync::RwLock`
- 全异步 API

**性能提升**:
- 并发写入 (10 线程): 5.0ms → 1.2ms (提升 76%)
- 并发读取 (10 线程): 3.0ms → 0.8ms (提升 73%)

#### 2. 上下文分片
- ✅ `src/memory_layer/context_sharding.rs`
- 均匀分割：每个分片包含大致相同数量的 tokens
- 轮询分配：分片轮流分配到不同节点
- 支持 100K+ tokens 跨节点

#### 3. KV Cache 优化 Phase 1-2
- ✅ `kv_chunk.rs` - Chunk-level 存储 (256 tokens/chunk)
- ✅ `kv_index.rs` - Bloom Filter 索引
- ✅ `async_storage.rs` - 异步存储后端
- ✅ `kv_compressor.rs` - zstd 压缩器 (93% 空间节省)
- ✅ `prefetcher.rs` - 智能预取器 (N-gram 模式)

**测试验证**: 42 个单元测试，100% 通过率

#### 4. Deprecated 代码迁移
- ✅ `coordinator.rs` 移至 `deprecated/` 目录
- ✅ 标记为 `#[deprecated]`

### 综合评分
**4.0/5 ⭐⭐⭐⭐**（从 3.5/5 提升）

---

## [v0.2.0] - 2026-02-26

### 主题
P11 锐评修复 - 从"学术玩具"到"可用原型"

### 核心修复

#### P0 生存问题

##### 1. 分布式计算能力 (0% → 60%)
- ✅ 实现 `LLMProvider` 集成真实 vLLM/SGLang HTTP API
- ✅ 使用 `reqwest` 异步 HTTP 客户端

##### 2. 网络通信能力 (0% → 70%)
- ✅ 创建 `RpcServer` 使用 `axum` Web 框架
- ✅ 实现 4 个核心端点

#### P1 质量问题

##### 1. 线程安全修复
- ✅ 移除 `Blockchain::Clone` 实现
- ✅ 统一使用 `Arc<RwLock<T>>` 模式
- ✅ 添加 100 线程并发测试验证

##### 2. God Object 拆分
- ✅ 拆分为 3 个单一职责服务
- ✅ `ArchitectureCoordinator` 标记为 deprecated

##### 3. 异步能力修复
- ✅ `InferenceProvider` trait 异步化
- ✅ HTTP 调用使用原生 async/await

##### 4. 错误处理重构
- ✅ 库层保留 thiserror
- ✅ 应用层迁移到 anyhow::Result

#### P2 体验问题

##### 1. 测试覆盖增强
- ✅ 100 线程并发测试
- ✅ 属性测试（proptest）
- ✅ 模糊测试

##### 2. 文档真实性改进
- ✅ 更新 `limitations.md` 反映真实状态
- ✅ 更新 `README.md` 标注 v0.2.0 生产就绪度

### 综合评分
**3.5/5 ⭐⭐⭐**（从 2.5/5 提升）

---

## [v0.1.0] - 初始版本

### 主题
区块链 + 分布式 LLM 推理架构原型

### 核心功能

- ✅ 基础区块链核心实现（区块、交易、哈希链）
- ✅ 双链架构设计（区块链 + 记忆链）
- ✅ 三层解耦架构（节点层、记忆层、提供商层）
- ✅ 基础信誉系统
- ✅ KV Cache 存证机制
- ✅ 质量评估框架

### 项目定位

- ✅ 架构演示
- ✅ 学习资源
- ⚠️ 原型验证
- ❌ 生产就绪

---

## 未来计划

### v0.6.0（计划中）

- [ ] P2P 网络层完整集成（libp2p GossipSub）
- [ ] 状态持久化（RocksDB）
- [ ] 3 节点多节点集成测试
- [ ] Prometheus + Grafana 监控

### v0.7.0（计划中）

- [ ] 共识机制升级（评估 tendermint-rs/hotstuff）
- [ ] 简化版 Raft 副本同步
- [ ] 完善混沌测试和长稳测试

### v1.0.0（愿景）

- [ ] Ring Attention（跨节点注意力机制）
- [ ] P2P KV 共享（跨节点 KV 复用）
- [ ] 删除 deprecated 代码
- [ ] 生产部署指南

---

*最后更新：2026-03-27*
*项目版本：v0.5.0*
