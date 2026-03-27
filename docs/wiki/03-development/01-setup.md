# 开发环境

> **阅读时间**: 10 分钟  
> **适用对象**: 开发者、贡献者

---

## 1. 工具链配置

### 1.1 Rust 工具链

```bash
# 安装 rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 安装稳定版 Rust
rustup install stable
rustup default stable

# 安装 nightly（用于基准测试）
rustup install nightly

# 添加组件
rustup component add rustfmt clippy rust-src
```

### 1.2 IDE 配置

#### VS Code

1. 安装扩展：
   - rust-analyzer
   - CodeLLDB（调试）
   - crates（依赖版本提示）
   - Better TOML

2. 配置 `settings.json`:
```json
{
    "rust-analyzer.checkOnSave.command": "clippy",
    "rust-analyzer.inlayHints.enable": true,
    "editor.formatOnSave": true,
    "[rust]": {
        "editor.defaultFormatter": "rust-lang.rust-analyzer"
    }
}
```

#### IntelliJ IDEA

1. 安装 Rust 插件
2. 配置工具链：
   - Settings → Languages & Frameworks → Rust
   - 设置 rustup 路径
   - 启用 rustfmt 和 clippy

### 1.3 其他工具

```bash
# protoc（gRPC 需要）
apt-get install protobuf-compiler

# cargo-edit（管理依赖）
cargo install cargo-edit

# cargo-watch（文件变化自动编译）
cargo install cargo-watch

# cargo-audit（安全审计）
cargo install cargo-audit
```

---

## 2. 项目结构

```
block_chain_with_context/
├── src/
│   ├── lib.rs                    # 库入口
│   ├── main.rs                   # 程序入口
│   ├── blockchain.rs             # 审计日志层
│   ├── memory_layer.rs           # 记忆层
│   ├── node_layer.rs             # 节点层
│   ├── provider_layer.rs         # 提供商层
│   ├── services/                 # 服务层
│   │   ├── mod.rs
│   │   ├── inference_orchestrator.rs
│   │   ├── commitment_service.rs
│   │   ├── failover_service.rs
│   │   └── qaas_service.rs
│   ├── memory_layer/             # 记忆层子模块
│   │   ├── tiered_storage.rs
│   │   ├── multi_level_cache.rs
│   │   ├── kv_chunk.rs
│   │   ├── kv_index.rs
│   │   ├── kv_compressor.rs
│   │   └── prefetcher.rs
│   ├── consensus/                # 共识模块
│   │   └── pbft.rs
│   ├── lie_algebra/              # 李群验证模块
│   │   ├── mapper.rs
│   │   ├── aggregator.rs
│   │   └── metric.rs
│   └── failover/                 # 故障切换模块
│       └── circuit_breaker.rs
├── tests/
│   ├── concurrency_tests.rs      # 并发测试
│   ├── fuzz_tests.rs             # 模糊测试
│   └── integration_tests.rs      # 集成测试
├── benches/
│   ├── performance_bench.rs      # 性能基准
│   └── lie_group_bench.rs        # 李群基准
├── examples/
│   ├── basic_kv.rs               # KV 示例
│   └── rpc_server.rs             # RPC 示例
├── proto/
│   └── rpc.proto                 # gRPC 定义
├── Cargo.toml                    # 项目配置
├── build.rs                      # 构建脚本
└── config.toml                   # 配置文件
```

---

## 3. 构建与运行

### 3.1 基本命令

```bash
# 构建项目
cargo build

# 构建 Release 版本
cargo build --release

# 运行项目
cargo run

# 运行测试
cargo test

# 运行基准测试
cargo +nightly bench
```

### 3.2 特性配置

```bash
# 默认构建
cargo build

# 启用所有特性
cargo build --all-features

# 仅启用 HTTP RPC
cargo build --no-default-features --features rpc,tiered-storage

# 启用 L3 Redis
cargo build --features remote-storage

# 启用 P2P 网络
cargo build --features p2p
```

### 3.3 开发模式

```bash
# 文件变化自动编译
cargo watch -x build

# 文件变化自动测试
cargo watch -x test

# 文件变化自动运行
cargo watch -x run
```

---

## 4. 代码质量

### 4.1 格式化

```bash
# 格式化代码
cargo fmt

# 检查格式
cargo fmt -- --check
```

### 4.2 Lint

```bash
# 运行 clippy
cargo clippy

# 严格模式（警告视为错误）
cargo clippy -- -D warnings

# 所有特性
cargo clippy --all-features --all-targets -- -D warnings
```

### 4.3 安全审计

```bash
# 安装 cargo-audit
cargo install cargo-audit

# 运行审计
cargo audit
```

### 4.4 文档测试

```bash
# 运行文档测试
cargo test --doc

# 生成文档
cargo doc

# 生成文档并打开
cargo doc --open
```

---

## 5. 调试技巧

### 5.1 日志调试

```rust
use tracing::{info, debug, warn, error};

fn process_request(request: &Request) -> Result<()> {
    debug!("Processing request: {:?}", request);

    match validate(request) {
        Ok(_) => {
            info!("Request validated successfully");
            Ok(())
        }
        Err(e) => {
            warn!("Validation failed: {}", e);
            Err(e)
        }
    }
}
```

配置日志级别：
```bash
# 设置环境变量
export RUST_LOG=debug

# 或在 config.toml 配置
[log]
level = "debug"
```

### 5.2 断点调试

使用 VS Code + CodeLLDB:

1. 在代码中设置断点
2. 按 F5 启动调试
3. 查看变量、调用栈

### 5.3 性能分析

```bash
# 安装 perf
sudo apt-get install linux-tools-common linux-tools-generic

# 生成火焰图
cargo install flamegraph
cargo flamegraph --root --freq 4000 --min-width 0.001 -- ./target/release/program

# 查看火焰图
# 打开生成的 SVG 文件
```

---

## 6. 依赖管理

### 6.1 添加依赖

```bash
# 添加依赖
cargo add tokio --features full
cargo add serde --features derive
cargo add anyhow --dev

# 编辑 Cargo.toml
cargo edit
```

### 6.2 更新依赖

```bash
# 更新所有依赖
cargo update

# 更新特定依赖
cargo update -p tokio

# 查看可更新依赖
cargo outdated
```

### 6.3 依赖树

```bash
# 查看依赖树
cargo tree

# 查看特定依赖的反向依赖
cargo tree -i tokio

# 导出依赖图为 PNG
cargo tree --edges features --format "{p} {f}" | dot -Tpng > deps.png
```

---

## 7. Git 工作流

### 7.1 分支策略

```
main          - 主分支，生产代码
develop       - 开发分支
feature/*     - 功能分支
bugfix/*      - 修复分支
release/*     - 发布分支
```

### 7.2 提交流程

```bash
# 创建功能分支
git checkout -b feature/my-feature

# 提交更改
git add .
git commit -m "feat: add my feature"

# 推送到远程
git push origin feature/my-feature

# 创建 Pull Request
```

### 7.3 提交信息规范

```
<type>(<scope>): <subject>

<body>

<footer>
```

**类型**:
- `feat`: 新功能
- `fix`: Bug 修复
- `docs`: 文档更新
- `style`: 代码格式
- `refactor`: 重构
- `test`: 测试
- `chore`: 构建/工具

**示例**:
```
feat(memory_layer): add L3 Redis cache support

- Add Redis connection pool
- Implement async read/write
- Add configuration options

Closes #123
```

---

## 8. 常见问题

### 8.1 编译错误

**问题**: `protoc (protobuf compiler) not found`

**解决方案**:
```bash
apt-get install protobuf-compiler
# 或
cargo build --no-default-features --features rpc,tiered-storage
```

### 8.2 测试失败

**问题**: 并发测试失败

**解决方案**:
```bash
# 带输出运行
cargo test --test concurrency_tests -- --nocapture

# 运行单个测试
cargo test --lib specific_test_name -- --nocapture
```

### 8.3 内存泄漏

**问题**: 内存使用持续增长

**解决方案**:
```bash
# 使用 valgrind
valgrind --leak-check=full ./target/debug/program

# 或使用 cargo-miri
cargo miri test
```

---

## 9. 下一步

- 📝 [编码规范](02-coding-style.md) - Rust 代码规范
- 🔧 [调试技巧](03-debugging.md) - 调试工具、常见问题
- ✅ [测试指南](04-testing.md) - 单元测试、并发测试
- 🤝 [贡献流程](05-contributing.md) - Git 工作流、PR 流程

---

*最后更新：2026-03-26*  
*项目版本：v0.5.0*
