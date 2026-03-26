//! 服务总线模块 - 服务间通信基础设施
//!
//! **设计目标**：
//! - 提供服务间解耦通信机制
//! - 支持发布/订阅模式
//! - 支持请求/响应模式
//! - 支持异步消息传递
//!
//! **核心概念**：
//! - **Channel** - 消息通道，支持多生产者多消费者
//! - **Event** - 事件消息，携带事件类型和数据
//! - **Bus** - 服务总线，管理所有通道和消息路由

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use serde::{Serialize, Deserialize};
use anyhow::{Result, Context};
use crate::audit::AuditEvent;
use crate::consensus::messages::Operation;
use crate::quality_assessment::{QualityProof, QualityAssessmentRequest, QualityAssessmentResponse};
use crate::provider_layer::{InferenceRequest, InferenceResponse};
use crate::block::KvCacheProof;
use crate::metadata::BlockMetadata;

/// 事件类型枚举 - 定义所有可广播的事件
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EventType {
    /// 推理请求提交
    InferenceRequested,
    /// 推理完成
    InferenceCompleted,
    /// 质量验证请求
    QualityVerificationRequested,
    /// 质量验证完成
    QualityVerificationCompleted,
    /// 共识提案提交
    ConsensusProposed,
    /// 共识达成
    ConsensusReached,
    /// 区块链存证提交
    AttestationSubmitted,
    /// 区块链存证完成
    AttestationCompleted,
    /// 节点故障切换
    FailoverTriggered,
    /// 审计事件记录
    AuditEventRecorded,
    /// 自定义事件
    Custom(String),
}

/// 服务总线消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusMessage {
    /// 事件类型
    pub event_type: EventType,
    /// 消息来源服务 ID
    pub source: String,
    /// 消息目标服务 ID（可选，None 表示广播）
    pub target: Option<String>,
    /// 消息负载
    pub payload: MessagePayload,
    /// 时间戳
    pub timestamp: u64,
    /// 消息 ID（用于追踪）
    pub message_id: String,
}

/// 消息负载 - 支持多种数据类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum MessagePayload {
    /// 推理请求
    InferenceRequest(InferenceRequest),
    /// 推理响应
    InferenceResponse(InferenceResponse),
    /// 质量验证请求
    QualityRequest(QualityAssessmentRequest),
    /// 质量验证响应
    QualityResponse(QualityAssessmentResponse),
    /// 质量证明
    QualityProof(QualityProof),
    /// 共识操作
    ConsensusOperation(Operation),
    /// KV 存证
    KvProof(KvCacheProof),
    /// 区块元数据
    BlockMetadata(BlockMetadata),
    /// 审计事件
    AuditEvent(AuditEvent),
    /// 字符串数据
    Text(String),
    /// 字节数据
    Binary(Vec<u8>),
    /// JSON 数据
    Json(serde_json::Value),
    /// 空负载
    None,
}

impl BusMessage {
    /// 创建新消息
    pub fn new(
        event_type: EventType,
        source: String,
        target: Option<String>,
        payload: MessagePayload,
    ) -> Self {
        BusMessage {
            event_type,
            source,
            target,
            payload,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            message_id: uuid::Uuid::new_v4().to_string(),
        }
    }

    /// 创建广播消息
    pub fn broadcast(event_type: EventType, source: String, payload: MessagePayload) -> Self {
        Self::new(event_type, source, None, payload)
    }

    /// 创建点对点消息
    pub fn unicast(event_type: EventType, source: String, target: String, payload: MessagePayload) -> Self {
        Self::new(event_type, source, Some(target), payload)
    }
}

/// 请求/响应对
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestResponsePair {
    /// 请求消息
    pub request: BusMessage,
    /// 响应消息
    pub response: Option<BusMessage>,
    /// 是否已完成
    pub completed: bool,
}

/// 服务总线配置
#[derive(Debug, Clone)]
pub struct ServiceBusConfig {
    /// 广播通道容量
    pub broadcast_capacity: usize,
    /// 点对点通道容量
    pub unicast_capacity: usize,
    /// 请求超时时间（秒）
    pub request_timeout_secs: u64,
    /// 是否启用消息日志
    pub enable_message_log: bool,
    /// 最大消息日志大小
    pub max_message_log_size: usize,
}

impl Default for ServiceBusConfig {
    fn default() -> Self {
        ServiceBusConfig {
            broadcast_capacity: 1024,
            unicast_capacity: 256,
            request_timeout_secs: 30,
            enable_message_log: true,
            max_message_log_size: 10000,
        }
    }
}

/// 服务总线 - 核心通信基础设施
///
/// **功能**：
/// - 广播消息到所有订阅者
/// - 点对点消息路由
/// - 请求/响应模式支持
/// - 消息日志和审计追踪
pub struct ServiceBus {
    /// 广播发送者
    broadcast_tx: broadcast::Sender<BusMessage>,
    /// 点对点通道映射 (target_id -> mpsc::Sender)
    unicast_channels: Arc<RwLock<HashMap<String, mpsc::Sender<BusMessage>>>>,
    /// 请求/响应追踪 (message_id -> RequestResponsePair)
    pending_requests: Arc<RwLock<HashMap<String, RequestResponsePair>>>,
    /// 消息日志（用于审计）
    message_log: Arc<RwLock<Vec<BusMessage>>>,
    /// 配置
    config: ServiceBusConfig,
    /// 服务注册表 (service_id -> service_info)
    service_registry: Arc<RwLock<HashMap<String, ServiceInfo>>>,
}

/// 服务信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    /// 服务 ID
    pub service_id: String,
    /// 服务类型
    pub service_type: String,
    /// 服务地址/端点
    pub endpoint: String,
    /// 是否健康
    pub is_healthy: bool,
    /// 注册时间戳
    pub registered_at: u64,
}

impl ServiceBus {
    /// 创建新的服务总线
    pub fn new(config: ServiceBusConfig) -> Self {
        let (broadcast_tx, _) = broadcast::channel(config.broadcast_capacity);
        
        ServiceBus {
            broadcast_tx,
            unicast_channels: Arc::new(RwLock::new(HashMap::new())),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            message_log: Arc::new(RwLock::new(Vec::new())),
            config,
            service_registry: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 创建默认配置的服务总线
    pub fn with_defaults() -> Self {
        Self::new(ServiceBusConfig::default())
    }

    /// 注册服务
    pub async fn register_service(&self, info: ServiceInfo) -> Result<()> {
        let mut registry = self.service_registry.write().await;
        
        // 创建点对点接收通道
        let (tx, rx) = mpsc::channel(self.config.unicast_capacity);
        self.unicast_channels.write().await.insert(info.service_id.clone(), tx);
        
        // 启动消息消费者
        let service_id = info.service_id.clone();
        let channels = self.unicast_channels.clone();
        tokio::spawn(async move {
            Self::consume_unicast_messages(service_id, rx, channels).await;
        });
        
        registry.insert(info.service_id.clone(), info);
        Ok(())
    }

    /// 注销服务
    pub async fn unregister_service(&self, service_id: &str) -> Result<()> {
        let mut registry = self.service_registry.write().await;
        registry.remove(service_id);
        
        let mut channels = self.unicast_channels.write().await;
        channels.remove(service_id);
        
        Ok(())
    }

    /// 广播消息
    pub async fn broadcast(&self, message: BusMessage) -> Result<()> {
        // 记录消息日志
        self.log_message(&message).await;

        self.broadcast_tx.send(message.clone())
            .context("Failed to broadcast message")?;

        Ok(())
    }

    /// 发送点对点消息
    pub async fn send_unicast(&self, message: BusMessage) -> Result<()> {
        let target = message.target.as_ref()
            .context("Unicast message requires target")?;
        
        let channels = self.unicast_channels.read().await;
        let tx = channels.get(target)
            .context(format!("Target service {} not found", target))?;
        
        // 记录消息日志
        self.log_message(&message).await;
        
        tx.send(message).await
            .context("Failed to send unicast message")?;
        
        Ok(())
    }

    /// 发送请求并等待响应
    pub async fn request_response(&self, request: BusMessage) -> Result<BusMessage> {
        let message_id = request.message_id.clone();
        
        // 记录待处理请求
        {
            let mut pending = self.pending_requests.write().await;
            pending.insert(
                message_id.clone(),
                RequestResponsePair {
                    request: request.clone(),
                    response: None,
                    completed: false,
                },
            );
        }
        
        // 发送请求
        match &request.target {
            Some(_) => self.send_unicast(request.clone()).await?,
            None => self.broadcast(request.clone()).await?,
        }
        
        // 等待响应
        let timeout = tokio::time::Duration::from_secs(self.config.request_timeout_secs);
        let start = std::time::Instant::now();
        
        while start.elapsed() < timeout {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            
            let pending = self.pending_requests.read().await;
            if let Some(pair) = pending.get(&message_id) {
                if pair.completed {
                    if let Some(response) = &pair.response {
                        return Ok(response.clone());
                    }
                }
            }
        }
        
        anyhow::bail!("Request timeout after {} seconds", self.config.request_timeout_secs)
    }

    /// 响应请求
    pub async fn respond_to_request(&self, request_id: &str, response: BusMessage) -> Result<()> {
        let mut pending = self.pending_requests.write().await;
        
        if let Some(pair) = pending.get_mut(request_id) {
            pair.response = Some(response.clone());
            pair.completed = true;
        } else {
            anyhow::bail!("Request {} not found", request_id);
        }
        
        // 发送响应
        match &response.target {
            Some(_) => self.send_unicast(response).await?,
            None => self.broadcast(response).await?,
        }
        
        Ok(())
    }

    /// 订阅特定类型的事件
    pub fn subscribe(&self, event_types: Vec<EventType>) -> EventBusSubscriber {
        let rx = self.broadcast_tx.subscribe();
        EventBusSubscriber {
            rx,
            event_types,
        }
    }

    /// 订阅所有事件
    pub fn subscribe_all(&self) -> EventBusSubscriber {
        let rx = self.broadcast_tx.subscribe();
        EventBusSubscriber {
            rx,
            event_types: vec![], // 空列表表示订阅所有
        }
    }

    /// 记录消息到日志
    async fn log_message(&self, message: &BusMessage) {
        if !self.config.enable_message_log {
            return;
        }
        
        let mut log = self.message_log.write().await;
        log.push(message.clone());
        
        // 限制日志大小
        if log.len() > self.config.max_message_log_size {
            let remove_count = log.len() - self.config.max_message_log_size;
            log.drain(0..remove_count);
        }
    }

    /// 获取消息日志
    pub async fn get_message_log(&self) -> Vec<BusMessage> {
        self.message_log.read().await.clone()
    }

    /// 获取服务注册表
    pub async fn get_service_registry(&self) -> HashMap<String, ServiceInfo> {
        self.service_registry.read().await.clone()
    }

    /// 获取待处理请求
    pub async fn get_pending_requests(&self) -> HashMap<String, RequestResponsePair> {
        self.pending_requests.read().await.clone()
    }

    /// 消费点对点消息（后台任务）
    async fn consume_unicast_messages(
        service_id: String,
        mut rx: mpsc::Receiver<BusMessage>,
        _channels: Arc<RwLock<HashMap<String, mpsc::Sender<BusMessage>>>>,
    ) {
        while let Some(message) = rx.recv().await {
            // 这里可以添加消息处理逻辑
            // 目前只是简单地消费掉，实际处理由服务自己完成
            tracing::debug!("Service {} received message: {:?}", service_id, message.event_type);
        }
    }
}

/// 事件总线订阅者
pub struct EventBusSubscriber {
    rx: broadcast::Receiver<BusMessage>,
    event_types: Vec<EventType>,
}

impl EventBusSubscriber {
    /// 接收下一个匹配的事件
    pub async fn recv(&mut self) -> Option<BusMessage> {
        loop {
            match self.rx.recv().await {
                Ok(message) => {
                    // 如果没有订阅过滤器，或者事件类型匹配
                    if self.event_types.is_empty() || self.event_types.contains(&message.event_type) {
                        return Some(message);
                    }
                }
                Err(broadcast::error::RecvError::Closed) => return None,
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    // 消息太多，消费者跟不上，继续接收
                    continue;
                }
            }
        }
    }

    /// 创建事件流
    pub fn into_stream(self) -> EventBusStream {
        EventBusStream { subscriber: self }
    }
}

/// 事件流（用于 Stream trait 集成）
pub struct EventBusStream {
    #[allow(dead_code)]
    subscriber: EventBusSubscriber,
}

// 简化实现：暂时不实现 Stream trait
// impl futures::stream::Stream for EventBusStream {
//     type Item = BusMessage;

//     fn poll_next(
//         mut self: std::pin::Pin<&mut Self>,
//         cx: &mut std::task::Context<'_>,
//     ) -> std::task::Poll<Option<Self::Item>> {
//         let mut recv = self.subscriber.recv();
//         std::pin::Pin::new(&mut recv).poll(cx)
//     }
// }

/// 服务总线构建器
pub struct ServiceBusBuilder {
    config: ServiceBusConfig,
    services: Vec<ServiceInfo>,
}

impl ServiceBusBuilder {
    pub fn new() -> Self {
        ServiceBusBuilder {
            config: ServiceBusConfig::default(),
            services: Vec::new(),
        }
    }

    pub fn with_config(mut self, config: ServiceBusConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_service(mut self, info: ServiceInfo) -> Self {
        self.services.push(info);
        self
    }

    pub fn build(self) -> Result<ServiceBus> {
        let bus = ServiceBus::new(self.config);
        
        // 注册服务（异步操作需要在运行时执行）
        // 这里返回 bus，服务可以在运行时注册
        Ok(bus)
    }
}

impl Default for ServiceBusBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_service_bus_broadcast() {
        let bus = ServiceBus::with_defaults();
        
        let message = BusMessage::broadcast(
            EventType::InferenceRequested,
            "test_service".to_string(),
            MessagePayload::Text("test".to_string()),
        );
        
        let mut subscriber = bus.subscribe_all();
        
        bus.broadcast(message.clone()).unwrap();
        
        let received = subscriber.recv().await.unwrap();
        assert_eq!(received.message_id, message.message_id);
    }

    #[tokio::test]
    async fn test_service_bus_unicast() {
        let bus = ServiceBus::with_defaults();
        
        // 注册服务
        let service_info = ServiceInfo {
            service_id: "target_service".to_string(),
            service_type: "test".to_string(),
            endpoint: "localhost:8080".to_string(),
            is_healthy: true,
            registered_at: 0,
        };
        
        bus.register_service(service_info).await.unwrap();
        
        let message = BusMessage::unicast(
            EventType::InferenceCompleted,
            "source_service".to_string(),
            "target_service".to_string(),
            MessagePayload::Text("test".to_string()),
        );
        
        bus.send_unicast(message).await.unwrap();
    }

    #[tokio::test]
    async fn test_service_registry() {
        let bus = ServiceBus::with_defaults();
        
        let service_info = ServiceInfo {
            service_id: "test_service".to_string(),
            service_type: "inference".to_string(),
            endpoint: "localhost:8080".to_string(),
            is_healthy: true,
            registered_at: 0,
        };
        
        bus.register_service(service_info.clone()).await.unwrap();
        
        let registry = bus.get_service_registry().await;
        assert!(registry.contains_key("test_service"));
        assert_eq!(registry.get("test_service").unwrap().service_type, "inference");
    }
}
