# 故障排查

> **阅读时间**: 20 分钟  
> **适用对象**: 运维工程师、开发者

---

## 1. 排查流程

### 1.1 问题定位流程

```
收到告警
    ↓
确认问题范围（单节点/多节点）
    ↓
检查系统指标（CPU、内存、磁盘）
    ↓
检查应用日志
    ↓
定位问题根因
    ↓
实施修复方案
    ↓
验证修复效果
```

### 1.2 信息收集清单

- [ ] 问题发生时间
- [ ] 影响范围（单节点/多节点）
- [ ] 错误日志
- [ ] 监控指标截图
- [ ] 最近变更记录

---

## 2. 常见问题

### 2.1 服务无法启动

**症状**: `systemctl start blockchain` 失败

**排查步骤**:
```bash
# 1. 查看系统日志
journalctl -u blockchain -n 50 --no-pager

# 2. 检查配置文件
block_chain_with_context --check-config

# 3. 检查端口占用
netstat -tlnp | grep 3000
lsof -i :3000

# 4. 检查数据目录权限
ls -la /opt/blockchain/data
```

**常见原因**:
- 配置文件错误
- 端口被占用
- 数据目录权限问题
- 依赖服务未启动（Redis）

**解决方案**:
```bash
# 修复配置文件
vim /opt/blockchain/config/config.toml

# 释放端口
kill -9 $(lsof -t -i:3000)

# 修复权限
sudo chown -R blockchain:blockchain /opt/blockchain

# 启动 Redis
sudo systemctl start redis
```

---

### 2.2 内存使用过高

**症状**: 内存使用持续增长，超过 8GB

**排查步骤**:
```bash
# 1. 查看内存使用
free -h
ps aux | grep block_chain

# 2. 分析内存分布
cat /proc/$(pgrep block_chain)/smaps

# 3. 检查 L1 缓存配置
grep l1_capacity /opt/blockchain/config/config.toml

# 4. 查看 GC 日志（如有）
```

**解决方案**:
```bash
# 1. 临时方案：重启服务
sudo systemctl restart blockchain

# 2. 调整 L1 缓存容量
# config.toml: l1_capacity = 5000

# 3. 启用 L3 缓存
# config.toml: l3_enabled = true

# 4. 长期方案：优化代码，减少内存分配
```

---

### 2.3 磁盘空间不足

**症状**: 磁盘使用率超过 90%

**排查步骤**:
```bash
# 1. 查看磁盘使用
df -h

# 2. 查找大文件
du -sh /opt/blockchain/* | sort -h

# 3. 检查 L2 缓存大小
du -sh /opt/blockchain/data/l2_cache

# 4. 检查日志文件大小
du -sh /opt/blockchain/logs/*
```

**解决方案**:
```bash
# 1. 清理旧日志
find /opt/blockchain/logs -name "*.log.*" -mtime +7 -delete

# 2. 清理 L2 缓存
rm -rf /opt/blockchain/data/l2_cache/*

# 3. 配置日志轮转
# config.toml: rotation_days = 7, max_size_mb = 100

# 4. 扩容磁盘
```

---

### 2.4 KV 缓存命中率低

**症状**: `kv_cache_hit_rate < 0.5`

**排查步骤**:
```bash
# 1. 查看各级缓存命中率
curl http://localhost:3000/metrics | grep kv_cache_hit_rate

# 2. 检查缓存容量
curl http://localhost:3000/metrics | grep kv_cache_size

# 3. 分析访问模式
# 查看是否有大量冷数据
```

**解决方案**:
```bash
# 1. 增加 L1 缓存容量
# config.toml: l1_capacity = 10000

# 2. 启用 L3 缓存
# config.toml: l3_enabled = true

# 3. 优化预取策略
# config.toml: prefetcher_enabled = true

# 4. 调整淘汰策略
# config.toml: eviction_policy = "lru"
```

---

### 2.5 推理延迟高

**症状**: P99 延迟 > 1s

**排查步骤**:
```bash
# 1. 查看延迟分布
curl http://localhost:3000/metrics | grep inference_latency

# 2. 检查提供商健康状态
curl http://localhost:3000/providers/health

# 3. 检查网络延迟
ping vllm-server
curl -w "%{time_total}" http://vllm-server:8000/health

# 4. 查看 CPU 使用率
top -p $(pgrep block_chain)
```

**解决方案**:
```bash
# 1. 切换提供商
curl -X POST http://localhost:3000/providers/switch \
  -H "Content-Type: application/json" \
  -d '{"provider_id": "vllm_backup"}'

# 2. 增加超时时间
# config.toml: inference_timeout_ms = 60000

# 3. 优化网络
# 使用专线或内网连接

# 4. 扩容 vLLM 服务
```

---

### 2.6 共识失败

**症状**: `consensus_agreement_ratio < 0.67`

**排查步骤**:
```bash
# 1. 查看活跃节点数
curl http://localhost:3000/consensus/validators

# 2. 检查网络连接
curl http://localhost:3000/network/peers

# 3. 查看共识日志
grep consensus /opt/blockchain/logs/app.log

# 4. 检查节点同步状态
curl http://localhost:3000/sync/status
```

**解决方案**:
```bash
# 1. 重启离线节点
ssh node_2 "sudo systemctl restart blockchain"

# 2. 检查网络分区
# 确保所有节点可以互相通信

# 3. 重新同步数据
curl -X POST http://localhost:3000/sync/force

# 4. 调整共识阈值
# config.toml: trust_threshold = 0.6
```

---

### 2.7 Redis 连接失败

**症状**: `Redis connection failed`

**排查步骤**:
```bash
# 1. 检查 Redis 状态
sudo systemctl status redis
redis-cli ping

# 2. 检查网络连接
telnet localhost 6379

# 3. 查看 Redis 日志
sudo tail -f /var/log/redis/redis.log

# 4. 检查连接数
redis-cli info clients
```

**解决方案**:
```bash
# 1. 重启 Redis
sudo systemctl restart redis

# 2. 增加 Redis 连接数
# redis.conf: maxclients 10000

# 3. 检查 Redis 内存
redis-cli info memory

# 4. 清理 Redis 数据
redis-cli FLUSHDB  # 谨慎使用！
```

---

## 3. 日志分析

### 3.1 日志级别

| 级别 | 说明 | 使用场景 |
|------|------|----------|
| ERROR | 错误 | 操作失败，需要处理 |
| WARN | 警告 | 潜在问题，需要注意 |
| INFO | 信息 | 正常操作记录 |
| DEBUG | 调试 | 详细调试信息 |
| TRACE | 追踪 | 最详细的追踪信息 |

### 3.2 关键错误模式

```bash
# 搜索 ERROR 日志
grep ERROR /opt/blockchain/logs/app.log

# 搜索特定错误
grep "Access denied" /opt/blockchain/logs/app.log

# 统计错误频率
grep ERROR /opt/blockchain/logs/app.log | cut -d' ' -f1 | uniq -c

# 查看最近错误
tail -100 /opt/blockchain/logs/app.log | grep ERROR
```

### 3.3 日志分析工具

```bash
# 使用 lnav（高级日志查看器）
lnav /opt/blockchain/logs/app.log

# 使用 jq 分析 JSON 日志
cat app.log | jq 'select(.level == "ERROR")'

# 使用 grep 分析错误趋势
grep ERROR app.log | awk '{print $1}' | sort | uniq -c
```

---

## 4. 性能问题排查

### 4.1 CPU 使用率高

```bash
# 1. 查看进程 CPU 使用
top -p $(pgrep block_chain)

# 2. 生成火焰图
cargo flamegraph --pid $(pgrep block_chain)

# 3. 使用 perf 分析
perf top -p $(pgrep block_chain)
```

### 4.2 磁盘 IO 瓶颈

```bash
# 1. 查看磁盘 IO
iostat -x 1

# 2. 查看进程 IO
iotop -o -p $(pgrep block_chain)

# 3. 检查磁盘健康
smartctl -a /dev/sda
```

### 4.3 网络瓶颈

```bash
# 1. 查看网络流量
iftop -P -p 3000

# 2. 查看网络连接
ss -tnp | grep 3000

# 3. 测试网络延迟
ping -c 100 node_2
```

---

## 5. 排查工具

### 5.1 系统工具

```bash
# 系统监控
htop
iotop
iftop

# 网络工具
netstat
ss
tcpdump

# 磁盘工具
df
du
iostat
```

### 5.2 Rust 工具

```bash
# 内存分析
cargo install cargo-miri
cargo miri test

# 性能分析
cargo install flamegraph
cargo flamegraph

# 并发分析
cargo install loom
```

### 5.3 自定义工具

```bash
# 健康检查脚本
./health_check.sh

# 指标收集脚本
./metrics_collector.sh

# 日志分析脚本
./log_analyzer.sh
```

---

## 6. 应急处理

### 6.1 紧急重启

```bash
# 1. 保存状态（如可能）
curl -X POST http://localhost:3000/admin/checkpoint

# 2. 优雅关闭
sudo systemctl stop blockchain

# 3. 清理临时文件
rm -rf /opt/blockchain/data/tmp/*

# 4. 启动服务
sudo systemctl start blockchain
```

### 6.2 回滚版本

```bash
# 1. 停止服务
sudo systemctl stop blockchain

# 2. 备份当前版本
cp /opt/blockchain/bin/block_chain_with_context \
   /opt/blockchain/bin/block_chain_with_context.bak

# 3. 恢复旧版本
cp /opt/blockchain/bin/block_chain_with_context.v0.4.0 \
   /opt/blockchain/bin/block_chain_with_context

# 4. 启动服务
sudo systemctl start blockchain
```

### 6.3 数据恢复

```bash
# 1. 停止服务
sudo systemctl stop blockchain

# 2. 恢复备份
tar -xzf blockchain_backup_20260326.tar.gz -C /

# 3. 验证数据
block_chain_with_context --check-data

# 4. 启动服务
sudo systemctl start blockchain
```

---

## 7. 相关文档

- [部署指南](01-deployment.md) - 单节点、多节点部署
- [监控告警](02-monitoring.md) - Prometheus、Grafana
- [性能调优](../../06-KV_CACHE_OPTIMIZATION.md) - 性能指标、优化建议

---

*最后更新：2026-03-26*  
*项目版本：v0.5.0*
