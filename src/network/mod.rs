//! 网络层模块
//!
//! 提供真实的 P2P 网络通信能力，支持：
//! - PBFT 共识消息广播
//! - Gossip 数据同步
//! - 节点发现和心跳
//!
//! **网络实现**：
//! - `p2p_network`: 基于 gRPC 的网络实现（生产环境）
//! - `libp2p_network`: 基于 libp2p 的网络实现（P2P 原生，简化版）
//!
//! **使用指南**：
//! - 默认使用 gRPC 网络（需要 protoc 编译器）
//! - 启用 `p2p` 特性使用 libp2p 网络（推荐用于去中心化部署）
//!
//! 详细集成指南请参阅 `docs/LIBP2P_INTEGRATION_GUIDE.md`

pub mod p2p_network;

#[cfg(feature = "p2p")]
pub mod libp2p_network;

pub use p2p_network::*;

#[cfg(feature = "p2p")]
pub use libp2p_network::*;
