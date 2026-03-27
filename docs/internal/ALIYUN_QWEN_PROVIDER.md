# 阿里云百炼 Qwen API 集成指南

本指南介绍如何在项目中使用阿里云百炼 Qwen API 进行大模型推理。

## 快速开始

### 1. 获取 API Key

1. 访问 [阿里云百炼控制台](https://bailian.console.aliyun.com/)
2. 登录并创建/选择您的项目
3. 在 API 管理页面获取您的 API Key (格式：`sk-xxx`)

### 2. 配置环境变量

复制 `.env.example` 为 `.env` 并配置阿里云相关参数：

```bash
# 阿里云百炼 API Key
DASHSCOPE_API_KEY=sk-your_api_key_here

# 阿里云区域选择
ALIYUN_REGION=cn-beijing

# 默认模型名称
ALIYUN_MODEL=qwen3.5-plus

# 算力容量 (token/s)
ALIYUN_CAPACITY=100

# 请求超时 (毫秒)
ALIYUN_TIMEOUT_MS=30000

# 最大重试次数
ALIYUN_MAX_RETRIES=3
```

### 3. 区域选择

项目支持以下阿里云区域：

| 区域 | 代码 | 端点 |
|------|------|------|
| 中国（北京） | `cn-beijing` | `https://dashscope.aliyuncs.com` |
| 新加坡 | `singapore` | `https://dashscope-intl.aliyuncs.com` |
| 美国（弗吉尼亚） | `us-virginia` | `https://dashscope-us.aliyuncs.com` |

## 使用示例

### 基本使用

```rust
use block_chain_with_context::provider_layer::{
    aliyun_qwen_provider::{QwenProvider, QwenModelConfig, AliyunRegion},
    InferenceProvider, InferenceRequest,
};
use block_chain_with_context::memory_layer::MemoryLayerManager;
use block_chain_with_context::node_layer::{AccessCredential, AccessType};

// 1. 创建 Qwen 提供商
let mut config = QwenModelConfig::default();
config.model_name = "qwen3.5-plus".to_string();

let provider = QwenProvider::new(
    "qwen_provider".to_string(),
    AliyunRegion::CnBeijing,
    "sk-your_api_key",
    config,
    100, // 算力容量
);

// 2. 创建记忆层和访问凭证
let mut memory = MemoryLayerManager::new("node_1");
let credential = AccessCredential {
    credential_id: "cred_1".to_string(),
    provider_id: "qwen_provider".to_string(),
    memory_block_ids: vec!["all".to_string()],
    access_type: AccessType::ReadWrite,
    expires_at: u64::MAX,
    issuer_node_id: "node_1".to_string(),
    signature: "sig".to_string(),
    is_revoked: false,
};

// 3. 创建推理请求
let request = InferenceRequest::new(
    "req_1".to_string(),
    "你好，请介绍一下自己".to_string(),
    "qwen3.5-plus".to_string(),
    500,
);

// 4. 执行推理
let response = provider.execute_inference(&request, &memory, &credential).await;

match response {
    Ok(resp) => {
        println!("回复：{}", resp.completion);
        println!("Token 使用：{} input, {} output", resp.prompt_tokens, resp.completion_tokens);
    }
    Err(e) => eprintln!("推理失败：{}", e),
}
```

### 使用便捷构造函数

```rust
// 北京区域默认配置
let provider = QwenProvider::with_beijing_default(
    "qwen_provider".to_string(),
    "sk-your_api_key",
    "qwen3.5-plus",
    100,
);

// 带思考模式（仅部分模型支持）
let provider_thinking = QwenProvider::with_thinking(
    "qwen_thinking".to_string(),
    AliyunRegion::CnBeijing,
    "sk-your_api_key",
    "qwen3-max",
    200,
);

// 带工具调用
let provider_tools = QwenProvider::with_tools(
    "qwen_tools".to_string(),
    AliyunRegion::Singapore,
    "sk-your_api_key",
    "qwen3.5-plus",
    150,
    vec!["web_search", "code_interpreter"],
);
```

### 高级配置

```rust
use block_chain_with_context::failover::circuit_breaker::CircuitBreaker;

// 带断路器配置
let cb = CircuitBreaker::with_defaults();
let provider = QwenProvider::with_circuit_breaker(
    "qwen_provider".to_string(),
    AliyunRegion::CnBeijing,
    "sk-your_api_key",
    QwenModelConfig::default(),
    100,
    cb,
);

// 自定义超时和重试
let provider = QwenProvider::new(
    "qwen_provider".to_string(),
    AliyunRegion::CnBeijing,
    "sk-your_api_key",
    QwenModelConfig::default(),
    100,
)
.with_timeout(60000)  // 60 秒超时
.with_max_retries(5); // 最多重试 5 次
```

### 多轮对话

```rust
// 第一轮对话
let request1 = InferenceRequest::new(
    "req_1".to_string(),
    "我叫张三，请记住我的名字".to_string(),
    "qwen3.5-plus".to_string(),
    100,
);

let response1 = provider.execute_inference(&request1, &memory, &credential).await?;

// 第二轮对话（使用上下文）
let request2 = InferenceRequest::new(
    "req_2".to_string(),
    "我叫什么名字？".to_string(),
    "qwen3.5-plus".to_string(),
    100,
).with_memory_blocks(vec![0]); // 读取之前的上下文

let response2 = provider.execute_inference(&request2, &memory, &credential).await?;
println!("回答：{}", response2.completion);
// 输出：你叫张三
```

## 支持模型

### 中国区域

| 模型 | 说明 |
|------|------|
| `qwen3-max` | 最强性能，复杂任务 |
| `qwen3.5-plus` | 高性能，推荐默认使用 |
| `qwen3.5-flash` | 快速响应，成本敏感 |
| `qwen-plus` | 平衡性能和成本 |
| `qwen-flash` | 极速响应 |
| `qwen3-coder-plus` | 代码专用 |
| `qwen3-coder-flash` | 代码快速版 |

### 全球区域

新加坡和美国区域支持大部分中国区域模型，具体以阿里云官方文档为准。

## 内置工具

Qwen API 支持以下内置工具：

| 工具 | 说明 |
|------|------|
| `web_search` | 网络搜索 |
| `web_extractor` | 网页内容提取 |
| `code_interpreter` | 代码解释器 |
| `file_search` | 知识库搜索 |
| `image_search` | 图片搜索 |

使用示例：

```rust
let config = QwenModelConfig {
    model_name: "qwen3.5-plus".to_string(),
    enable_thinking: false,
    enable_tools: true,
    built_in_tools: vec![
        "web_search".to_string(),
        "code_interpreter".to_string(),
    ],
};

let provider = QwenProvider::new(
    "qwen_tools".to_string(),
    AliyunRegion::CnBeijing,
    "sk-your_api_key",
    config,
    100,
);
```

## 错误处理

```rust
use block_chain_with_context::provider_layer::aliyun_http_client::QwenApiError;

match provider.execute_inference(&request, &memory, &credential).await {
    Ok(response) => {
        if response.success {
            println!("成功：{}", response.completion);
        } else {
            eprintln!("推理失败：{:?}", response.error_message);
        }
    }
    Err(e) => {
        // 错误类型匹配
        match e.as_str() {
            e if e.contains("timeout") => eprintln!("请求超时"),
            e if e.contains("authentication") => eprintln!("认证失败，请检查 API Key"),
            e if e.contains("rate limit") => eprintln!("请求频率超限"),
            _ => eprintln!("其他错误：{}", e),
        }
    }
}
```

## 最佳实践

### 1. 选择合适的模型

- **日常对话/一般任务**: `qwen3.5-plus` 或 `qwen-plus`
- **复杂推理/代码生成**: `qwen3-max` 或 `qwen3-coder-plus`
- **成本敏感/高频调用**: `qwen3.5-flash` 或 `qwen-flash`

### 2. 配置合理的超时

- 简单任务：5-10 秒
- 一般对话：30 秒
- 复杂推理：60 秒或更长

### 3. 使用断路器

生产环境建议启用断路器模式，避免单点故障：

```rust
let cb = CircuitBreaker::builder()
    .failure_threshold(5)
    .success_threshold(3)
    .timeout(std::time::Duration::from_secs(30))
    .build();

let provider = QwenProvider::with_circuit_breaker(
    "qwen_provider".to_string(),
    AliyunRegion::CnBeijing,
    "sk-your_api_key",
    QwenModelConfig::default(),
    100,
    cb,
);
```

### 4. 上下文管理

使用记忆层管理对话上下文：

```rust
// 写入上下文
memory.write_kv(
    "context".to_string(),
    b"用户偏好：技术文档，简洁回答".to_vec(),
    &credential,
).unwrap();

// 后续请求会自动读取上下文
let request = InferenceRequest::new(
    "req_1".to_string(),
    "解释 Rust 的所有权".to_string(),
    "qwen3.5-plus".to_string(),
    500,
).with_memory_blocks(vec![0]);
```

## 故障排查

### 认证失败

**错误**: `AuthenticationError` 或 `401 Unauthorized`

**解决方案**:
1. 检查 API Key 是否正确
2. 确认 API Key 未过期
3. 验证区域端点是否匹配

### 请求超时

**错误**: `Timeout`

**解决方案**:
1. 增加超时时间：`.with_timeout(60000)`
2. 检查网络连接
3. 尝试其他区域端点

### 速率限制

**错误**: `429 Too Many Requests`

**解决方案**:
1. 降低请求频率
2. 增加重试等待时间
3. 联系阿里云提升配额

### 模型不可用

**错误**: `Model not found` 或 `404`

**解决方案**:
1. 确认模型名称正确
2. 检查区域是否支持该模型
3. 查看阿里云官方文档

## 参考资料

- [阿里云百炼官方文档](https://help.aliyun.com/zh/model-studio/)
- [Qwen API 参考](https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-responses)
- [模型列表](https://help.aliyun.com/zh/model-studio/models)

## 支持

如有问题，请提交 Issue 或联系项目维护者。
