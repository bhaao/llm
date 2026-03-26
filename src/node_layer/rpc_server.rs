//! 节点 RPC 服务 - 通过 HTTP 暴露节点 API
//!
//! **核心功能**：
//! - 提供 HTTP API 供其他节点调用
//! - 实现跨节点 KV 分片读取
//! - 实现交易提交
//! - 实现 Prometheus 指标导出
//!
//! # 端点
//!
//! - `GET /get_kv_shard?key=xxx&shard=xxx` - 读取 KV 分片
//! - `POST /submit_transaction` - 提交交易
//! - `GET /health` - 健康检查
//! - `GET /node_info` - 节点信息
//! - `GET /metrics` - Prometheus 指标（v0.6.0 新增）

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::blockchain::Blockchain;
use crate::memory_layer::{MemoryLayerManager, KvShard};
use crate::node_layer::NodeLayerManager;
use crate::transaction::Transaction;
use crate::traits::Hashable;
use crate::metrics::MetricsRegistry;

/// RPC 服务器状态
pub struct RpcServerState {
    /// 节点层管理器
    pub node_layer: Arc<NodeLayerManager>,
    /// 记忆层管理器
    pub memory_layer: Arc<MemoryLayerManager>,
    /// 区块链
    pub blockchain: Arc<RwLock<Blockchain>>,
    /// 监控指标注册表（v0.6.0 新增）
    pub metrics_registry: Arc<MetricsRegistry>,
}

/// KV 读取请求参数
#[derive(Debug, Deserialize)]
pub struct KvQuery {
    /// KV 键
    pub key: String,
    /// 分片 ID
    pub shard: String,
    /// 认证令牌（可选）
    #[serde(default)]
    pub token: Option<String>,
}

/// KV 读取响应
#[derive(Debug, Serialize)]
pub struct KvResponse {
    /// 是否找到
    pub found: bool,
    /// KV 分片数据（如果找到）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shard: Option<KvShardData>,
    /// 错误信息（如果出错）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// KV 分片数据（序列化版本）
#[derive(Debug, Serialize, Clone)]
pub struct KvShardData {
    /// 键
    pub key: String,
    /// 值
    pub value: Vec<u8>,
    /// 版本号
    pub version: u64,
    /// 时间戳
    pub timestamp: u64,
    /// 哈希
    pub hash: String,
}

impl From<KvShard> for KvShardData {
    fn from(shard: KvShard) -> Self {
        KvShardData {
            key: shard.key,
            value: shard.value,
            version: 0, // KvShard 没有 version 字段，使用 0
            timestamp: shard.created_at, // 使用 created_at 作为 timestamp
            hash: shard.hash,
        }
    }
}

/// 交易提交请求
#[derive(Debug, Deserialize)]
pub struct TransactionRequest {
    /// 发送方
    pub from: String,
    /// 接收方
    pub to: String,
    /// 交易类型
    pub transaction_type: String,
    /// 交易数据
    pub data: Option<String>,
}

/// 交易提交响应
#[derive(Debug, Serialize)]
pub struct TransactionResponse {
    /// 是否成功
    pub success: bool,
    /// 交易哈希
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_hash: Option<String>,
    /// 错误信息（如果失败）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 健康检查响应
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// 是否健康
    pub healthy: bool,
    /// 节点 ID
    pub node_id: String,
    /// 区块高度
    pub block_height: u64,
}

/// 节点信息响应
#[derive(Debug, Serialize)]
pub struct NodeInfoResponse {
    /// 节点 ID
    pub node_id: String,
    /// 地址
    pub address: String,
    /// 角色
    pub role: String,
    /// 活跃提供商数量
    pub active_providers: usize,
}

/// 创建 RPC 路由器
pub fn create_router(state: Arc<RpcServerState>) -> Router {
    // CORS 配置
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // 速率限制：已移除（tower-http 0.6 不再提供 RateLimitLayer）
    // TODO: 使用 tower_governor 或自定义中间件实现速率限制

    Router::new()
        .route("/get_kv_shard", get(get_kv_shard))
        .route("/submit_transaction", post(submit_transaction))
        .route("/health", get(health_check))
        .route("/node_info", get(node_info))
        .route("/metrics", get(metrics))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// 读取 KV 分片
///
/// `GET /get_kv_shard?key=xxx&shard=xxx&token=xxx`
async fn get_kv_shard(
    State(state): State<Arc<RpcServerState>>,
    Query(params): Query<KvQuery>,
) -> Json<KvResponse> {
    // 基础认证检查（TODO: 实现完整的 JWT/签名验证）
    if let Some(token) = &params.token {
        // 这里应该验证 token，现在只是检查是否存在
        if token.is_empty() {
            return Json(KvResponse {
                found: false,
                shard: None,
                error: Some("Invalid or missing authentication token".to_string()),
            });
        }
    }

    // 创建临时访问凭证（TODO: 应该从 token 中解析）
    use crate::node_layer::{AccessCredential, AccessType};
    let credential = AccessCredential {
        credential_id: "rpc_temp".to_string(),
        provider_id: "rpc_server".to_string(),
        memory_block_ids: vec!["all".to_string()],
        access_type: AccessType::ReadOnly,
        expires_at: u64::MAX,
        issuer_node_id: state.node_layer.node_public_key.clone(),
        signature: "rpc_signature".to_string(),
        is_revoked: false,
    };

    // 读取 KV
    match state.memory_layer.read_kv(&params.key, &credential) {
        Some(shard) => Json(KvResponse {
            found: true,
            shard: Some(KvShardData::from(shard)),
            error: None,
        }),
        None => Json(KvResponse {
            found: false,
            shard: None,
            error: Some(format!("KV not found: key={}, shard={}", params.key, params.shard)),
        }),
    }
}

/// 提交交易
///
/// `POST /submit_transaction`
async fn submit_transaction(
    State(state): State<Arc<RpcServerState>>,
    Json(req): Json<TransactionRequest>,
) -> Result<Json<TransactionResponse>, StatusCode> {
    // 解析交易类型
    let tx_type = match req.transaction_type.as_str() {
        "transfer" => crate::transaction::TransactionType::Transfer,
        "inference_response" => crate::transaction::TransactionType::InferenceResponse,
        "internal" => crate::transaction::TransactionType::Internal,
        _ => {
            return Ok(Json(TransactionResponse {
                success: false,
                transaction_hash: None,
                error: Some(format!("Unknown transaction type: {}", req.transaction_type)),
            }));
        }
    };

    // 创建交易
    let tx = Transaction::new(
        req.from.clone(),
        req.to.clone(),
        tx_type,
        crate::transaction::TransactionPayload::None,
    );

    // 计算交易哈希
    let tx_hash = tx.hash();

    // 添加到待处理交易池
    {
        let mut blockchain = state.blockchain.write().await;
        blockchain.add_pending_transaction(tx);
    }

    Ok(Json(TransactionResponse {
        success: true,
        transaction_hash: Some(tx_hash),
        error: None,
    }))
}

/// 健康检查
///
/// `GET /health`
async fn health_check(
    State(state): State<Arc<RpcServerState>>,
) -> Json<HealthResponse> {
    let blockchain = state.blockchain.read().await;
    let block_height = blockchain.chain().len() as u64;

    Json(HealthResponse {
        healthy: true,
        node_id: state.node_layer.node_public_key.clone(),
        block_height,
    })
}

/// 获取节点信息
///
/// `GET /node_info`
async fn node_info(
    State(state): State<Arc<RpcServerState>>,
) -> Json<NodeInfoResponse> {
    let active_providers = state.node_layer.get_active_providers().len();

    Json(NodeInfoResponse {
        node_id: state.node_layer.node_public_key.clone(),
        address: state.node_layer.node_public_key.clone(),
        role: "validator".to_string(),
        active_providers,
    })
}

/// 获取 Prometheus 指标
///
/// `GET /metrics`（v0.6.0 新增）
async fn metrics(
    State(state): State<Arc<RpcServerState>>,
) -> Result<String, StatusCode> {
    // 更新动态指标
    state.metrics_registry.set_gossip_peers_count(
        state.node_layer.get_active_providers().len()
    );

    // 获取区块链高度作为 PBFT 视图号（示例）
    let blockchain = state.blockchain.read().await;
    let block_height = blockchain.chain().len() as u64;
    state.metrics_registry.set_pbft_view_number(block_height);

    // 导出 Prometheus 格式指标
    Ok(state.metrics_registry.metrics_text())
}

/// RPC 服务器
pub struct RpcServer {
    state: Arc<RpcServerState>,
    address: String,
}

impl RpcServer {
    /// 创建新的 RPC 服务器
    pub fn new(
        node_layer: Arc<NodeLayerManager>,
        memory_layer: Arc<MemoryLayerManager>,
        blockchain: Arc<RwLock<Blockchain>>,
        address: &str,
    ) -> Self {
        RpcServer {
            state: Arc::new(RpcServerState {
                node_layer,
                memory_layer,
                blockchain,
                metrics_registry: MetricsRegistry::new_arc(),
            }),
            address: address.to_string(),
        }
    }

    /// 创建带自定义指标注册表的 RPC 服务器
    pub fn with_metrics(
        node_layer: Arc<NodeLayerManager>,
        memory_layer: Arc<MemoryLayerManager>,
        blockchain: Arc<RwLock<Blockchain>>,
        metrics_registry: Arc<MetricsRegistry>,
        address: &str,
    ) -> Self {
        RpcServer {
            state: Arc::new(RpcServerState {
                node_layer,
                memory_layer,
                blockchain,
                metrics_registry,
            }),
            address: address.to_string(),
        }
    }

    /// 获取服务器状态
    pub fn state(&self) -> Arc<RpcServerState> {
        self.state.clone()
    }

    /// 创建路由器
    pub fn into_router(self) -> Router {
        create_router(self.state)
    }

    /// 运行服务器
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = self.address.clone();
        let app = self.into_router();
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        println!("RPC server listening on {}", addr);
        axum::serve(listener, app).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockchain::BlockchainConfig;

    #[tokio::test]
    async fn test_health_check() {
        let node_layer = Arc::new(NodeLayerManager::new(
            "test_node".to_string(),
            "test_address".to_string(),
        ));
        let memory_layer = Arc::new(MemoryLayerManager::new("test_node"));
        let blockchain = Arc::new(RwLock::new(
            Blockchain::with_config("test_address".to_string(), BlockchainConfig::default())
        ));

        let state = Arc::new(RpcServerState {
            node_layer,
            memory_layer,
            blockchain,
        });

        let response = health_check(State(state)).await;
        assert!(response.healthy);
        assert_eq!(response.node_id, "test_node");
        assert_eq!(response.block_height, 1); // 创世区块
    }

    #[tokio::test]
    async fn test_kv_response_serialization() {
        let shard = KvShard {
            key: "test_key".to_string(),
            value: b"test_value".to_vec(),
            hash: "abc123".to_string(),
            created_at: 1234567890,
            updated_at: 1234567890,
        };

        let data = KvShardData::from(shard);
        assert_eq!(data.key, "test_key");
        assert_eq!(data.value, b"test_value");
        assert_eq!(data.version, 0); // KvShard 没有 version 字段
        assert_eq!(data.hash, "abc123");
    }
}
