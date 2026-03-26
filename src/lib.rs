//! 分布式 KV 缓存系统 - 带哈希审计日志
//!
//! 本模块实现了一个高性能的分布式 KV 缓存系统，专为大模型推理场景设计：
//! - **核心功能**：分布式 KV 上下文存储，支持分片、压缩、多级缓存
//! - **审计日志**：KV 哈希存证，提供不可篡改的数据完整性验证
//! - **信誉系统**：节点信誉管理，支持可信调度
//!
//! # 架构概述
//!
//! 系统由三个主要组件构成：
//!
//! - **推理提供商层**：无状态计算单元，执行 LLM 推理（vLLM/SGLang）
//! - **记忆层**：分布式 KV 存储，支持 L1/L2/L3 三级缓存
//! - **审计日志层**：KV 哈希存证、信誉记录、共识结果
//!
//! # 核心特性
//!
//! - **高性能**：L1 缓存命中延迟 < 1ms，支持 100+ 线程并发访问
//! - **可验证**：所有 KV 数据都有哈希存证，支持完整性验证
//! - **可扩展**：支持多节点部署，共识阈值可配置
//! - **并发安全**：带超时的锁机制，避免死锁
//!
//! # 使用示例
//!
//! ## 基本 KV 存储
//!
//! ```
//! use block_chain_with_context::{MemoryLayerManager, AccessCredential, AccessType};
//!
//! // 创建记忆层管理器
//! let mut memory = MemoryLayerManager::new("node_1");
//!
//! // 创建访问凭证
//! let credential = AccessCredential {
//!     credential_id: "cred_1".to_string(),
//!     provider_id: "provider_1".to_string(),
//!     memory_block_ids: vec!["all".to_string()],
//!     access_type: AccessType::ReadWrite,
//!     expires_at: u64::MAX,
//!     issuer_node_id: "node_1".to_string(),
//!     signature: "sig".to_string(),
//!     is_revoked: false,
//! };
//!
//! // 写入 KV 数据
//! memory.write_kv("key".to_string(), b"value".to_vec(), &credential).unwrap();
//!
//! // 读取 KV 数据
//! let shard = memory.read_kv("key", &credential);
//! assert!(shard.is_some());
//! ```
//!
//! ## 配置管理（Builder 模式）
//!
//! ```
//! use block_chain_with_context::BlockchainConfig;
//!
//! // 使用 Builder 模式构建配置
//! let config = BlockchainConfig::builder()
//!     .trust_threshold(0.75)
//!     .inference_timeout_ms(30000)
//!     .max_retries(5)
//!     .log_level("info")
//!     .build()
//!     .expect("配置验证失败");
//! ```
//!
//! ## 审计日志（哈希存证）
//!
//! ```
//! use block_chain_with_context::{Blockchain, KvCacheProof};
//!
//! // 创建区块链（审计日志）
//! let mut blockchain = Blockchain::new("node_1".to_string());
//!
//! // 注册节点
//! blockchain.register_node("node_1".to_string());
//!
//! // 添加 KV 存证
//! let kv_proof = KvCacheProof::new(
//!     "kv_001".to_string(),
//!     "hash_123".to_string(),
//!     "node_1".to_string(),
//!     1024,
//! );
//! blockchain.add_kv_proof(kv_proof);
//! ```
//!
//! # 性能指标
//!
//! | 操作 | L1 命中 | L2 命中 | L3 命中 |
//! |------|--------|--------|--------|
//! | 读取延迟 | < 1ms | 10-50ms | 100-500ms |
//! | 写入延迟 | < 1ms | 10-50ms | 100-500ms |
//!
//! **数据来源**：`cargo bench` 基准测试报告
//!
//! # 测试
//!
//! ```bash
//! # 运行所有测试
//! cargo test
//!
//! # 运行并发测试
//! cargo test --test concurrency_tests -- --nocapture
//!
//! # 运行基准测试（需要 nightly）
//! cargo +nightly bench
//! ```
//!
//! # 相关文档
//!
//! - [开发者指南](docs/DEVELOPER_GUIDE.md)
//! - [架构文档](docs/ARCHITECTURE.md)
//! - [P11 锐评与修复](docs/P11_REVIEW.md)

// 启用严格警告模式
#![deny(warnings)]
#![warn(rust_2018_idioms)]

// PBFT 共识模块（原型）
pub mod consensus;

// 异步提交服务
pub mod async_commit;

// Gossip 同步协议（原型）
pub mod gossip;

// 核心模块
pub mod traits;
pub mod transaction;
pub mod metadata;
pub mod block;
pub mod blockchain;
pub mod error;
pub mod quality_assessment;
pub mod reputation;
pub mod utils;
pub mod storage;
pub mod audit;

// 三层架构模块
pub mod node_layer;
pub mod memory_layer;
pub mod provider_layer;
pub mod failover;

// 服务层模块
pub mod services;
pub mod service_bus;
pub mod orchestrator;

// 评估器模块
pub mod assessor_registry;
pub mod integrity_checker;
pub mod collusion_analyzer;
pub mod enhanced_reputation;
pub mod quality_scheduler;
pub mod validator_reputation;

// 验证模块
pub mod verifiable_package;
pub mod verification_sdk;
pub mod cli;

// 其他模块
pub mod metrics;
pub mod network_adapter;
pub mod persistence;

// 主程序
pub mod main;

// 重新导出常用类型
pub use blockchain::{Blockchain, BlockchainConfig, ConsensusEngine};
pub use block::{Block, KvCacheProof};
pub use transaction::{Transaction, TransactionType, TransactionPayload};
pub use metadata::BlockMetadata;
pub use memory_layer::{MemoryLayerManager, KvShard, KvProof};
pub use node_layer::{AccessCredential, AccessType};
pub use reputation::{ReputationManager, NodeReputation};
