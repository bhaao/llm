# 调试技巧

> **阅读时间**: 20 分钟  
> **适用对象**: 开发者

---

## 1. 日志调试

### 1.1 使用 tracing

```rust
use tracing::{info, debug, warn, error, trace, instrument};

#[instrument(skip(data), fields(data_len = data.len()))]
async fn process_data(data: &[u8]) -> Result<()> {
    debug!("Starting data processing");
    
    match validate(data).await {
        Ok(_) => {
            info!("Data validated successfully");
            Ok(())
        }
        Err(e) => {
            warn!("Validation failed: {}", e);
            Err(e)
        }
    }
}
```

### 1.2 配置日志级别

```bash
# 环境变量
export RUST_LOG=debug
export RUST_LOG=info,my_crate=debug
export RUST_LOG=trace

# 在代码中配置
use tracing_subscriber;

tracing_subscriber::fmt()
    .with_max_level(tracing::Level::DEBUG)
    .init();
```

### 1.3 结构化日志

```rust
use tracing::info;

info!(
    target = "inference",
    request_id = %request.id,
    provider_id = %provider_id,
    latency_ms = latency.as_millis(),
    "Inference completed"
);
```

---

## 2. 断点调试

### 2.1 VS Code + CodeLLDB

1. 安装扩展：CodeLLDB
2. 配置 `.vscode/launch.json`:

```json
{
    "version": "0.2.0",
    "configurations": [
        {
            "type": "lldb",
            "request": "launch",
            "name": "Debug",
            "cargo": {
                "args": ["build", "--bin=main"]
            },
            "cwd": "${workspaceFolder}"
        }
    ]
}
```

3. 设置断点，按 F5 启动调试

### 2.2 GDB/LLDB

```bash
# 构建 debug 版本
cargo build

# 使用 lldb
lldb target/debug/main
(lldb) breakpoint set --name function_name
(lldb) run
(lldb) thread backtrace

# 使用 gdb
gdb target/debug/main
(gdb) break function_name
(gdb) run
(gdb) bt
```

---

## 3. 性能调试

### 3.1 火焰图

```bash
# 安装 flamegraph
cargo install flamegraph

# 生成火焰图
cargo flamegraph --root --freq 4000 -- ./target/release/program

# 查看生成的 SVG 文件
# 打开 flamegraph.svg
```

### 3.2 perf

```bash
# 安装 perf
sudo apt-get install linux-tools-common linux-tools-generic

# 采样
perf record -F 99 -p $(pgrep program) -- sleep 30

# 查看报告
perf report

# 生成火焰图
perf script | stackcollapse-perf.pl | flamegraph.pl > perf.svg
```

### 3.3 内存分析

```bash
# 使用 valgrind
valgrind --leak-check=full --show-leak-kinds=all \
    ./target/debug/program

# 使用 cargo-miri
cargo miri test

# 使用 heaptrack
heaptrack ./target/debug/program
```

---

## 4. 并发调试

### 4.1 检测死锁

```rust
use std::sync::{Arc, Mutex};

// 使用 timeout 检测死锁
use std::time::Duration;

let lock = Arc::new(Mutex::new(data));
let lock_clone = lock.clone();

let handle = tokio::task::spawn_blocking(move || {
    lock_clone.lock().unwrap()
});

// 设置超时
let result = tokio::time::timeout(
    Duration::from_secs(5),
    handle
).await;

if result.is_err() {
    eprintln!("Possible deadlock detected!");
}
```

### 4.2 使用 LoRa

```bash
# 安装 loom（模型检测器）
cargo add loom --dev

# 编写 loom 测试
#[test]
fn test_concurrent_access() {
    loom::model(|| {
        let data = Arc::new(AtomicUsize::new(0));
        // ...
    });
}
```

---

## 5. 常见问题排查

### 5.1 内存泄漏

**症状**: 内存使用持续增长

**排查步骤**:
```bash
# 1. 使用 valgrind
valgrind --leak-check=full ./target/debug/program

# 2. 检查 Rc/Arc 循环引用
# 使用 Weak 引用打破循环

# 3. 检查未关闭的资源
# 使用 RAII 模式
```

### 5.2 竞态条件

**症状**: 间歇性测试失败

**排查步骤**:
```bash
# 1. 增加测试重复次数
cargo stress-test

# 2. 使用 loom 检测
cargo test --features loom

# 3. 添加日志追踪执行顺序
tracing::info!("Thread {} acquired lock", thread_id);
```

### 5.3 性能瓶颈

**症状**: 响应时间过长

**排查步骤**:
```bash
# 1. 生成火焰图
cargo flamegraph

# 2. 查看热点函数
# 找出占用 CPU 最多的函数

# 3. 优化建议:
# - 减少不必要的克隆
# - 使用并行处理
# - 优化数据结构
```

---

## 6. 调试宏

### 6.1 自定义调试宏

```rust
#[macro_export]
macro_rules! debug_var {
    ($var:expr) => {
        tracing::debug!(
            "{} = {:?}",
            stringify!($var),
            $var
        );
    };
}

// 使用
debug_var!(some_variable);
// 输出：some_variable = Value { ... }
```

### 6.2 计时宏

```rust
#[macro_export]
macro_rules! time_it {
    ($expr:expr) => {{
        let start = std::time::Instant::now();
        let result = $expr;
        let duration = start.elapsed();
        tracing::info!("Elapsed: {:?}", duration);
        result
    }};
}

// 使用
let data = time_it!(expensive_operation());
```

---

## 7. 远程调试

### 7.1 Docker 调试

```dockerfile
# Dockerfile
FROM rust:1.70

# 安装调试工具
RUN apt-get update && apt-get install -y \
    gdb \
    valgrind \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

# 构建 debug 版本
RUN cargo build
```

```bash
# 运行容器
docker run -it --cap=SYS_PTRACE my-app /bin/bash

# 在容器内调试
gdb target/debug/main
```

### 7.2 生产环境调试

```rust
// 启用 backtrace
std::env::set_var("RUST_BACKTRACE", "1");

// 捕获 panic
std::panic::set_hook(Box::new(|panic_info| {
    eprintln!("Panic occurred: {}", panic_info);
    eprintln!("Backtrace:\n{:?}", std::backtrace::Backtrace::force_capture());
}));
```

---

## 8. 调试检查清单

- [ ] 日志级别设置正确
- [ ] 关键路径有日志埋点
- [ ] 错误信息包含上下文
- [ ] 超时设置合理
- [ ] 资源正确释放
- [ ] 并发访问有锁保护
- [ ] 测试覆盖边界情况

---

## 9. 相关文档

- [开发环境](01-setup.md) - IDE、工具链配置
- [编码规范](02-coding-style.md) - Rust 代码规范
- [测试指南](04-testing.md) - 单元测试、并发测试
- [贡献流程](05-contributing.md) - Git 工作流、PR 流程

---

*最后更新：2026-03-26*  
*项目版本：v0.5.0*
