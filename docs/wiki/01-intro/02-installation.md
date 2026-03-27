# 环境安装

> **阅读时间**: 10 分钟  
> **适用对象**: 新用户

---

## 1. 基础要求

| 依赖 | 版本 | 说明 |
|------|------|------|
| **Rust** | 1.70+ | 必须 |
| **Edition** | 2021 | 必须 |
| **protoc** | 3.0+ | gRPC 特性需要 |

---

## 2. 安装 Rust

### 2.1 Linux/macOS

```bash
# 安装 rustup（Rust 工具链管理器）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 加载环境变量
source $HOME/.cargo/env

# 验证安装
rustc --version
cargo --version
```

### 2.2 Windows

1. 下载并运行 [rustup-init.exe](https://rustup.rs)
2. 按提示完成安装
3. 重启终端

### 2.3 升级 Rust

```bash
# 升级 rustup
rustup update

# 升级到特定版本
rustup install 1.70.0

# 设置默认版本
rustup default 1.70.0
```

---

## 3. 安装 protoc

protoc 是 Protocol Buffers 编译器，gRPC 特性需要。

### 3.1 Linux

```bash
# Debian/Ubuntu
apt-get update && apt-get install protobuf-compiler

# Arch Linux
pacman -S protobuf

# 验证安装
protoc --version
```

### 3.2 macOS

```bash
# Homebrew
brew install protobuf

# 验证安装
protoc --version
```

### 3.3 Windows

1. 访问 [GitHub Releases](https://github.com/protocolbuffers/protobuf/releases)
2. 下载 `protoc-<version>-win64.zip`
3. 解压到 `C:\protoc`
4. 添加 `C:\protoc\bin` 到 PATH 环境变量

---

## 4. 安装可选依赖

### 4.1 Redis（L3 缓存）

```bash
# Docker 方式
docker run -d -p 6379:6379 redis:latest

# 验证
redis-cli ping  # 应返回 PONG
```

### 4.2 vLLM/SGLang（LLM 推理）

```bash
# 安装 vLLM
pip install vllm

# 启动服务
python -m vllm.entrypoints.api_server \
    --model meta-llama/Llama-2-7b-chat-hf \
    --host 0.0.0.0 \
    --port 8000
```

---

## 5. 验证安装

```bash
# 检查 Rust 版本
rustc --version  # 应 >= 1.70.0

# 检查 cargo
cargo --version

# 检查 protoc（如使用 gRPC）
protoc --version  # 应 >= 3.0

# 检查 Redis（如使用 L3 缓存）
redis-cli ping  # 应返回 PONG
```

---

## 6. 常见问题

### 6.1 rustup 安装失败

**问题**: 网络连接超时

**解决方案**: 使用国内镜像
```bash
# 使用中科大镜像
export RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static
export RUSTUP_UPDATE_ROOT=https://mirrors.ustc.edu.cn/rust-static/rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 6.2 protoc 版本过低

**问题**: 编译报错 `unsupported proto file`

**解决方案**: 升级到最新版本
```bash
# 卸载旧版本
apt-get remove protobuf-compiler

# 安装最新版本（从源码）
wget https://github.com/protocolbuffers/protobuf/releases/download/v21.12/protoc-21.12-linux-x86_64.zip
unzip protoc-21.12-linux-x86_64.zip -d $HOME/.local
export PATH=$HOME/.local/bin:$PATH
```

### 6.3 权限问题

**问题**: `Permission denied`

**解决方案**:
```bash
# Linux/macOS - 使用 sudo
sudo apt-get install protobuf-compiler

# 或安装到用户目录
wget <protoc-url>
unzip protoc.zip -d $HOME/.local
export PATH=$HOME/.local/bin:$PATH
```

---

## 7. 下一步

- 🚀 [快速开始](03-quickstart.md) - 构建、测试、运行示例
- ⚙️ [配置指南](04-configuration.md) - 配置文件、环境变量

---

*最后更新：2026-03-26*  
*项目版本：v0.5.0*
