# API Key 配置指南

本文档介绍如何配置 Ollama API Key 以使用线上推理服务。

## 快速开始

### 1. 复制环境变量模板

```bash
cp .env.example .env
```

### 2. 编辑 .env 文件

```bash
# Ollama 服务地址（线上服务）
OLLAMA_URL=https://api.ollama.com

# Ollama API Key（从 Ollama 控制台获取）
OLLAMA_API_KEY=your_api_key_here

# 默认模型名称
OLLAMA_MODEL=qwen3-coder-next:q8_0

# 算力容量 (token/s)
OLLAMA_CAPACITY=100

# Token 分割阈值
OLLAMA_TOKEN_THRESHOLD=4096
```

### 3. 注册提供商

```bash
# 方式一：从环境变量加载（推荐）
cargo run -- provider register-ollama-from-env --id my_ollama_provider

# 方式二：命令行直接指定
cargo run -- provider register-ollama \
  --id my_ollama_provider \
  --url https://api.ollama.com \
  --api-key your_api_key \
  --model qwen3-coder-next:q8_0 \
  --capacity 100
```

## 环境变量说明

| 变量名 | 说明 | 默认值 | 必填 |
|--------|------|--------|------|
| `OLLAMA_URL` | Ollama 服务地址 | `http://localhost:11434` | 否 |
| `OLLAMA_API_KEY` | API Key（线上服务需要） | 无 | 线上服务必填 |
| `OLLAMA_MODEL` | 默认模型名称 | `qwen3-coder-next:q8_0` | 否 |
| `OLLAMA_CAPACITY` | 算力容量 (token/s) | `50` | 否 |
| `OLLAMA_TOKEN_THRESHOLD` | Token 分割阈值 | `4096` | 否 |

## 本地 vs 线上服务

### 本地 Ollama 服务

```bash
# 不需要 API Key
cargo run -- provider register-ollama \
  --id local_ollama \
  --url http://localhost:11434 \
  --model qwen3-coder-next:q8_0
```

### 线上 Ollama 服务

```bash
# 需要 API Key
cargo run -- provider register-ollama \
  --id cloud_ollama \
  --url https://api.ollama.com \
  --api-key sk_your_key \
  --model qwen3-coder-next:q8_0
```

## JSON 输出格式

```bash
cargo run -- --format json provider register-ollama-from-env --id my_provider
```

输出示例：

```json
{
  "status": "success",
  "message": "Ollama provider registered from environment variables",
  "provider": {
    "id": "my_provider",
    "url": "https://api.ollama.com",
    "model": "qwen3-coder-next:q8_0",
    "api_key_configured": true
  }
}
```

## 安全建议

1. **不要将 `.env` 文件提交到 Git**
   - `.env` 已在 `.gitignore` 中
   - 只共享 `.env.example` 模板

2. **使用环境变量管理敏感信息**
   ```bash
   # 在生产环境中
   export OLLAMA_API_KEY=your_key
   cargo run -- provider register-ollama-from-env --id prod_provider
   ```

3. **定期轮换 API Key**
   - 定期更新 Ollama 控制台的 API Key
   - 更新后重新加载配置

## 故障排查

### 问题：API Key 认证失败

**解决方案**：
1. 检查 API Key 是否正确
2. 确认 Ollama 服务地址正确
3. 查看 Ollama 控制台的 API 权限设置

### 问题：环境变量未加载

**解决方案**：
1. 确认 `.env` 文件在当前目录
2. 检查 `.env` 格式（无空格，使用 `=` 连接）
3. 使用 `printenv | grep OLLAMA` 验证

### 问题：线上服务连接超时

**解决方案**：
1. 检查网络连接
2. 确认防火墙允许 HTTPS 出站
3. 尝试增加超时时间（代码中配置）

## 获取 API Key

1. 访问 [Ollama 控制台](https://ollama.com/settings/api)
2. 登录/注册账号
3. 创建新的 API Key
4. 复制并保存到 `.env` 文件

## 相关文档

- [Ollama 实现文档](./OLLAMA_IMPLEMENTATION.md)
- [README.md](../README.md)
- [CLI 使用指南](../README.md#cli-使用示例)

---

*最后更新：2026-03-27*
*项目版本：v0.5.0*
