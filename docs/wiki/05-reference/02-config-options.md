# 配置项参考

> **最后更新**: 2026-03-26  
> **项目版本**: v0.5.0

---

## 1. 节点配置

### 1.1 [node]

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `node_id` | string | - | 节点唯一标识（必须） |
| `address` | string | "0.0.0.0:3000" | RPC 监听地址 |
| `data_dir` | string | "./data" | 数据目录 |

**示例**:
```toml
[node]
node_id = "node_1"
address = "0.0.0.0:3000"
data_dir = "/var/data/blockchain"
```

---

## 2. 区块链配置

### 2.1 [blockchain]

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `trust_threshold` | float | 0.67 | 可信阈值 (0.0-1.0) |
| `inference_timeout_ms` | int | 30000 | 推理超时 (毫秒) |
| `commit_timeout_ms` | int | 10000 | 上链超时 (毫秒) |
| `max_retries` | int | 5 | 最大重试次数 |

**示例**:
```toml
[blockchain]
trust_threshold = 0.75
inference_timeout_ms = 60000
commit_timeout_ms = 20000
max_retries = 3
```

---

## 3. 李群验证配置

### 3.1 [lie_group]

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `enabled` | bool | true | 是否启用李群验证 |
| `mapper_strategy` | string | "exponential" | 映射策略 |
| `aggregator_formula` | string | "geometric_mean" | 聚合公式 |
| `distance_threshold` | float | 0.5 | 距离阈值 |

**策略选项**:
- `mapper_strategy`: "exponential", "logarithmic"
- `aggregator_formula`: "geometric_mean", "arithmetic_mean"

**示例**:
```toml
[lie_group]
enabled = true
mapper_strategy = "exponential"
aggregator_formula = "geometric_mean"
distance_threshold = 0.3
```

---

## 4. 缓存配置

### 4.1 [cache]

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `l1_capacity` | int | 1000 | L1 缓存容量 (条目数) |
| `l2_path` | string | "./data/l2_cache" | L2 磁盘缓存路径 |
| `l3_redis_url` | string | "redis://localhost:6379" | L3 Redis 连接 URL |
| `l3_enabled` | bool | false | 是否启用 L3 缓存 |
| `prefetcher_enabled` | bool | true | 是否启用预取 |
| `eviction_policy` | string | "lru" | 淘汰策略 |

**淘汰策略选项**:
- "lru": 最近最少使用
- "lfu": 最不经常使用
- "fifo": 先进先出

**示例**:
```toml
[cache]
l1_capacity = 5000
l2_path = "/var/cache/l2"
l3_redis_url = "redis://redis-cluster:6379"
l3_enabled = true
prefetcher_enabled = true
eviction_policy = "lru"
```

---

## 5. 日志配置

### 5.1 [log]

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `level` | string | "info" | 日志级别 |
| `enable_file_logging` | bool | true | 是否启用文件日志 |
| `log_file_path` | string | "/var/log/blockchain.log" | 日志文件路径 |
| `enable_rotation` | bool | true | 是否启用日志轮转 |
| `rotation_days` | int | 7 | 日志保留天数 |
| `max_size_mb` | int | 100 | 日志文件最大大小 (MB) |

**日志级别选项**:
- "trace": 最详细
- "debug": 调试
- "info": 信息
- "warn": 警告
- "error": 错误

**示例**:
```toml
[log]
level = "warn"
enable_file_logging = true
log_file_path = "/var/log/blockchain/app.log"
enable_rotation = true
rotation_days = 30
max_size_mb = 100
```

---

## 6. 网络配置

### 6.1 [network]

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `gossip_interval_ms` | int | 1000 | Gossip 同步间隔 (毫秒) |
| `max_peers` | int | 50 | 最大 peer 数量 |
| `connection_timeout_ms` | int | 5000 | 连接超时 (毫秒) |

**示例**:
```toml
[network]
gossip_interval_ms = 2000
max_peers = 100
connection_timeout_ms = 10000
```

---

## 7. 安全配置

### 7.1 [security]

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `enable_auth` | bool | true | 是否启用认证 |
| `api_key_header` | string | "X-API-Key" | API Key 请求头 |
| `rate_limit_enabled` | bool | true | 是否启用限流 |
| `rate_limit_per_second` | int | 1000 | 每秒请求数限制 |

**示例**:
```toml
[security]
enable_auth = true
api_key_header = "X-API-Key"
rate_limit_enabled = true
rate_limit_per_second = 5000
```

---

## 8. 环境变量

### 8.1 基础环境变量

| 变量名 | 说明 | 示例 |
|--------|------|------|
| `BLOCKCHAIN_CONFIG` | 配置文件路径 | /etc/blockchain/config.toml |
| `RUST_LOG` | 日志级别 | debug,info,warn |
| `NODE_ID` | 节点 ID | node_1 |
| `REDIS_URL` | Redis 连接 URL | redis://localhost:6379 |
| `PROVIDER_API_KEY` | 提供商 API Key | sk-xxx |

### 8.2 使用示例

```bash
# .env 文件
BLOCKCHAIN_CONFIG=/etc/blockchain/config.toml
RUST_LOG=info,warn
NODE_ID=prod_node_1
REDIS_URL=redis://redis-cluster:6379
PROVIDER_API_KEY=sk-xxx
```

---

## 9. 配置验证

### 9.1 命令行验证

```bash
# 检查配置文件
block_chain_with_context --check-config

# 检查数据目录
block_chain_with_context --check-data
```

### 9.2 代码验证

```rust
use block_chain_with_context::BlockchainConfig;

let config = BlockchainConfig::builder()
    .trust_threshold(0.75)
    .inference_timeout_ms(30000)
    .build()
    .expect("配置验证失败");
```

---

## 10. 配置检查清单

- [ ] `node_id` 唯一
- [ ] `address` 端口未被占用
- [ ] `data_dir` 有写权限
- [ ] `trust_threshold` 在 0.0-1.0 之间
- [ ] `l1_capacity` 大于 0
- [ ] Redis URL 格式正确（如启用）
- [ ] 日志文件路径可写

---

*最后更新：2026-03-26*  
*项目版本：v0.5.0*
