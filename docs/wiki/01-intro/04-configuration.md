# 配置指南

> **阅读时间**: 10 分钟  
> **适用对象**: 新用户、运维工程师

---

## 1. 配置文件

### 1.1 配置文件位置

```bash
# 默认配置文件路径
./config.toml

# 或通过环境变量指定
export BLOCKCHAIN_CONFIG=/path/to/config.toml
```

### 1.2 配置文件示例

```toml
# config.toml

# 节点配置
[node]
node_id = "node_1"
address = "0.0.0.0:3000"
data_dir = "./data"

# 区块链配置
[blockchain]
trust_threshold = 0.67
inference_timeout_ms = 30000
commit_timeout_ms = 10000
max_retries = 5

# 李群验证配置
[lie_group]
enabled = true
mapper_strategy = "exponential"
aggregator_formula = "geometric_mean"
distance_threshold = 0.5

# 缓存配置
[cache]
l1_capacity = 1000
l2_path = "./data/l2_cache"
l3_redis_url = "redis://localhost:6379"
l3_enabled = false

# 日志配置
[log]
level = "info"
enable_file_logging = true
log_file_path = "/var/log/blockchain.log"
enable_rotation = true
rotation_days = 7
```

---

## 2. 配置项详解

### 2.1 节点配置

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `node_id` | string | - | 节点唯一标识 |
| `address` | string | "0.0.0.0:3000" | RPC 监听地址 |
| `data_dir` | string | "./data" | 数据目录 |

### 2.2 区块链配置

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `trust_threshold` | float | 0.67 | 可信阈值 (0.0-1.0) |
| `inference_timeout_ms` | int | 30000 | 推理超时 (毫秒) |
| `commit_timeout_ms` | int | 10000 | 上链超时 (毫秒) |
| `max_retries` | int | 5 | 最大重试次数 |

### 2.3 李群验证配置

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `enabled` | bool | true | 是否启用李群验证 |
| `mapper_strategy` | string | "exponential" | 映射策略 (exponential/logarithmic) |
| `aggregator_formula` | string | "geometric_mean" | 聚合公式 |
| `distance_threshold` | float | 0.5 | 距离阈值 |

### 2.4 缓存配置

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `l1_capacity` | int | 1000 | L1 缓存容量 (条目数) |
| `l2_path` | string | "./data/l2_cache" | L2 磁盘缓存路径 |
| `l3_redis_url` | string | "redis://localhost:6379" | L3 Redis 连接 URL |
| `l3_enabled` | bool | false | 是否启用 L3 缓存 |

### 2.5 日志配置

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `level` | string | "info" | 日志级别 (trace/debug/info/warn/error) |
| `enable_file_logging` | bool | true | 是否启用文件日志 |
| `log_file_path` | string | "/var/log/blockchain.log" | 日志文件路径 |
| `enable_rotation` | bool | true | 是否启用日志轮转 |
| `rotation_days` | int | 7 | 日志保留天数 |

---

## 3. 环境变量

### 3.1 基础环境变量

```bash
# .env 文件示例

# API Key 配置
PROVIDER_API_KEY=your_api_key

# Redis 连接
REDIS_URL=redis://localhost:6379

# 日志级别
LOG_LEVEL=info

# 配置文件路径
BLOCKCHAIN_CONFIG=/path/to/config.toml
```

### 3.2 加载环境变量

```bash
# 使用 dotenv
source .env

# 或在代码中加载
use dotenv::dotenv;

fn main() {
    dotenv().ok();
    // ...
}
```

### 3.3 环境变量优先级

环境变量优先级 > 配置文件 > 默认值

```rust
use std::env;

// 示例：获取日志级别
let log_level = env::var("LOG_LEVEL")
    .unwrap_or_else(|_| config.log.level.clone());
```

---

## 4. Builder 模式配置

在代码中使用 Builder 模式动态构建配置：

```rust
use block_chain_with_context::{
    BlockchainConfig, LogConfig, TimeoutConfig, RetryConfig,
};

fn main() {
    // 区块链配置
    let config = BlockchainConfig::builder()
        .trust_threshold(0.75)
        .inference_timeout_ms(30000)
        .commit_timeout_ms(10000)
        .max_retries(5)
        .log_level("info")
        .build()
        .expect("配置验证失败");

    // 日志配置
    let log_config = LogConfig {
        level: "info".to_string(),
        enable_file_logging: true,
        log_file_path: Some("/var/log/blockchain.log".to_string()),
        enable_rotation: true,
        rotation_days: 7,
    };

    // 超时配置
    let timeout_config = TimeoutConfig {
        inference_timeout_ms: 30000,
        commit_timeout_ms: 10000,
        health_check_timeout_ms: 5000,
    };

    // 重试配置
    let retry_config = RetryConfig {
        max_retries: 5,
        initial_delay_ms: 100,
        max_delay_ms: 5000,
        multiplier: 2.0,
    };
}
```

---

## 5. 特性配置

### 5.1 Cargo.toml 配置

```toml
[dependencies]
block_chain_with_context = { version = "0.5.0", features = [
    "rpc",
    "grpc",
    "tiered-storage",
    "remote-storage",  # 可选：L3 Redis
    "p2p",             # 可选：P2P 网络
    "persistence",     # 可选：状态持久化
] }
```

### 5.2 构建命令

```bash
# 默认构建（rpc + grpc + tiered-storage）
cargo build

# 启用 L3 Redis 缓存
cargo build --features "remote-storage"

# 启用 P2P 网络
cargo build --features "p2p"

# 启用所有特性
cargo build --all-features
```

---

## 6. 配置验证

### 6.1 配置验证函数

```rust
use block_chain_with_context::BlockchainConfig;

fn validate_config(config: &BlockchainConfig) -> Result<(), String> {
    if config.trust_threshold < 0.0 || config.trust_threshold > 1.0 {
        return Err("trust_threshold 必须在 0.0-1.0 之间".to_string());
    }
    if config.inference_timeout_ms <= 0 {
        return Err("inference_timeout_ms 必须大于 0".to_string());
    }
    if config.max_retries < 0 {
        return Err("max_retries 必须大于等于 0".to_string());
    }
    Ok(())
}
```

### 6.2 配置检查清单

- [ ] `trust_threshold` 在 0.0-1.0 之间
- [ ] `inference_timeout_ms` 大于 0
- [ ] `commit_timeout_ms` 大于 0
- [ ] `max_retries` 大于等于 0
- [ ] `l1_capacity` 大于 0
- [ ] Redis URL 格式正确（如启用 L3）
- [ ] 日志文件路径可写（如启用文件日志）

---

## 7. 配置示例

### 7.1 开发环境配置

```toml
# config.dev.toml

[node]
node_id = "dev_node"
address = "127.0.0.1:3000"
data_dir = "./data/dev"

[blockchain]
trust_threshold = 0.5
inference_timeout_ms = 60000
commit_timeout_ms = 20000
max_retries = 10

[log]
level = "debug"
enable_file_logging = true
log_file_path = "./logs/dev.log"
```

### 7.2 生产环境配置

```toml
# config.prod.toml

[node]
node_id = "prod_node_1"
address = "0.0.0.0:3000"
data_dir = "/var/data/blockchain"

[blockchain]
trust_threshold = 0.75
inference_timeout_ms = 30000
commit_timeout_ms = 10000
max_retries = 3

[cache]
l1_capacity = 5000
l2_path = "/var/cache/l2"
l3_redis_url = "redis://redis-cluster:6379"
l3_enabled = true

[log]
level = "warn"
enable_file_logging = true
log_file_path = "/var/log/blockchain/prod.log"
rotation_days = 30
```

### 7.3 单节点测试配置

```toml
# config.single.toml

[node]
node_id = "single_node"
address = "127.0.0.1:3000"

[blockchain]
trust_threshold = 0.67
inference_timeout_ms = 30000
commit_timeout_ms = 10000

[lie_group]
enabled = true

[cache]
l1_capacity = 1000
l3_enabled = false

[log]
level = "info"
```

---

## 8. 常见问题

### 8.1 配置文件未找到

**问题**: `Config file not found`

**解决方案**:
```bash
# 检查配置文件路径
ls -la config.toml

# 或指定配置文件路径
export BLOCKCHAIN_CONFIG=/path/to/config.toml
```

### 8.2 环境变量未生效

**问题**: 环境变量设置后未生效

**解决方案**:
```bash
# 检查环境变量是否设置成功
echo $LOG_LEVEL

# 重启应用
# 环境变量在应用启动时加载
```

### 8.3 Redis 连接失败

**问题**: `Redis connection failed`

**解决方案**:
```bash
# 检查 Redis 是否运行
redis-cli ping  # 应返回 PONG

# 检查 Redis URL 格式
# 正确：redis://localhost:6379
# 错误：localhost:6379
```

---

## 9. 下一步

- 🚀 [快速开始](03-quickstart.md) - 构建、测试、运行示例
- 🏗️ [整体架构](../02-architecture/01-overview.md) - 深入了解系统架构
- 🛠️ [开发环境](../03-development/01-setup.md) - IDE、工具链配置

---

*最后更新：2026-03-26*  
*项目版本：v0.5.0*
