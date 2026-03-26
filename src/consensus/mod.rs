//! PBFT 共识模块 - 拜占庭容错共识实现
//!
//! **架构定位**：
//! - 从"信任"转向"验证"（Trust but Verify → Verify Everything）
//! - 假设所有节点都是坏人，但还能达成共识
//! - 实现真正的三阶段提交 + 视图切换
//!
//! **核心特性**：
//! - Pre-prepare → Prepare → Commit 三阶段提交
//! - 2f+1 签名收集防伪造
//! - 视图切换处理 Leader 作恶
//! - Checkpoint 机制支持垃圾回收
//!
//! ⚠️ **生产就绪度说明**
//!
//! 当前实现是**简化版 PBFT 共识原型**，适用于：
//! - ✅ 架构验证/技术演示
//! - ✅ 学习 PBFT 共识机制
//! - ✅ 小规模联盟链测试（≤5 节点）
//!
//! ❌ **当前实现与生产级的差距**：
//! - 缺少 P2P 网络层（消息通过内存传递，非真实网络广播）
//! - 缺少持久化（节点重启后共识状态丢失）
//! - 缺少视图切换的完整实现（仅基础框架）
//! - 缺少消息重传和超时重试机制
//!
//! 🔧 **生产环境建议**：
//! - 集成成熟共识库：[tendermint-rs](https://github.com/penumbra-zone/tendermint-rs) 或 [hotstuff](https://github.com/hotstuff/hotstuff)
//! - 实现 P2P 广播层：使用 [libp2p](https://libp2p.io/) 或 [rust-libp2p](https://github.com/libp2p/rust-libp2p)
//! - 添加状态持久化：使用 RocksDB 或 Redis 存储共识状态
//!
//! **模块结构**：
//! - `pbft`: PBFT 共识核心实现
//! - `messages`: 共识消息类型
//! - `certificate`: 证书和签名收集
//! - `view_change`: 视图切换机制

pub mod messages;
pub mod certificate;
pub mod view_change;
pub mod pbft;
pub mod quality_aware_consensus;

// 重新导出主要类型
pub use messages::{PBFTMessage, SignedMessage, MessageType};
pub use certificate::{QuorumCertificate, CertificateError};
pub use view_change::{ViewChangeManager, ViewChangeState};
pub use pbft::{PBFTConsensus, ConsensusState, ConsensusConfig};
pub use quality_aware_consensus::{
    QualityAwareConsensusManager, QualityConsensusConfig,
    QualityVote, QualityVoteBuilder, VoteStatus,
    ConsensusDecision, WeightedVoteResult,
};
