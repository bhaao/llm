//! 网络适配器 - 将 gRPC 网络适配到 PBFT 和 Gossip
//!
//! **设计目标**：
//! - 不修改现有 PBFT/Gossip 核心逻辑
//! - 通过 trait 抽象网络层，支持切换不同实现
//! - 实现从内存模拟到真实网络的平滑迁移

use std::sync::Arc;
use tokio::sync::RwLock;
use log::debug;

// 导入网络层
#[cfg(feature = "grpc")]
use crate::network::{ConsensusNetwork as P2pConsensusNetwork, GossipNetwork as P2pGossipNetwork, GrpcNetwork, NetworkConfig};

// 导入 PBFT 消息类型
use crate::consensus::messages::SignedMessage;

// 导入 Gossip 消息类型
use crate::gossip::GossipMessage;

/// PBFT 共识网络接口 trait
///
/// 定义了 PBFT 共识所需的网络通信能力
#[tonic::async_trait]
pub trait ConsensusNetwork: Send + Sync {
    /// 广播消息到所有节点
    async fn broadcast(&self, message: &SignedMessage) -> Result<(), String>;
    
    /// 发送消息到指定节点
    async fn send_to(&self, target: &str, message: &SignedMessage) -> Result<(), String>;
}

/// Gossip 网络接口 trait
///
/// 定义了 Gossip 同步所需的网络通信能力
#[tonic::async_trait]
pub trait GossipNetwork: Send + Sync {
    /// 推送 Gossip 消息
    async fn gossip(&self, data: GossipMessage) -> Result<(), String>;
    
    /// 选择 Gossip peer（用于扇出）
    fn select_peers(&self, fanout: usize) -> Vec<String>;
}

/// 内存网络实现（用于测试）
pub struct MemoryNetwork {
    /// 节点 ID
    node_id: String,
}

impl MemoryNetwork {
    pub fn new(node_id: String) -> Self {
        MemoryNetwork { node_id }
    }
}

#[tonic::async_trait]
impl ConsensusNetwork for MemoryNetwork {
    async fn broadcast(&self, _message: &SignedMessage) -> Result<(), String> {
        // 内存模拟：直接返回成功
        debug!("MemoryNetwork: broadcast message from {}", self.node_id);
        Ok(())
    }

    async fn send_to(&self, _target: &str, _message: &SignedMessage) -> Result<(), String> {
        // 内存模拟：直接返回成功
        debug!("MemoryNetwork: send_to from {}", self.node_id);
        Ok(())
    }
}

#[tonic::async_trait]
impl GossipNetwork for MemoryNetwork {
    async fn gossip(&self, _data: GossipMessage) -> Result<(), String> {
        // 内存模拟：直接返回成功
        debug!("MemoryNetwork: gossip message from {}", self.node_id);
        Ok(())
    }

    fn select_peers(&self, _fanout: usize) -> Vec<String> {
        // 内存模拟：返回空列表
        Vec::new()
    }
}

/// gRPC 网络实现（生产环境）
#[cfg(feature = "grpc")]
pub struct GrpcConsensusNetwork {
    /// 底层 gRPC 网络
    inner: Arc<RwLock<GrpcNetwork>>,
    /// 节点 ID
    #[allow(dead_code)]
    node_id: String,
}

#[cfg(feature = "grpc")]
impl GrpcConsensusNetwork {
    /// 创建新的 gRPC 共识网络
    pub async fn new(config: NetworkConfig) -> Result<Self, String> {
        let node_id = config.node_id.clone();
        let network = GrpcNetwork::new(config)
            .await
            .map_err(|e| e.to_string())?;
        
        Ok(GrpcConsensusNetwork {
            inner: Arc::new(RwLock::new(network)),
            node_id,
        })
    }
}

#[tonic::async_trait]
#[cfg(feature = "grpc")]
impl ConsensusNetwork for GrpcConsensusNetwork {
    async fn broadcast(&self, message: &SignedMessage) -> Result<(), String> {
        use crate::network::consensus_proto::SignedPbftMessage;
        use crate::network::consensus_proto::PbftMessageType;
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        // 将 SignedMessage 转换为 SignedPbftMessage
        let pbft_message = match &message.message {
            crate::consensus::messages::PBFTMessage::PrePrepare { view, sequence, digest, leader: _, request: _ } => {
                // 直接序列化整个 PBFTMessage
                SignedPbftMessage {
                    message_type: PbftMessageType::PrePrepare as i32,
                    sender_id: message.sender_id.clone(),
                    view: *view,
                    sequence: *sequence,
                    digest: digest.clone(),
                    message_data: bincode::serialize(&message.message).map_err(|e| e.to_string())?,
                    signature: message.signature.clone(),
                    public_key: vec![],
                    timestamp,
                }
            }
            crate::consensus::messages::PBFTMessage::Prepare { view, sequence, digest, replica_id: _, signature } => {
                SignedPbftMessage {
                    message_type: PbftMessageType::Prepare as i32,
                    sender_id: message.sender_id.clone(),
                    view: *view,
                    sequence: *sequence,
                    digest: digest.clone(),
                    message_data: bincode::serialize(&message.message).map_err(|e| e.to_string())?,
                    signature: signature.clone(),
                    public_key: vec![],
                    timestamp,
                }
            }
            crate::consensus::messages::PBFTMessage::Commit { view, sequence, digest, replica_id: _, signature } => {
                SignedPbftMessage {
                    message_type: PbftMessageType::Commit as i32,
                    sender_id: message.sender_id.clone(),
                    view: *view,
                    sequence: *sequence,
                    digest: digest.clone(),
                    message_data: bincode::serialize(&message.message).map_err(|e| e.to_string())?,
                    signature: signature.clone(),
                    public_key: vec![],
                    timestamp,
                }
            }
            _ => {
                return Err(format!("Unsupported message type: {:?}", message.message_type()));
            }
        };
        
        let network = self.inner.read().await;
        let count = network.broadcast(&pbft_message).await
            .map_err(|e| e.to_string())?;
        
        debug!("Broadcast completed: {} nodes received", count);
        
        Ok(())
    }
    
    async fn send_to(&self, target: &str, message: &SignedMessage) -> Result<(), String> {
        use crate::network::consensus_proto::SignedPbftMessage;
        use crate::network::consensus_proto::PbftMessageType;
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        // 简化实现：仅支持 Commit 消息的点对点发送
        let pbft_message = match &message.message {
            crate::consensus::messages::PBFTMessage::Commit { view, sequence, digest, replica_id: _, signature } => {
                SignedPbftMessage {
                    message_type: PbftMessageType::Commit as i32,
                    sender_id: message.sender_id.clone(),
                    view: *view,
                    sequence: *sequence,
                    digest: digest.clone(),
                    message_data: bincode::serialize(&message.message).map_err(|e| e.to_string())?,
                    signature: signature.clone(),
                    public_key: vec![],
                    timestamp,
                }
            }
            _ => {
                return Err(format!("Unsupported message type for send_to: {:?}", message.message_type()));
            }
        };
        
        let network = self.inner.read().await;
        network.send_to(target, &pbft_message).await
            .map_err(|e| e.to_string())?;
        
        Ok(())
    }
}

/// gRPC Gossip 网络实现
#[cfg(feature = "grpc")]
pub struct GrpcGossipNetwork {
    /// 底层 gRPC 网络
    inner: Arc<RwLock<GrpcNetwork>>,
    /// 节点 ID
    #[allow(dead_code)]
    node_id: String,
    /// fanout 大小
    #[allow(dead_code)]
    fanout: usize,
}

#[cfg(feature = "grpc")]
impl GrpcGossipNetwork {
    /// 创建新的 gRPC Gossip 网络
    pub async fn new(config: NetworkConfig, fanout: usize) -> Result<Self, String> {
        let node_id = config.node_id.clone();
        let network = GrpcNetwork::new(config)
            .await
            .map_err(|e| e.to_string())?;
        
        Ok(GrpcGossipNetwork {
            inner: Arc::new(RwLock::new(network)),
            node_id,
            fanout,
        })
    }
}

#[tonic::async_trait]
#[cfg(feature = "grpc")]
impl GossipNetwork for GrpcGossipNetwork {
    async fn gossip(&self, data: GossipMessage) -> Result<(), String> {
        use crate::network::consensus_proto::GossipMessage as ProtoGossipMessage;
        use crate::network::consensus_proto::GossipMessageType;
        use crate::network::consensus_proto::VectorClock as ProtoVectorClock;
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        // 将 GossipMessage 转换为 ProtoGossipMessage
        let proto_message = ProtoGossipMessage {
            message_type: GossipMessageType::ShardSync as i32,
            sender_id: self.node_id.clone(),
            shard_id: data.shard_id.clone(),
            shard_data: bincode::serialize(&data.shard).map_err(|e| e.to_string())?,
            vector_clock: Some(ProtoVectorClock {
                clocks: data.vector_clock.get_clocks().iter()
                    .map(|(k, v)| (k.clone(), *v))
                    .collect(),
            }),
            merkle_proof: None,
            timestamp,
            signature: vec![],
        };
        
        let network = self.inner.read().await;
        network.push_gossip(&proto_message).await
            .map_err(|e| e.to_string())?;
        
        Ok(())
    }
    
    fn select_peers(&self, _fanout: usize) -> Vec<String> {
        // 简化实现：返回空列表
        // 实际实现中应该从 gRPC 网络获取节点列表
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_memory_network() {
        let network = MemoryNetwork::new("test_node".to_string());
        
        // 创建测试消息
        use crate::consensus::messages::PBFTMessage;
        use ed25519_dalek::SigningKey;
        use rand::random;
        
        let key_bytes: [u8; 32] = random();
        let signing_key = SigningKey::from_bytes(&key_bytes);
        
        let message = PBFTMessage::PrePrepare {
            view: 0,
            sequence: 1,
            digest: "test_digest".to_string(),
            leader: "test_node".to_string(),
            request: vec![1, 2, 3],
        };
        
        let signed = SignedMessage::new(message, "test_node".to_string(), &signing_key);
        
        // 测试广播
        let result = network.broadcast(&signed).await;
        assert!(result.is_ok());
    }
}
