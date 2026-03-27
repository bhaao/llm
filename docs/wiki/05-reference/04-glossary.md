# 术语表

> **最后更新**: 2026-03-26  
> **项目版本**: v0.5.0

---

## A

### Audit Layer (审计日志层)
三层架构的最底层，负责 KV 哈希存证、节点信誉管理、共识结果记录。不依赖其他两层。

### AccessCredential (访问凭证)
用于控制 KV 存储访问权限的凭证，包含凭证 ID、访问类型、有效期等信息。

---

## B

### Blockchain (区块链)
本项目中的审计日志层实现，用于存储 KV 哈希存证、元数据、信誉记录等。

### Bloom Filter (布隆过滤器)
用于 KV 索引的概率数据结构，支持 O(1) 时间复杂度的键存在性查询。

### Builder Pattern (Builder 模式)
用于构建复杂对象的设计模式，通过链式调用逐步配置对象属性。

---

## C

### Chunk-level Storage (Chunk 级存储)
将 KV 数据按固定大小（256 tokens）分块存储，相比 Block-level 更细粒度。

### Circuit Breaker (断路器)
故障模式，当连续失败达到阈值时自动熔断，防止级联故障。

### Consensus (共识)
多节点环境下达成一致的过程，本项目使用 PBFT 共识算法。

---

## D

### Data Sharding (数据分片)
将数据按某种规则分散到多个节点存储，支持水平扩展。

### Distributed KV Cache (分布式 KV 缓存)
跨多个节点分布的 KV 缓存系统，支持分片、多副本、一致性协议。

---

## E

### Exponential Map (指数映射)
李代数到李群的映射：exp: g → G

---

## F

### Failover (故障切换)
当主节点/服务失败时自动切换到备用节点/服务的机制。

---

## G

### Gossip Protocol (Gossip 协议)
节点间随机交换信息的协议，用于数据同步和成员发现。

### Geometric Mean (几何平均)
李群聚合公式：G = exp(1/N * Σlog(g_i))

### gRPC
基于 Protocol Buffers 的 RPC 框架，用于跨节点通信。

---

## H

### Hash Chain (哈希链)
每个区块包含前一个区块的哈希，形成不可篡改的链式结构。

---

## I

### Inference Orchestration (推理编排)
协调多个组件完成 LLM 推理请求的过程。

---

## K

### KV Cache (KV 缓存)
存储 LLM 推理中间结果（Key-Value）的缓存，用于复用上下文。

### KvCacheProof (KV 存证)
KV 数据的哈希证明，用于区块链存证。

---

## L

### L1 Cache (L1 缓存)
第一级缓存，位于 CPU 内存，延迟 < 1ms。

### L2 Cache (L2 缓存)
第二级缓存，位于磁盘，延迟 10-50ms。

### L3 Cache (L3 缓存)
第三级缓存，位于远程 Redis，延迟 100-500ms。

### Lie Algebra (李代数)
李群在单位元处的切空间，向量空间结构。

### Lie Group (李群)
具有群结构的流形，连续对称性的数学描述。

### Lie Group Aggregator (李群聚合器)
将多个李群元素聚合为一个的组件，信任根所在。

### Lie Group Metric (李群度量)
计算两个李群元素距离的工具，用于 QaaS 验证。

### Logarithmic Map (对数映射)
李群到李代数的映射：log: G → g

---

## M

### Memory Layer (记忆层)
三层架构的中间层，负责 KV Cache 存储、分片、压缩、多副本。

### MemoryChain (记忆链)
记忆层中的数据结构，链式存储 KV 数据及其哈希。

### Multi-level Cache (多级缓存)
L1 + L2 + L3 三层缓存架构，平衡性能和成本。

---

## N

### Node Layer (节点层)
管理节点身份、访问凭证、信誉系统的模块。

---

## P

### PBFT (Practical Byzantine Fault Tolerance)
实用拜占庭容错算法，支持 f 个恶意节点的容错（需要 3f+1 节点）。

### Prefetching (预取)
根据访问模式提前加载数据到缓存，减少延迟。

### Provider Layer (提供商层)
三层架构的最上层，负责 LLM 推理执行、提供商管理。

---

## Q

### QaaS (Quality as a Service)
质量验证服务，使用李群度量评估推理输出质量。

---

## R

### RAII (Resource Acquisition Is Initialization)
Rust 资源管理模式，通过所有权系统自动管理资源生命周期。

### Raft Consensus (Raft 共识)
一种易于理解的分布式共识算法，用于日志复制。

### Redis
开源内存数据存储，用作 L3 缓存。

### Rust
系统编程语言，内存安全、高性能。

---

## S

### Service Layer (服务层)
应用层服务，包括 InferenceOrchestrator、CommitmentService、FailoverService 等。

---

## T

### Tiered Storage (分层存储)
L1/L2/L3 多级存储架构，按访问频率自动迁移数据。

### Trust Root (信任根)
系统可信性的基础，本项目中信任根在李群聚合公式。

---

## V

### Validator (验证者)
参与共识过程的节点，负责验证和签名。

### vLLM
高效的 LLM 推理引擎，支持 PagedAttention 等优化。

---

## Z

### zstd
Zstandard 压缩算法，Facebook 开发的高性能压缩器。

---

*最后更新：2026-03-26*  
*项目版本：v0.5.0*
