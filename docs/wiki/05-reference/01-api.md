# API 参考

> **最后更新**: 2026-03-26  
> **项目版本**: v0.5.0

---

## 1. HTTP API

### 1.1 健康检查

```http
GET /health
```

**响应**:
```json
{
  "status": "healthy",
  "node_id": "node_1",
  "uptime_seconds": 3600
}
```

---

### 1.2 读取 KV

```http
GET /get_kv_shard?key={key}&shard={shard}
```

**参数**:
- `key` (必需): 键名
- `shard` (可选): 分片名

**响应**:
```json
{
  "key": "my_key",
  "value": "base64_encoded_value",
  "shard": "context"
}
```

---

### 1.3 写入 KV

```http
POST /write_kv
Content-Type: application/json

{
  "key": "my_key",
  "value": "base64_encoded_value",
  "shard": "context",
  "credential_id": "cred_1"
}
```

**响应**:
```json
{
  "success": true,
  "hash": "0x1234567890abcdef"
}
```

---

### 1.4 提交交易

```http
POST /submit_transaction
Content-Type: application/json

{
  "from": "user_1",
  "to": "assistant_1",
  "transaction_type": "transfer",
  "data": null
}
```

**响应**:
```json
{
  "transaction_id": "tx_123",
  "status": "pending"
}
```

---

### 1.5 推理请求

```http
POST /inference
Content-Type: application/json

{
  "request_id": "req_1",
  "prompt": "Hello, AI!",
  "model": "llama-7b",
  "max_tokens": 100
}
```

**响应**:
```json
{
  "request_id": "req_1",
  "completion": "Hello! How can I help you?",
  "usage": {
    "prompt_tokens": 10,
    "completion_tokens": 20,
    "total_tokens": 30
  }
}
```

---

### 1.6 节点注册

```http
POST /register_node
Content-Type: application/json

{
  "node_id": "node_2",
  "address": "localhost:3002"
}
```

**响应**:
```json
{
  "success": true,
  "node_id": "node_2"
}
```

---

### 1.7 提供商健康

```http
GET /providers/health
```

**响应**:
```json
{
  "providers": [
    {
      "provider_id": "vllm",
      "status": "healthy",
      "health_score": 0.95
    }
  ]
}
```

---

### 1.8 切换提供商

```http
POST /providers/switch
Content-Type: application/json

{
  "provider_id": "vllm_backup"
}
```

**响应**:
```json
{
  "success": true,
  "current_provider": "vllm_backup"
}
```

---

### 1.9 共识状态

```http
GET /consensus/status
```

**响应**:
```json
{
  "active_validators": 3,
  "agreement_ratio": 1.0,
  "last_round": 100
}
```

---

### 1.10 同步状态

```http
GET /sync/status
```

**响应**:
```json
{
  "syncing": false,
  "current_block": 1000,
  "highest_block": 1000
}
```

---

## 2. Rust API

### 2.1 MemoryLayerManager

```rust
use block_chain_with_context::MemoryLayerManager;

// 创建实例
let mut memory = MemoryLayerManager::new("node_1");

// 写入 KV
memory.write_kv(
    "key".to_string(),
    b"value".to_vec(),
    &credential,
)?;

// 读取 KV
let shard = memory.read_kv("key", &credential);

// 删除 KV
memory.delete_kv("key", &credential)?;
```

---

### 2.2 Blockchain

```rust
use block_chain_with_context::Blockchain;

// 创建实例
let mut blockchain = Blockchain::new("node_1".to_string());

// 注册节点
blockchain.register_node("node_1".to_string());

// 添加 KV 存证
let kv_proof = KvCacheProof::new(
    "kv_001".to_string(),
    "hash_123".to_string(),
    "node_1".to_string(),
    1024,
);
blockchain.add_kv_proof(kv_proof);
```

---

### 2.3 InferenceOrchestrator

```rust
use block_chain_with_context::services::InferenceOrchestrator;

// 创建实例
let orchestrator = InferenceOrchestrator::new(
    node_layer,
    memory_layer,
    provider_layer,
);

// 执行推理
let response = orchestrator.execute(&request, &credential)?;
```

---

### 2.4 CommitmentService

```rust
use block_chain_with_context::services::CommitmentService;

// 创建实例
let commitment = CommitmentService::with_config(
    "addr_1".into(),
    BlockchainConfig::default(),
)?;

// 提交存证
commitment.commit_inference(
    metadata,
    &provider_id,
    &response,
    kv_proofs,
)?;
```

---

### 2.5 FailoverService

```rust
use block_chain_with_context::services::FailoverService;

// 创建实例
let failover = FailoverService::new(
    provider_layer,
    TimeoutConfig::default(),
);

// 带故障切换的执行
let response = failover.execute_with_failover(&request)?;
```

---

## 3. gRPC API

### 3.1 服务定义

```protobuf
// proto/rpc.proto

service KVCacheService {
  rpc GetKV(GetKVRequest) returns (GetKVResponse);
  rpc WriteKV(WriteKVRequest) returns (WriteKVResponse);
  rpc SubmitTransaction(TransactionRequest) returns (TransactionResponse);
}

service InferenceService {
  rpc Inference(InferenceRequest) returns (InferenceResponse);
}

service ConsensusService {
  rpc SubmitElement(LieAlgebraElement) returns (LieGroupElement);
}
```

---

### 3.2 使用示例

```rust
use tonic::Request;
use proto::kv_cache_service_client::KvCacheServiceClient;

// 创建客户端
let mut client = KvCacheServiceClient::connect("http://localhost:3000").await?;

// 读取 KV
let response = client.get_kv(Request::new(GetKVRequest {
    key: "my_key".to_string(),
})).await?;

// 写入 KV
let response = client.write_kv(Request::new(WriteKVRequest {
    key: "my_key".to_string(),
    value: b"value".to_vec(),
})).await?;
```

---

## 4. 错误码

| 错误码 | 说明 |
|--------|------|
| 400 | 请求参数错误 |
| 401 | 未授权 |
| 403 | 禁止访问 |
| 404 | 资源不存在 |
| 408 | 请求超时 |
| 500 | 服务器内部错误 |
| 503 | 服务不可用 |

---

## 5. 限流

| 端点 | 限流 |
|------|------|
| /health | 100 req/s |
| /get_kv_shard | 1000 req/s |
| /write_kv | 500 req/s |
| /inference | 100 req/s |
| /submit_transaction | 200 req/s |

---

*最后更新：2026-03-26*  
*项目版本：v0.5.0*
