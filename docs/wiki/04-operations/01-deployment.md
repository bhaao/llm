# 部署指南

> **阅读时间**: 20 分钟  
> **适用对象**: 运维工程师、技术决策者

---

## 1. 部署架构

### 1.1 单节点部署

```text
┌─────────────────────────────────────┐
│         单节点部署                   │
│  ┌─────────────────────────────┐    │
│  │  Provider Layer             │    │
│  │  + Memory Layer             │    │
│  │  + Audit Layer              │    │
│  └─────────────────────────────┘    │
└─────────────────────────────────────┘
```

**适用场景**: 开发测试、原型验证、小规模生产

### 1.2 多节点部署（原型）

```text
┌─────────────────────────────────────────────────────────┐
│                    多节点部署                            │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐              │
│  │ Node 1   │  │ Node 2   │  │ Node 3   │              │
│  │ + Memory │  │ + Memory │  │ + Memory │              │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘              │
│       │             │             │                     │
│       └─────────────┴─────────────┘                     │
│                    Gossip Sync                          │
│       ┌─────────────────────────────┐                   │
│       │      PBFT Consensus         │                   │
│       └─────────────────────────────┘                   │
└─────────────────────────────────────────────────────────┘
```

**适用场景**: 生产环境（待 v0.6.0 完善）

---

## 2. 环境准备

### 2.1 系统要求

| 组件 | 要求 | 说明 |
|------|------|------|
| **OS** | Linux (Ubuntu 20.04+) | 推荐 |
| **CPU** | 4 核+ | 8 核+ 生产环境 |
| **内存** | 8GB+ | 16GB+ 生产环境 |
| **磁盘** | 50GB+ SSD | 根据数据量调整 |
| **网络** | 1Gbps+ | 多节点需要 |

### 2.2 依赖安装

```bash
# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# 安装 protoc
apt-get update && apt-get install protobuf-compiler

# 安装 Redis（可选，L3 缓存）
apt-get install redis-server

# 安装 Docker（可选）
curl -fsSL https://get.docker.com | sh
```

### 2.3 目录结构

```bash
# 创建目录
sudo mkdir -p /opt/blockchain/{bin,data,logs,config}
sudo chown -R $USER:$USER /opt/blockchain

# 目录说明:
# - bin: 可执行文件
# - data: 数据文件
# - logs: 日志文件
# - config: 配置文件
```

---

## 3. 单节点部署

### 3.1 构建

```bash
# 克隆项目
git clone https://github.com/user/block_chain_with_context.git
cd block_chain_with_context

# 构建 Release 版本
cargo build --release

# 复制可执行文件
cp target/release/block_chain_with_context /opt/blockchain/bin/
```

### 3.2 配置

```toml
# /opt/blockchain/config/config.toml

[node]
node_id = "prod_node_1"
address = "0.0.0.0:3000"
data_dir = "/opt/blockchain/data"

[blockchain]
trust_threshold = 0.75
inference_timeout_ms = 30000
commit_timeout_ms = 10000
max_retries = 3

[cache]
l1_capacity = 5000
l2_path = "/opt/blockchain/data/l2_cache"
l3_redis_url = "redis://localhost:6379"
l3_enabled = true

[log]
level = "warn"
enable_file_logging = true
log_file_path = "/opt/blockchain/logs/app.log"
enable_rotation = true
rotation_days = 30
```

### 3.3 systemd 服务

```ini
# /etc/systemd/system/blockchain.service

[Unit]
Description=Distributed KV Cache System
After=network.target redis.service

[Service]
Type=simple
User=blockchain
WorkingDirectory=/opt/blockchain
ExecStart=/opt/blockchain/bin/block_chain_with_context
Environment=RUST_LOG=warn
Environment=BLOCKCHAIN_CONFIG=/opt/blockchain/config/config.toml
Restart=on-failure
RestartSec=10

# 安全限制
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/blockchain/data /opt/blockchain/logs

[Install]
WantedBy=multi-user.target
```

### 3.4 启动服务

```bash
# 重载 systemd
sudo systemctl daemon-reload

# 启用服务
sudo systemctl enable blockchain

# 启动服务
sudo systemctl start blockchain

# 查看状态
sudo systemctl status blockchain

# 查看日志
journalctl -u blockchain -f
```

---

## 4. Docker 部署

### 4.1 Dockerfile

```dockerfile
FROM rust:1.70-slim as builder

WORKDIR /app
COPY . .

# 安装依赖
RUN apt-get update && apt-get install -y protobuf-compiler

# 构建
RUN cargo build --release

# 运行阶段
FROM debian:bullseye-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/block_chain_with_context /usr/local/bin/

EXPOSE 3000

CMD ["block_chain_with_context"]
```

### 4.2 Docker Compose

```yaml
# docker-compose.yml

version: '3.8'

services:
  app:
    build: .
    ports:
      - "3000:3000"
    volumes:
      - ./data:/opt/blockchain/data
      - ./logs:/opt/blockchain/logs
      - ./config:/opt/blockchain/config
    environment:
      - RUST_LOG=warn
      - BLOCKCHAIN_CONFIG=/opt/blockchain/config/config.toml
    depends_on:
      - redis
    restart: unless-stopped

  redis:
    image: redis:7-alpine
    volumes:
      - redis_data:/data
    restart: unless-stopped

volumes:
  redis_data:
```

### 4.3 启动

```bash
# 构建并启动
docker-compose up -d

# 查看日志
docker-compose logs -f app

# 停止
docker-compose down

# 重启
docker-compose restart
```

---

## 5. 多节点部署

### 5.1 节点配置

```toml
# Node 1: config.node1.toml
[node]
node_id = "node_1"
address = "0.0.0.0:3001"

# Node 2: config.node2.toml
[node]
node_id = "node_2"
address = "0.0.0.0:3002"

# Node 3: config.node3.toml
[node]
node_id = "node_3"
address = "0.0.0.0:3003"
```

### 5.2 启动节点

```bash
# 启动 Node 1
BLOCKCHAIN_CONFIG=config.node1.toml ./block_chain_with_context &

# 启动 Node 2
BLOCKCHAIN_CONFIG=config.node2.toml ./block_chain_with_context &

# 启动 Node 3
BLOCKCHAIN_CONFIG=config.node3.toml ./block_chain_with_context &
```

### 5.3 节点发现

```bash
# 手动注册节点
curl -X POST http://localhost:3001/register_node \
  -H "Content-Type: application/json" \
  -d '{"node_id": "node_2", "address": "localhost:3002"}'
```

---

## 6. 配置调优

### 6.1 性能调优

```toml
# 增加缓存容量
[cache]
l1_capacity = 10000
l3_enabled = true

# 调整超时
[blockchain]
inference_timeout_ms = 60000
commit_timeout_ms = 20000
```

### 6.2 日志调优

```toml
# 生产环境日志配置
[log]
level = "warn"
enable_file_logging = true
log_file_path = "/var/log/blockchain/app.log"
enable_rotation = true
rotation_days = 30
max_size_mb = 100
```

---

## 7. 备份与恢复

### 7.1 数据备份

```bash
# 备份数据目录
tar -czf blockchain_backup_$(date +%Y%m%d).tar.gz \
    /opt/blockchain/data

# 备份配置文件
tar -czf blockchain_config_$(date +%Y%m%d).tar.gz \
    /opt/blockchain/config
```

### 7.2 数据恢复

```bash
# 停止服务
sudo systemctl stop blockchain

# 恢复数据
tar -xzf blockchain_backup_20260326.tar.gz -C /

# 启动服务
sudo systemctl start blockchain
```

---

## 8. 健康检查

### 8.1 HTTP 健康检查

```bash
# 健康检查端点
curl http://localhost:3000/health

# 预期响应
{"status": "healthy", "node_id": "node_1"}
```

### 8.2 监控脚本

```bash
#!/bin/bash
# health_check.sh

RESPONSE=$(curl -s http://localhost:3000/health)
STATUS=$(echo $RESPONSE | jq -r '.status')

if [ "$STATUS" != "healthy" ]; then
    echo "Node is unhealthy!"
    exit 1
fi

echo "Node is healthy"
exit 0
```

---

## 9. 常见问题

### 9.1 服务无法启动

**问题**: `Failed to start blockchain.service`

**解决方案**:
```bash
# 查看日志
journalctl -u blockchain -n 50

# 检查配置
block_chain_with_context --check-config

# 检查端口占用
netstat -tlnp | grep 3000
```

### 9.2 内存使用过高

**问题**: 内存使用持续增长

**解决方案**:
```bash
# 检查 L1 缓存容量
# 调整 config.toml: l1_capacity = 5000

# 重启服务
sudo systemctl restart blockchain
```

### 9.3 磁盘空间不足

**问题**: 磁盘空间快速增长

**解决方案**:
```bash
# 检查 L2 缓存大小
du -sh /opt/blockchain/data/l2_cache

# 配置日志轮转
# 定期清理旧日志
```

---

## 10. 相关文档

- [监控告警](02-monitoring.md) - Prometheus、Grafana
- [故障排查](03-troubleshooting.md) - 常见问题、排查流程
- [性能调优](../../06-KV_CACHE_OPTIMIZATION.md) - 性能指标、优化建议

---

*最后更新：2026-03-26*  
*项目版本：v0.5.0*
