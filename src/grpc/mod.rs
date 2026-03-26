//! gRPC 服务模块 - 跨节点通信层
//!
//! **核心功能**：
//! - 基于 gRPC 的跨节点 KV 分片访问
//! - 上下文分片跨节点传输
//! - 多级缓存远程访问
//! - 健康检查和监控

#![cfg(feature = "grpc")]

// 包含由 build.rs 生成的 protobuf 代码
pub mod node_rpc {
    include!(concat!(env!("OUT_DIR"), "/block_chain_with_context.rs"));
}

use std::sync::Arc;
use std::time::Instant;
use tonic::{Request, Response, Status, Code};
use node_rpc::{
    node_rpc_service_server::NodeRpcService as NodeRpcServiceServer,
    GetKvShardRequest, GetKvShardResponse,
    PutKvShardRequest, PutKvShardResponse,
    DeleteKvShardRequest, DeleteKvShardResponse,
    ContainsKeyRequest, ContainsKeyResponse,
    SliceContextRequest, SliceContextResponse,
    ReassembleContextRequest, ReassembleContextResponse,
    GetMultiLevelKvRequest, GetMultiLevelKvResponse,
    PutMultiLevelKvRequest, PutMultiLevelKvResponse,
    GetCacheMetricsRequest, GetCacheMetricsResponse,
    SubmitTransactionRequest, SubmitTransactionResponse,
    GetBlockByHeightRequest, GetBlockByHeightResponse,
    HealthCheckRequest, HealthCheckResponse, HealthStatus,
    KvShard, MultiLevelKvData,
    CacheMetrics,
};

/// gRPC 服务实现
pub struct GrpcService {
    /// 节点 ID
    node_id: String,
    /// 启动时间
    start_time: Instant,
    /// 多级缓存管理器（可选）
    cache_manager: Option<Arc<crate::memory_layer::multi_level_cache::MultiLevelCacheManager>>,
}

impl GrpcService {
    /// 创建新的 gRPC 服务
    pub fn new(node_id: String) -> Self {
        GrpcService {
            node_id,
            start_time: Instant::now(),
            cache_manager: None,
        }
    }

    /// 创建带多级缓存的 gRPC 服务
    pub fn with_cache(
        node_id: String,
        cache_manager: Arc<crate::memory_layer::multi_level_cache::MultiLevelCacheManager>,
    ) -> Self {
        GrpcService {
            node_id,
            start_time: Instant::now(),
            cache_manager: Some(cache_manager),
        }
    }

    /// 将 KV 数据转换为 protobuf 消息
    fn kv_to_proto(
        key: String,
        value: Vec<u8>,
        version: u64,
        created_at: u64,
        size_bytes: usize,
        tier: crate::memory_layer::multi_level_cache::StorageTier,
    ) -> KvShard {
        KvShard {
            key,
            value,
            version,
            created_at,
            size_bytes: size_bytes as u64,
            tier: match tier {
                crate::memory_layer::multi_level_cache::StorageTier::L1CpuMemory => node_rpc::StorageTier::L1CpuMemory,
                crate::memory_layer::multi_level_cache::StorageTier::L2Disk => node_rpc::StorageTier::L2Disk,
                crate::memory_layer::multi_level_cache::StorageTier::L3Remote => node_rpc::StorageTier::L3Remote,
            } as i32,
        }
    }

    /// 将 protobuf KV 转换为内部数据
    fn proto_to_kv(proto: KvShard) -> crate::memory_layer::multi_level_cache::MultiLevelKvData {
        use crate::memory_layer::multi_level_cache::{MultiLevelKvData, StorageTier as LocalStorageTier};

        let tier = match node_rpc::StorageTier::try_from(proto.tier).unwrap_or(node_rpc::StorageTier::L1CpuMemory) {
            node_rpc::StorageTier::L1CpuMemory => LocalStorageTier::L1CpuMemory,
            node_rpc::StorageTier::L2Disk => LocalStorageTier::L2Disk,
            node_rpc::StorageTier::L3Remote => LocalStorageTier::L3Remote,
        };

        let mut data = MultiLevelKvData::new(proto.key, proto.value);
        data.version = proto.version;
        data.created_at = proto.created_at;
        data.size_bytes = proto.size_bytes as usize;
        data.current_tier = tier;
        data
    }
}

#[tonic::async_trait]
impl NodeRpcServiceServer for GrpcService {
    async fn get_kv_shard(
        &self,
        request: Request<GetKvShardRequest>,
    ) -> Result<Response<GetKvShardResponse>, Status> {
        let req = request.into_inner();

        if let Some(ref cache) = self.cache_manager {
            match cache.get(&req.key).await {
                Ok(Some(data)) => {
                    let shard = Self::kv_to_proto(
                        data.key,
                        data.value,
                        data.version,
                        data.created_at,
                        data.size_bytes,
                        data.current_tier,
                    );
                    return Ok(Response::new(GetKvShardResponse {
                        result: Some(node_rpc::get_kv_shard_response::Result::Shard(shard)),
                    }));
                }
                Ok(None) => {
                    return Ok(Response::new(GetKvShardResponse {
                        result: Some(node_rpc::get_kv_shard_response::Result::Error(
                            "Key not found".to_string(),
                        )),
                    }));
                }
                Err(e) => {
                    let err_msg: String = format!("Cache error: {}", e);
                    return Ok(Response::new(GetKvShardResponse {
                        result: Some(node_rpc::get_kv_shard_response::Result::Error(err_msg)),
                    }));
                }
            }
        }

        Err(Status::new(Code::Unimplemented, "KV cache not configured"))
    }

    async fn put_kv_shard(
        &self,
        request: Request<PutKvShardRequest>,
    ) -> Result<Response<PutKvShardResponse>, Status> {
        let req = request.into_inner();

        if let Some(ref cache) = self.cache_manager {
            let shard = req.shard.ok_or_else(|| Status::invalid_argument("Missing shard data"))?;
            let data = Self::proto_to_kv(shard);

            match cache.put(data).await {
                Ok(_) => {
                    return Ok(Response::new(PutKvShardResponse {
                        success: true,
                        error: None,
                    }));
                }
                Err(e) => {
                    let err_msg: String = format!("Cache error: {}", e);
                    return Ok(Response::new(PutKvShardResponse {
                        success: false,
                        error: Some(err_msg),
                    }));
                }
            }
        }

        Err(Status::new(Code::Unimplemented, "KV cache not configured"))
    }

    async fn delete_kv_shard(
        &self,
        request: Request<DeleteKvShardRequest>,
    ) -> Result<Response<DeleteKvShardResponse>, Status> {
        let req = request.into_inner();

        if let Some(ref cache) = self.cache_manager {
            match cache.delete(&req.key).await {
                Ok(_) => {
                    return Ok(Response::new(DeleteKvShardResponse {
                        success: true,
                        error: None,
                    }));
                }
                Err(e) => {
                    let err_msg: String = format!("Cache error: {}", e);
                    return Ok(Response::new(DeleteKvShardResponse {
                        success: false,
                        error: Some(err_msg),
                    }));
                }
            }
        }

        Err(Status::new(Code::Unimplemented, "KV cache not configured"))
    }

    async fn contains_key(
        &self,
        request: Request<ContainsKeyRequest>,
    ) -> Result<Response<ContainsKeyResponse>, Status> {
        let req = request.into_inner();

        if let Some(ref cache) = self.cache_manager {
            let exists = cache.contains_key(&req.key).await;
            return Ok(Response::new(ContainsKeyResponse { exists }));
        }

        Err(Status::new(Code::Unimplemented, "KV cache not configured"))
    }

    async fn slice_context(
        &self,
        _request: Request<SliceContextRequest>,
    ) -> Result<Response<SliceContextResponse>, Status> {
        Err(Status::new(Code::Unimplemented, "Context sharding not yet implemented via gRPC"))
    }

    async fn reassemble_context(
        &self,
        _request: Request<ReassembleContextRequest>,
    ) -> Result<Response<ReassembleContextResponse>, Status> {
        Err(Status::new(Code::Unimplemented, "Context reassembly not yet implemented via gRPC"))
    }

    async fn get_multi_level_kv(
        &self,
        request: Request<GetMultiLevelKvRequest>,
    ) -> Result<Response<GetMultiLevelKvResponse>, Status> {
        let req = request.into_inner();

        if let Some(ref cache) = self.cache_manager {
            match cache.get(&req.key).await {
                Ok(Some(data)) => {
                    let proto_data = MultiLevelKvData {
                        key: data.key,
                        value: data.value,
                        size_bytes: data.size_bytes as u64,
                        version: data.version,
                        created_at: data.created_at,
                        last_accessed_at: data.last_accessed_at,
                        access_count: data.access_count,
                        current_tier: match data.current_tier {
                            crate::memory_layer::multi_level_cache::StorageTier::L1CpuMemory => node_rpc::StorageTier::L1CpuMemory,
                            crate::memory_layer::multi_level_cache::StorageTier::L2Disk => node_rpc::StorageTier::L2Disk,
                            crate::memory_layer::multi_level_cache::StorageTier::L3Remote => node_rpc::StorageTier::L3Remote,
                        } as i32,
                    };
                    return Ok(Response::new(GetMultiLevelKvResponse {
                        result: Some(node_rpc::get_multi_level_kv_response::Result::Data(proto_data)),
                    }));
                }
                Ok(None) => {
                    return Ok(Response::new(GetMultiLevelKvResponse {
                        result: Some(node_rpc::get_multi_level_kv_response::Result::Error(
                            "Key not found".to_string(),
                        )),
                    }));
                }
                Err(e) => {
                    let err_msg: String = format!("Cache error: {}", e);
                    return Ok(Response::new(GetMultiLevelKvResponse {
                        result: Some(node_rpc::get_multi_level_kv_response::Result::Error(err_msg)),
                    }));
                }
            }
        }

        Err(Status::new(Code::Unimplemented, "Multi-level cache not configured"))
    }

    async fn put_multi_level_kv(
        &self,
        request: Request<PutMultiLevelKvRequest>,
    ) -> Result<Response<PutMultiLevelKvResponse>, Status> {
        let req = request.into_inner();

        if let Some(ref cache) = self.cache_manager {
            let proto_data = req.data.ok_or_else(|| Status::invalid_argument("Missing data"))?;

            // 将 protobuf 的 StorageTier 转换为内部 StorageTier
            let internal_tier = match node_rpc::StorageTier::try_from(proto_data.current_tier) {
                Ok(node_rpc::StorageTier::L1CpuMemory) => crate::memory_layer::multi_level_cache::StorageTier::L1CpuMemory,
                Ok(node_rpc::StorageTier::L2Disk) => crate::memory_layer::multi_level_cache::StorageTier::L2Disk,
                Ok(node_rpc::StorageTier::L3Remote) => crate::memory_layer::multi_level_cache::StorageTier::L3Remote,
                Err(_) => crate::memory_layer::multi_level_cache::StorageTier::L1CpuMemory, // 默认
            };

            let mut data = Self::proto_to_kv(KvShard {
                key: proto_data.key,
                value: proto_data.value,
                version: proto_data.version,
                created_at: proto_data.created_at,
                size_bytes: proto_data.size_bytes,
                tier: internal_tier as i32,
            });
            data.access_count = proto_data.access_count;
            data.last_accessed_at = proto_data.last_accessed_at;

            match cache.put(data).await {
                Ok(_) => {
                    return Ok(Response::new(PutMultiLevelKvResponse {
                        success: true,
                        error: None,
                    }));
                }
                Err(e) => {
                    let err_msg: String = format!("Cache error: {}", e);
                    return Ok(Response::new(PutMultiLevelKvResponse {
                        success: false,
                        error: Some(err_msg),
                    }));
                }
            }
        }

        Err(Status::new(Code::Unimplemented, "Multi-level cache not configured"))
    }

    async fn get_cache_metrics(
        &self,
        _request: Request<GetCacheMetricsRequest>,
    ) -> Result<Response<GetCacheMetricsResponse>, Status> {
        if let Some(ref cache) = self.cache_manager {
            let metrics = cache.get_metrics().await;
            let proto_metrics = CacheMetrics {
                l1_entries: metrics.l1_entries as u32,
                l2_entries: metrics.l2_entries as u32,
                l3_entries: metrics.l3_entries as u32,
                total_size_bytes: metrics.total_size_bytes as u64,
                l1_hit_rate: metrics.l1_hit_rate,
                overall_hit_rate: metrics.overall_hit_rate,
                demote_l1_to_l2: metrics.demote_l1_to_l2,
                promote_l2_to_l1: metrics.promote_l2_to_l1,
                demote_l2_to_l3: metrics.demote_l2_to_l3,
            };
            return Ok(Response::new(GetCacheMetricsResponse {
                metrics: Some(proto_metrics),
            }));
        }

        Err(Status::new(Code::Unimplemented, "Multi-level cache not configured"))
    }

    async fn submit_transaction(
        &self,
        _request: Request<SubmitTransactionRequest>,
    ) -> Result<Response<SubmitTransactionResponse>, Status> {
        Err(Status::new(Code::Unimplemented, "Transaction submission not yet implemented via gRPC"))
    }

    async fn get_block_by_height(
        &self,
        _request: Request<GetBlockByHeightRequest>,
    ) -> Result<Response<GetBlockByHeightResponse>, Status> {
        Err(Status::new(Code::Unimplemented, "Block query not yet implemented via gRPC"))
    }

    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        let uptime = self.start_time.elapsed().as_secs();
        Ok(Response::new(HealthCheckResponse {
            status: HealthStatus::Serving as i32,
            node_id: self.node_id.clone(),
            uptime_secs: uptime,
        }))
    }
}

/// gRPC 服务器配置
#[derive(Debug, Clone)]
pub struct GrpcServerConfig {
    pub addr: std::net::SocketAddr,
    pub max_message_size: usize,
    pub connect_timeout_ms: u64,
    pub request_timeout_ms: u64,
}

impl Default for GrpcServerConfig {
    fn default() -> Self {
        GrpcServerConfig {
            addr: "127.0.0.1:50051".parse().unwrap(),
            max_message_size: 10 * 1024 * 1024,
            connect_timeout_ms: 5000,
            request_timeout_ms: 30000,
        }
    }
}

/// 启动 gRPC 服务器
pub async fn start_grpc_server(
    config: GrpcServerConfig,
    service: GrpcService,
) -> Result<(), Box<dyn std::error::Error>> {
    use tonic::transport::Server;
    use node_rpc::node_rpc_service_server::NodeRpcServiceServer;

    let addr = config.addr;
    println!("Starting gRPC server on {}", addr);

    Server::builder()
        .add_service(NodeRpcServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_grpc_service_creation() {
        let service = GrpcService::new("test_node".to_string());
        assert_eq!(service.node_id, "test_node");
    }

    #[tokio::test]
    async fn test_health_check() {
        use tonic::Request;

        let service = GrpcService::new("test_node".to_string());
        let request = Request::new(HealthCheckRequest {});

        let response = service.health_check(request).await.unwrap();
        let health = response.into_inner();

        assert_eq!(health.status, HealthStatus::Serving as i32);
        assert_eq!(health.node_id, "test_node");
        // uptime_secs is u64, always >= 0, so just check it's a reasonable value
        assert!(health.uptime_secs < 1000); // Should be less than 1000 seconds for a test
    }
}
