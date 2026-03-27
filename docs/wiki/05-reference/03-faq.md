# 常见问题

> **最后更新**: 2026-03-26  
> **项目版本**: v0.5.0

---

## 1. 安装与构建

### Q: protoc 未找到

**问题**: `ERROR: protoc (protobuf compiler) not found`

**解决方案**:
```bash
# Debian/Ubuntu
apt-get install protobuf-compiler

# macOS
brew install protobuf

# 或禁用 gRPC 特性
cargo build --no-default-features --features rpc,tiered-storage
```

---

### Q: Rust 版本过低

**问题**: `error[E0658]: feature is not stable`

**解决方案**:
```bash
# 升级 Rust
rustup update

# 检查版本
rustc --version  # 应 >= 1.70.0
```

---

### Q: 编译时间过长

**问题**: 编译时间超过 30 分钟

**解决方案**:
```bash
# 使用 Release 模式（更快）
cargo build --release

# 增加并行编译
export CARGO_BUILD_JOBS=4

# 使用 sccache 缓存
cargo install sccache
export RUSTC_WRAPPER=sccache
```

---

## 2. 运行与配置

### Q: 服务无法启动

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

---

### Q: Redis 连接失败

**问题**: `Redis connection failed`

**解决方案**:
```bash
# 检查 Redis 状态
redis-cli ping  # 应返回 PONG

# 检查 Redis URL 格式
# 正确：redis://localhost:6379
# 错误：localhost:6379

# 重启 Redis
sudo systemctl restart redis
```

---

### Q: 配置文件未找到

**问题**: `Config file not found`

**解决方案**:
```bash
# 检查配置文件路径
ls -la config.toml

# 或指定配置文件路径
export BLOCKCHAIN_CONFIG=/path/to/config.toml
```

---

## 3. 使用与 API

### Q: KV 读取返回空

**问题**: `read_kv` 返回 `None`

**解决方案**:
1. 检查键名是否正确
2. 检查访问凭证是否有效
3. 检查 KV 是否已写入

```rust
// 写入后读取
memory.write_kv("key".to_string(), b"value".to_vec(), &credential)?;
let shard = memory.read_kv("key", &credential);
assert!(shard.is_some());
```

---

### Q: 访问被拒绝

**问题**: `MemoryError::AccessDenied`

**解决方案**:
1. 检查凭证是否过期
2. 检查凭证权限范围
3. 检查凭证是否被撤销

```rust
// 创建有效凭证
let credential = AccessCredential {
    credential_id: "cred_1".to_string(),
    access_type: AccessType::ReadWrite,
    expires_at: u64::MAX,  // 永不过期
    is_revoked: false,
    // ...
};
```

---

### Q: 推理超时

**问题**: `Inference timeout`

**解决方案**:
1. 检查 vLLM/SGLang 服务是否运行
2. 增加超时配置
3. 检查网络连接

```toml
# config.toml
[blockchain]
inference_timeout_ms = 60000  # 增加到 60 秒
```

---

## 4. 性能与优化

### Q: 缓存命中率低

**问题**: `kv_cache_hit_rate < 0.5`

**解决方案**:
1. 增加 L1 缓存容量
2. 启用 L3 缓存
3. 优化预取策略

```toml
# config.toml
[cache]
l1_capacity = 10000  # 增加容量
l3_enabled = true    # 启用 L3
prefetcher_enabled = true  # 启用预取
```

---

### Q: 内存使用过高

**问题**: 内存使用持续增长

**解决方案**:
1. 减少 L1 缓存容量
2. 启用 L3 缓存
3. 重启服务

```toml
# config.toml
[cache]
l1_capacity = 2000  # 减少容量
l3_enabled = true   # 启用 L3
```

---

### Q: 磁盘空间不足

**问题**: 磁盘使用率超过 90%

**解决方案**:
1. 清理旧日志
2. 清理 L2 缓存
3. 配置日志轮转

```bash
# 清理旧日志
find /opt/blockchain/logs -name "*.log.*" -mtime +7 -delete

# 清理 L2 缓存
rm -rf /opt/blockchain/data/l2_cache/*
```

---

## 5. 并发与一致性

### Q: 并发测试失败

**问题**: 间歇性测试失败

**解决方案**:
```bash
# 单线程运行
cargo test -- --test-threads=1

# 增加调试输出
cargo test -- --nocapture

# 检查锁顺序
# 确保遵循 L1 → L2 → L3 顺序
```

---

### Q: 共识失败

**问题**: `consensus_agreement_ratio < 0.67`

**解决方案**:
1. 检查活跃节点数
2. 检查网络连接
3. 重新同步数据

```bash
# 查看活跃节点
curl http://localhost:3000/consensus/validators

# 强制同步
curl -X POST http://localhost:3000/sync/force
```

---

## 6. 故障排查

### Q: 如何查看日志

**问题**: 如何查看应用日志

**解决方案**:
```bash
# systemd 服务
journalctl -u blockchain -f

# 日志文件
tail -f /opt/blockchain/logs/app.log

# 使用 lnav
lnav /opt/blockchain/logs/app.log
```

---

### Q: 如何生成火焰图

**问题**: 如何分析性能瓶颈

**解决方案**:
```bash
# 安装 flamegraph
cargo install flamegraph

# 生成火焰图
cargo flamegraph --root --freq 4000 -- ./target/release/program

# 查看生成的 SVG 文件
```

---

### Q: 如何调试内存泄漏

**问题**: 内存使用持续增长

**解决方案**:
```bash
# 使用 valgrind
valgrind --leak-check=full ./target/debug/program

# 使用 cargo-miri
cargo miri test

# 检查 Rc/Arc 循环引用
```

---

## 7. 部署与运维

### Q: 如何备份数据

**问题**: 如何备份重要数据

**解决方案**:
```bash
# 备份数据目录
tar -czf blockchain_backup_$(date +%Y%m%d).tar.gz \
    /opt/blockchain/data

# 备份配置文件
tar -czf blockchain_config_$(date +%Y%m%d).tar.gz \
    /opt/blockchain/config
```

---

### Q: 如何回滚版本

**问题**: 如何回滚到旧版本

**解决方案**:
```bash
# 停止服务
sudo systemctl stop blockchain

# 恢复旧版本
cp /opt/blockchain/bin/block_chain_with_context.v0.4.0 \
   /opt/blockchain/bin/block_chain_with_context

# 启动服务
sudo systemctl start blockchain
```

---

### Q: 如何监控服务

**问题**: 如何监控服务状态

**解决方案**:
```bash
# 健康检查
curl http://localhost:3000/health

# 查看指标
curl http://localhost:3000/metrics

# 使用 Prometheus + Grafana
# 参考监控告警文档
```

---

## 8. 开发与贡献

### Q: 如何开始贡献

**问题**: 如何参与项目贡献

**解决方案**:
1. Fork 项目
2. 创建功能分支
3. 提交更改
4. 创建 Pull Request

参考 [贡献流程](../03-development/05-contributing.md)。

---

### Q: 如何运行测试

**问题**: 如何运行项目测试

**解决方案**:
```bash
# 运行所有测试
cargo test

# 运行单元测试
cargo test --lib

# 运行并发测试
cargo test --test concurrency_tests -- --nocapture

# 运行基准测试
cargo +nightly bench
```

---

### Q: 如何生成文档

**问题**: 如何生成 API 文档

**解决方案**:
```bash
# 生成文档
cargo doc

# 生成文档并打开
cargo doc --open

# 运行文档测试
cargo test --doc
```

---

## 9. 其他问题

### Q: 项目支持哪些 LLM 引擎

**A**: 目前支持 vLLM 和 SGLang，通过 HTTP API 集成。

---

### Q: 是否支持多节点部署

**A**: 支持，但多节点分布式能力处于原型阶段，预计 v0.6.0 完善。

---

### Q: 生产环境是否可用

**A**: 单节点场景生产就绪，多节点场景建议等待 v0.6.0。

---

## 10. 获取帮助

如果以上 FAQ 未能解决您的问题，请：

1. 查看 [故障排查](../04-operations/03-troubleshooting.md)
2. 查看 [GitHub Issues](https://github.com/user/block_chain_with_context/issues)
3. 联系项目维护者

---

*最后更新：2026-03-26*  
*项目版本：v0.5.0*
