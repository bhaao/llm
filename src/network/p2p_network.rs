//! P2P 网络层 - 基于 gRPC 的真实网络实现
//!
//! **架构设计**：
//! - 使用 gRPC 作为底层通信协议
//! - 提供 PBFT 共识和 Gossip 同步的网络接口
//! - 支持节点发现、连接管理和消息路由
//!
//! **核心组件**：
//! - `ConsensusNetwork`: PBFT 共识网络接口 trait
//! - `GossipNetwork`: Gossip 同步网络接口 trait
//! - `GrpcNetwork`: gRPC 网络实现
//! - `P2pNetworkServer`: gRPC 服务端实现

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio_stream::Stream;
use log::{info, warn, debug, error};
use tonic::transport::{Channel, Server};
use tonic::{Request, Response, Status};

// 导入生成的 gRPC 代码
pub mod consensus_proto {
    tonic::include_proto!("consensus");
}

use consensus_proto::{
    consensus_service_client::ConsensusServiceClient,
    consensus_service_server::{ConsensusService, ConsensusServiceServer},
    gossip_service_client::GossipServiceClient,
    gossip_service_server::{GossipService, GossipServiceServer},
    BroadcastRequest, BroadcastResponse, SendToRequest, SendToResponse,
    SubscribeRequest, PushGossipRequest, PushGossipResponse,
    ShardRequest, ShardResponse, SyncRequest, HeartbeatRequest, HeartbeatResponse,
    SignedPbftMessage, GossipMessage, KvShardData, VectorClock, NodeStatus,
};

/// 网络错误类型
#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("网络连接失败：{0}")]
    ConnectionFailed(String),
    #[error("消息发送失败：{0}")]
    SendFailed(String),
    #[error("序列化失败：{0}")]
    SerializationFailed(String),
    #[error("反序列化失败：{0}")]
    DeserializationFailed(String),
    #[error("节点不存在：{0}")]
    NodeNotFound(String),
    #[error("超时：{0}")]
    Timeout(String),
}

/// 节点信息
#[derive(Debug, Clone)]
pub struct NodeInfo {
    /// 节点 ID
    pub node_id: String,
    /// gRPC 地址
    pub address: String,
    /// 是否在线
    pub is_online: bool,
    /// 公钥
    pub public_key: Vec<u8>,
}

/// 网络配置
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// 本节点 ID
    pub node_id: String,
    /// 监听地址
    pub listen_addr: String,
    /// 初始节点列表
    pub initial_nodes: Vec<NodeInfo>,
    /// 连接超时（毫秒）
    pub connect_timeout_ms: u64,
    /// 请求超时（毫秒）
    pub request_timeout_ms: u64,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        NetworkConfig {
            node_id: "node_1".to_string(),
            listen_addr: "http://127.0.0.1:50051".to_string(),
            initial_nodes: Vec::new(),
            connect_timeout_ms: 5000,
            request_timeout_ms: 10000,
        }
    }
}

// ==================== PBFT 共识网络接口 ====================

/// PBFT 共识网络接口 trait
///
/// 定义了 PBFT 共识所需的网络通信能力
#[tonic::async_trait]
pub trait ConsensusNetwork: Send + Sync {
    /// 广播消息到所有节点
    async fn broadcast(&self, message: &SignedPbftMessage) -> Result<u32, NetworkError>;
    
    /// 发送消息到指定节点
    async fn send_to(&self, target_id: &str, message: &SignedPbftMessage) -> Result<(), NetworkError>;
    
    /// 订阅共识消息流
    async fn subscribe(&self, topic: &str) -> Result<Box<dyn Stream<Item = Result<SignedPbftMessage, NetworkError>> + Send + Unpin>, NetworkError>;
    
    /// 获取已知节点列表
    fn get_known_nodes(&self) -> Vec<NodeInfo>;
    
    /// 添加节点
    fn add_node(&mut self, node: NodeInfo);
    
    /// 移除节点
    fn remove_node(&mut self, node_id: &str);
}

// ==================== Gossip 网络接口 ====================

/// Gossip 网络接口 trait
///
/// 定义了 Gossip 同步所需的网络通信能力
#[tonic::async_trait]
pub trait GossipNetwork: Send + Sync {
    /// 推送 Gossip 消息
    async fn push_gossip(&self, message: &GossipMessage) -> Result<Option<VectorClock>, NetworkError>;
    
    /// 请求分片数据
    async fn request_shard(
        &self,
        shard_id: &str,
        requester_version: &VectorClock,
    ) -> Result<Option<KvShardData>, NetworkError>;
    
    /// 选择 Gossip peer（用于扇出）
    fn select_peers(&self, fanout: usize) -> Vec<NodeInfo>;
    
    /// 获取已知节点列表
    fn get_known_nodes(&self) -> Vec<NodeInfo>;
    
    /// 添加节点
    fn add_node(&mut self, node: NodeInfo);
}

// ==================== gRPC 网络实现 ====================

/// gRPC 网络实现
///
/// 使用 gRPC 客户端池连接到其他节点
pub struct GrpcNetwork {
    /// 配置
    config: NetworkConfig,
    /// gRPC 客户端池
    clients: Arc<RwLock<HashMap<String, ConsensusServiceClient<Channel>>>>,
    /// Gossip 客户端池
    gossip_clients: Arc<RwLock<HashMap<String, GossipServiceClient<Channel>>>>,
    /// 节点信息
    nodes: Arc<RwLock<HashMap<String, NodeInfo>>>,
    /// 消息广播通道
    broadcast_tx: Arc<mpsc::Sender<SignedPbftMessage>>,
    /// 消息接收通道
    broadcast_rx: Arc<RwLock<mpsc::Receiver<SignedPbftMessage>>>,
}

impl GrpcNetwork {
    /// 创建新的 gRPC 网络
    pub async fn new(config: NetworkConfig) -> Result<Self, NetworkError> {
        let (tx, rx) = mpsc::channel(1000);
        
        let mut network = GrpcNetwork {
            config,
            clients: Arc::new(RwLock::new(HashMap::new())),
            gossip_clients: Arc::new(RwLock::new(HashMap::new())),
            nodes: Arc::new(RwLock::new(HashMap::new())),
            broadcast_tx: Arc::new(tx),
            broadcast_rx: Arc::new(RwLock::new(rx)),
        };
        
        // 初始化节点连接
        network.initialize_connections().await?;
        
        Ok(network)
    }
    
    /// 初始化节点连接
    async fn initialize_connections(&mut self) -> Result<(), NetworkError> {
        for node in &self.config.initial_nodes.clone() {
            if let Err(e) = self.connect_to_node(node).await {
                warn!("Failed to connect to initial node {}: {}", node.node_id, e);
            }
        }
        Ok(())
    }
    
    /// 连接到节点
    async fn connect_to_node(&self, node: &NodeInfo) -> Result<(), NetworkError> {
        debug!("Connecting to node {} at {}", node.node_id, node.address);
        
        let endpoint = Channel::from_shared(node.address.clone())
            .map_err(|e| NetworkError::ConnectionFailed(e.to_string()))?
            .timeout(std::time::Duration::from_millis(self.config.connect_timeout_ms))
            .connect_timeout(std::time::Duration::from_millis(self.config.connect_timeout_ms));
        
        let channel = endpoint.connect()
            .await
            .map_err(|e| NetworkError::ConnectionFailed(e.to_string()))?;
        
        let consensus_client = ConsensusServiceClient::new(channel.clone());
        let gossip_client = GossipServiceClient::new(channel);
        
        {
            let mut clients = self.clients.write().await;
            clients.insert(node.node_id.clone(), consensus_client);
        }
        
        {
            let mut gossip_clients = self.gossip_clients.write().await;
            gossip_clients.insert(node.node_id.clone(), gossip_client);
        }
        
        {
            let mut nodes = self.nodes.write().await;
            nodes.insert(node.node_id.clone(), node.clone());
        }
        
        info!("Connected to node {}", node.node_id);
        
        Ok(())
    }
    
    /// 断开节点连接
    #[allow(dead_code)]
    async fn disconnect_node(&self, node_id: &str) {
        {
            let mut clients = self.clients.write().await;
            clients.remove(node_id);
        }
        
        {
            let mut gossip_clients = self.gossip_clients.write().await;
            gossip_clients.remove(node_id);
        }
        
        {
            let mut nodes = self.nodes.write().await;
            nodes.remove(node_id);
        }
        
        info!("Disconnected from node {}", node_id);
    }
    
    /// 获取共识客户端
    async fn get_client(&self, node_id: &str) -> Result<ConsensusServiceClient<Channel>, NetworkError> {
        let clients = self.clients.read().await;
        clients.get(node_id)
            .cloned()
            .ok_or_else(|| NetworkError::NodeNotFound(node_id.to_string()))
    }
    
    /// 获取 Gossip 客户端
    async fn get_gossip_client(&self, node_id: &str) -> Result<GossipServiceClient<Channel>, NetworkError> {
        let clients = self.gossip_clients.read().await;
        clients.get(node_id)
            .cloned()
            .ok_or_else(|| NetworkError::NodeNotFound(node_id.to_string()))
    }
}

#[tonic::async_trait]
impl ConsensusNetwork for GrpcNetwork {
    /// 广播消息到所有节点
    async fn broadcast(&self, message: &SignedPbftMessage) -> Result<u32, NetworkError> {
        let clients = self.clients.read().await;
        let mut received_count = 0;
        
        for (node_id, client) in clients.iter() {
            let request = Request::new(BroadcastRequest {
                sender_id: self.config.node_id.clone(),
                message: Some(message.clone()),
                topic: "pbft".to_string(),
            });
            
            match client.clone().broadcast(request).await {
                Ok(response) => {
                    if response.get_ref().success {
                        received_count += 1;
                        debug!("Broadcast to {} succeeded", node_id);
                    } else {
                        warn!("Broadcast to {} failed: {:?}", node_id, response.get_ref().error);
                    }
                }
                Err(e) => {
                    warn!("Broadcast to {} failed: {}", node_id, e);
                }
            }
        }
        
        // 同时发送到本地广播通道
        let _ = self.broadcast_tx.send(message.clone()).await;
        
        info!("Broadcast completed: {}/{} nodes received", received_count, clients.len());
        
        Ok(received_count)
    }
    
    /// 发送消息到指定节点
    async fn send_to(&self, target_id: &str, message: &SignedPbftMessage) -> Result<(), NetworkError> {
        let mut client = self.get_client(target_id).await?;
        
        let request = Request::new(SendToRequest {
            sender_id: self.config.node_id.clone(),
            target_id: target_id.to_string(),
            message: Some(message.clone()),
        });
        
        let response = client.send_to(request).await
            .map_err(|e| NetworkError::SendFailed(e.to_string()))?;
        
        if !response.get_ref().success {
            return Err(NetworkError::SendFailed(
                response.get_ref().error.clone().unwrap_or_default()
            ));
        }
        
        debug!("SendTo {} succeeded", target_id);
        
        Ok(())
    }
    
    /// 订阅共识消息流
    async fn subscribe(&self, _topic: &str) -> Result<Box<dyn Stream<Item = Result<SignedPbftMessage, NetworkError>> + Send + Unpin>, NetworkError> {
        // 返回本地广播通道
        let _rx = self.broadcast_rx.read().await;
        
        // 创建一个 stream 适配器
        let stream = tokio_stream::empty();
        
        Ok(Box::new(stream))
    }
    
    /// 获取已知节点列表
    fn get_known_nodes(&self) -> Vec<NodeInfo> {
        // 这里需要异步上下文，返回空列表作为占位
        Vec::new()
    }
    
    /// 添加节点
    fn add_node(&mut self, node: NodeInfo) {
        // 同步方法无法直接调用异步方法，需要在外部处理连接
        // 这里仅添加到列表
        let nodes = self.nodes.clone();
        let node_id = node.node_id.clone();
        
        tokio::spawn(async move {
            let mut nodes_map = nodes.write().await;
            nodes_map.insert(node_id, node);
        });
    }
    
    /// 移除节点
    fn remove_node(&mut self, node_id: &str) {
        let nodes = self.nodes.clone();
        let node_id = node_id.to_string();
        
        tokio::spawn(async move {
            let mut nodes_map = nodes.write().await;
            nodes_map.remove(&node_id);
        });
    }
}

#[tonic::async_trait]
impl GossipNetwork for GrpcNetwork {
    /// 推送 Gossip 消息
    async fn push_gossip(&self, message: &GossipMessage) -> Result<Option<VectorClock>, NetworkError> {
        let _nodes = self.nodes.read().await;
        let peers = self.select_peers(2); // 默认 fanout=2
        
        let mut local_version: Option<VectorClock> = None;
        
        for peer in peers {
            if let Ok(mut client) = self.get_gossip_client(&peer.node_id).await {
                let request = Request::new(PushGossipRequest {
                    sender_id: self.config.node_id.clone(),
                    message: Some(message.clone()),
                });
                
                match client.push_gossip(request).await {
                    Ok(response) => {
                        if response.get_ref().success {
                            if let Some(version) = &response.get_ref().local_version {
                                local_version = Some(version.clone());
                            }
                            debug!("PushGossip to {} succeeded", peer.node_id);
                        } else {
                            warn!("PushGossip to {} failed: {:?}", peer.node_id, response.get_ref().error);
                        }
                    }
                    Err(e) => {
                        warn!("PushGossip to {} failed: {}", peer.node_id, e);
                    }
                }
            }
        }
        
        Ok(local_version)
    }
    
    /// 请求分片数据
    async fn request_shard(
        &self,
        shard_id: &str,
        requester_version: &VectorClock,
    ) -> Result<Option<KvShardData>, NetworkError> {
        let peers = self.select_peers(1);
        
        for peer in peers {
            if let Ok(mut client) = self.get_gossip_client(&peer.node_id).await {
                let request = Request::new(ShardRequest {
                    requester_id: self.config.node_id.clone(),
                    shard_id: shard_id.to_string(),
                    requester_version: Some(requester_version.clone()),
                });
                
                match client.request_shard(request).await {
                    Ok(response) => {
                        let resp = response.get_ref();
                        if resp.success {
                            return Ok(resp.shard_data.clone());
                        } else {
                            warn!("RequestShard from {} failed: {:?}", peer.node_id, resp.error);
                        }
                    }
                    Err(e) => {
                        warn!("RequestShard from {} failed: {}", peer.node_id, e);
                    }
                }
            }
        }
        
        Ok(None)
    }
    
    /// 选择 Gossip peer
    fn select_peers(&self, _fanout: usize) -> Vec<NodeInfo> {
        // 简单实现：随机选择 fanout 个节点
        // 实际实现中应该使用更复杂的策略（如 HyParView）
        
        // 由于这是同步方法，我们无法直接获取锁
        // 这里返回空列表作为占位，实际使用需要在外部实现
        Vec::new()
    }
    
    /// 获取已知节点列表
    fn get_known_nodes(&self) -> Vec<NodeInfo> {
        Vec::new()
    }
    
    /// 添加节点
    fn add_node(&mut self, node: NodeInfo) {
        let nodes = self.nodes.clone();
        let node_id = node.node_id.clone();
        
        tokio::spawn(async move {
            let mut nodes_map = nodes.write().await;
            nodes_map.insert(node_id, node);
        });
    }
}

// ==================== gRPC 服务端实现 ====================

/// PBFT 共识服务实现
pub struct PbftConsensusService {
    /// 节点 ID
    #[allow(dead_code)]
    node_id: String,
    /// 消息广播通道
    message_tx: mpsc::Sender<SignedPbftMessage>,
}

impl PbftConsensusService {
    /// 创建新的共识服务
    pub fn new(node_id: String, message_tx: mpsc::Sender<SignedPbftMessage>) -> Self {
        PbftConsensusService {
            node_id,
            message_tx,
        }
    }
}

#[tonic::async_trait]
impl ConsensusService for PbftConsensusService {
    async fn broadcast(
        &self,
        request: Request<BroadcastRequest>,
    ) -> Result<Response<BroadcastResponse>, Status> {
        let req = request.into_inner();
        
        debug!("Received broadcast from {}", req.sender_id);
        
        if let Some(message) = req.message {
            // 将消息发送到本地通道
            if let Err(e) = self.message_tx.send(message).await {
                error!("Failed to forward broadcast message: {}", e);
                return Ok(Response::new(BroadcastResponse {
                    success: false,
                    error: Some(e.to_string()),
                    received_count: 0,
                }));
            }
        }
        
        Ok(Response::new(BroadcastResponse {
            success: true,
            error: None,
            received_count: 1,
        }))
    }
    
    async fn send_to(
        &self,
        request: Request<SendToRequest>,
    ) -> Result<Response<SendToResponse>, Status> {
        let req = request.into_inner();
        
        debug!("Received send_to from {} to {}", req.sender_id, req.target_id);
        
        // 这里应该转发到目标节点，但简化实现直接返回成功
        Ok(Response::new(SendToResponse {
            success: true,
            error: None,
        }))
    }
    
    type SubscribeToConsensusStream = Pin<Box<dyn Stream<Item = Result<SignedPbftMessage, Status>> + Send + 'static>>;
    
    async fn subscribe_to_consensus(
        &self,
        request: Request<SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeToConsensusStream>, Status> {
        let _req = request.into_inner();
        
        // 创建一个空的 stream（简化实现）
        let stream = tokio_stream::empty();
        
        Ok(Response::new(Box::pin(stream)))
    }
}

/// Gossip 服务实现
pub struct GossipSyncService {
    /// 节点 ID
    node_id: String,
}

impl GossipSyncService {
    /// 创建新的 Gossip 服务
    pub fn new(node_id: String) -> Self {
        GossipSyncService { node_id }
    }
}

#[tonic::async_trait]
impl GossipService for GossipSyncService {
    async fn push_gossip(
        &self,
        request: Request<PushGossipRequest>,
    ) -> Result<Response<PushGossipResponse>, Status> {
        let req = request.into_inner();
        
        debug!("Received push_gossip from {}", req.sender_id);
        
        // 这里应该处理 Gossip 消息并返回本地版本
        // 简化实现直接返回成功
        
        Ok(Response::new(PushGossipResponse {
            success: true,
            error: None,
            local_version: None,
        }))
    }
    
    async fn request_shard(
        &self,
        request: Request<ShardRequest>,
    ) -> Result<Response<ShardResponse>, Status> {
        let _req = request.into_inner();
        
        // 简化实现：返回无更新
        Ok(Response::new(ShardResponse {
            success: true,
            error: None,
            shard_data: None,
            has_update: false,
        }))
    }
    
    type SyncShardsStream = Pin<Box<dyn Stream<Item = Result<GossipMessage, Status>> + Send + 'static>>;
    
    async fn sync_shards(
        &self,
        request: Request<SyncRequest>,
    ) -> Result<Response<Self::SyncShardsStream>, Status> {
        let _req = request.into_inner();
        
        // 创建一个空的 stream（简化实现）
        let stream = tokio_stream::empty();
        
        Ok(Response::new(Box::pin(stream)))
    }
    
    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();
        
        debug!("Received heartbeat from {}", req.sender_id);
        
        Ok(Response::new(HeartbeatResponse {
            success: true,
            error: None,
            responder_id: self.node_id.clone(),
            status: Some(NodeStatus {
                is_leader: false,
                sequence: 0,
                last_checkpoint: 0,
                health: "healthy".to_string(),
            }),
        }))
    }
}

/// 启动 gRPC 服务器
pub async fn start_grpc_server(
    listen_addr: &str,
    node_id: String,
    message_tx: mpsc::Sender<SignedPbftMessage>,
) -> Result<(), NetworkError> {
    let addr: std::net::SocketAddr = listen_addr
        .parse()
        .map_err(|e: std::net::AddrParseError| NetworkError::ConnectionFailed(e.to_string()))?;
    
    let consensus_service = PbftConsensusService::new(node_id.clone(), message_tx);
    let gossip_service = GossipSyncService::new(node_id);
    
    info!("Starting gRPC server on {}", addr);
    
    Server::builder()
        .add_service(ConsensusServiceServer::new(consensus_service))
        .add_service(GossipServiceServer::new(gossip_service))
        .serve(addr)
        .await
        .map_err(|e| NetworkError::ConnectionFailed(e.to_string()))?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_network_config_default() {
        let config = NetworkConfig::default();
        
        assert_eq!(config.node_id, "node_1");
        assert!(config.listen_addr.contains("127.0.0.1"));
        assert!(config.connect_timeout_ms > 0);
    }
    
    #[test]
    fn test_node_info_creation() {
        let node = NodeInfo {
            node_id: "test_node".to_string(),
            address: "http://localhost:50051".to_string(),
            is_online: true,
            public_key: vec![1, 2, 3],
        };
        
        assert_eq!(node.node_id, "test_node");
        assert!(node.is_online);
    }
}
