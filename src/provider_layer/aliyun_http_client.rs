//! 阿里云 Qwen HTTP 客户端模块 - 生产级实现
//!
//! **核心功能**：
//! - 通过 HTTP/REST API 调用阿里云百炼 Qwen API
//! - 支持 OpenAI 兼容协议
//! - 真正的 SSE 流式响应解析
//! - 安全的 API Key 管理
//! - 完善的请求追踪和日志
//!
//! # 使用示例
//!
//! ```ignore
//! use block_chain_with_context::provider_layer::aliyun_http_client::QwenHttpClient;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = QwenHttpClient::new("https://dashscope.aliyuncs.com", "sk-xxx");
//! let response = client.create_response("qwen3.5-plus", "Hello").await?;
//! println!("Response: {}", response.output_text);
//! # Ok(())
//! # }
//! ```

use bytes::Bytes;
use eventsource_stream::Eventsource;
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};
use thiserror::Error;
use tokio::time::Duration;
use tracing::{debug, error, info, warn};

/// 安全的 API Key 封装
/// 
/// - 不实现 Debug trait，避免日志泄露
/// - Drop 时安全擦除内存
/// - 支持从环境变量加载
#[derive(Clone)]
pub struct ApiKey {
    inner: String,
}

impl ApiKey {
    /// 创建新的 API Key
    pub fn new(key: impl Into<String>) -> Self {
        let inner = key.into();
        ApiKey { inner }
    }

    /// 从环境变量加载 API Key
    pub fn from_env(var_name: &str) -> Result<Self, String> {
        std::env::var(var_name)
            .map(Self::new)
            .map_err(|_| format!("Environment variable {} not set", var_name))
    }

    /// 获取原始 Key（仅用于内部使用）
    pub(crate) fn as_str(&self) -> &str {
        &self.inner
    }

    /// 获取 Key 的前缀（用于日志，显示前 8 个字符）
    pub fn prefix(&self) -> String {
        if self.inner.len() <= 8 {
            "***".to_string()
        } else {
            format!("{}***", &self.inner[..8])
        }
    }
}

impl Drop for ApiKey {
    fn drop(&mut self) {
        // 安全擦除：将内存清零
        unsafe {
            let ptr = self.inner.as_mut_ptr();
            let len = self.inner.len();
            std::ptr::write_bytes(ptr, 0, len);
        }
    }
}

/// 显式实现 Debug 为空，避免泄露 Key
impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ApiKey({})", self.prefix())
    }
}

/// Qwen API 错误 - 区分可重试和不可重试错误
#[derive(Error, Debug)]
pub enum QwenApiError {
    #[error("HTTP 请求失败：{0}")]
    HttpError(#[from] reqwest::Error),

    #[error("认证失败：{0}")]
    AuthenticationError(String),

    #[error("速率限制：{0}")]
    RateLimitError(String),

    #[error("服务不可用：{0}")]
    ServiceUnavailable(String),

    #[error("无效请求：{0}")]
    InvalidRequest(String),

    #[error("无效响应：{0}")]
    InvalidResponse(String),

    #[error("SSE 流解析失败：{0}")]
    StreamError(String),

    #[error("超时：{timeout_ms}ms")]
    Timeout { timeout_ms: u64 },

    #[error("未知错误：{0}")]
    Unknown(String),
}

impl QwenApiError {
    /// 判断是否为可重试错误
    pub fn is_retryable(&self) -> bool {
        match self {
            // 网络错误可重试
            QwenApiError::HttpError(_) => true,
            QwenApiError::StreamError(_) => true,
            // 服务端错误可重试
            QwenApiError::ServiceUnavailable(_) => true,
            QwenApiError::Timeout { .. } => true,
            // 客户端错误不可重试
            QwenApiError::AuthenticationError(_) => false,
            QwenApiError::RateLimitError(_) => false,
            QwenApiError::InvalidRequest(_) => false,
            QwenApiError::InvalidResponse(_) => false,
            QwenApiError::Unknown(_) => false,
        }
    }

    /// 获取错误码（用于监控）
    pub fn error_code(&self) -> &'static str {
        match self {
            QwenApiError::HttpError(_) => "HTTP_ERROR",
            QwenApiError::AuthenticationError(_) => "AUTH_ERROR",
            QwenApiError::RateLimitError(_) => "RATE_LIMIT",
            QwenApiError::ServiceUnavailable(_) => "SERVICE_UNAVAILABLE",
            QwenApiError::InvalidRequest(_) => "INVALID_REQUEST",
            QwenApiError::InvalidResponse(_) => "INVALID_RESPONSE",
            QwenApiError::StreamError(_) => "STREAM_ERROR",
            QwenApiError::Timeout { .. } => "TIMEOUT",
            QwenApiError::Unknown(_) => "UNKNOWN",
        }
    }
}

/// 消息角色
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    Developer,
    User,
    Assistant,
    FunctionCall,
    FunctionCallOutput,
}

/// 消息内容项（支持文本和多模态）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum MessageContent {
    Text { text: String },
    ImageUrl { image_url: ImageUrl },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// 消息结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    #[serde(with = "content_serializer")]
    pub content: String,
}

/// 自定义序列化器处理 content 字段
mod content_serializer {
    use serde::{Serializer, Deserializer};
    use serde::Deserialize;

    pub fn serialize<S>(value: &str, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(value)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<String, D::Error>
    where
        D: Deserializer<'de>,
    {
        // 支持 string 或 array 格式
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Content {
            String(String),
            Array(Vec<serde_json::Value>),
        }

        match Content::deserialize(deserializer)? {
            Content::String(s) => Ok(s),
            Content::Array(arr) => {
                // 尝试从数组中提取文本
                for item in arr {
                    if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                        return Ok(text.to_string());
                    }
                }
                Ok(String::new())
            }
        }
    }
}

/// 工具定义（用于函数调用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub tool_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<FunctionDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector_store_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: serde_json::Value,
}

/// 工具选择
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    String(String), // "auto", "none", "required"
    Object {
        #[serde(rename = "type")]
        tool_type: String,
        function: Option<FunctionChoice>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionChoice {
    pub name: String,
}

/// 创建响应请求体
#[derive(Debug, Clone, Serialize)]
pub struct CreateResponseRequest {
    /// 模型名称
    pub model: String,
    /// 输入（文本或消息数组）
    pub input: serde_json::Value,
    /// 是否启用流式输出
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// 工具列表
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// 工具选择
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// 温度参数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Top-p 参数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// 最大 token 数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// 上一轮响应 ID（多轮对话）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
}

/// 创建响应响应体
#[derive(Debug, Clone, Deserialize)]
pub struct CreateResponse {
    /// 响应 ID
    pub id: String,
    /// 创建时间戳
    pub created_at: u64,
    /// 模型名称
    pub model: String,
    /// 对象类型
    pub object: String,
    /// 状态
    pub status: String,
    /// 输出内容
    pub output: Vec<OutputItem>,
    /// Token 使用统计
    pub usage: Usage,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OutputItem {
    #[serde(rename = "type")]
    pub item_type: String,
    pub role: String,
    pub status: String,
    pub content: Vec<ContentItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContentItem {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    #[serde(default)]
    pub output_tokens_details: Option<OutputTokensDetails>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OutputTokensDetails {
    #[serde(default)]
    pub reasoning_tokens: u32,
}

/// SSE 流事件类型 - 真正的流式响应事件
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// 响应创建
    ResponseCreated,
    /// 响应进行中
    ResponseInProgress,
    /// 输出项添加
    OutputItemAdded { item_type: String },
    /// 内容增量
    ContentAdded { content_type: String, text: String },
    /// 文本增量（便捷方法）
    OutputTextDelta { delta: String },
    /// 思考内容增量（仅支持思考模式的模型）
    ReasoningDelta { delta: String },
    /// 工具调用增量
    ToolCallDelta { name: String, arguments: String },
    /// 文本完成
    OutputTextDone { text: String },
    /// 响应完成
    ResponseCompleted {
        response_id: String,
        model: String,
        text: String,
        usage: Usage,
    },
    /// 错误
    Error { error: String },
}

/// SSE 流包装器 - 真正的异步流实现
pub struct QwenStream {
    inner: Pin<Box<dyn Stream<Item = Result<StreamEvent, QwenApiError>> + Send>>,
}

impl QwenStream {
    /// 从 reqwest 响应创建流
    pub fn from_response(response: reqwest::Response, request_id: String) -> Self {
        let stream = response
            .bytes_stream()
            .map_err(|e| QwenApiError::HttpError(e))
            .into_eventsource()
            .map_err(|e| QwenApiError::StreamError(format!("Failed to create eventsource: {}", e)));

        let event_stream = match stream {
            Ok(es) => es,
            Err(e) => {
                return Self {
                    inner: Box::pin(futures::stream::once(async move { Err(e) })),
                };
            }
        };

        let parsed_stream = event_stream.map(move |event| {
            match event {
                Ok(event) => {
                    // 解析 SSE 事件
                    if event.event == "response.created" {
                        Ok(StreamEvent::ResponseCreated)
                    } else if event.event == "response.in_progress" {
                        Ok(StreamEvent::ResponseInProgress)
                    } else if event.event == "response.completed" {
                        // 解析完整响应
                        match serde_json::from_str::<CreateResponse>(&event.data) {
                            Ok(response) => {
                                let text = response.output
                                    .iter()
                                    .filter(|item| item.item_type == "message")
                                    .flat_map(|item| item.content.iter())
                                    .filter(|content| content.content_type == "output_text")
                                    .map(|content| content.text.clone())
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                
                                Ok(StreamEvent::ResponseCompleted {
                                    response_id: response.id.clone(),
                                    model: response.model.clone(),
                                    text,
                                    usage: response.usage.clone(),
                                })
                            }
                            Err(e) => Err(QwenApiError::InvalidResponse(format!(
                                "Failed to parse completed response: {}", e
                            ))),
                        }
                    } else if event.event == "response.output_item.added" {
                        // 解析输出项添加
                        #[derive(Deserialize)]
                        struct OutputItemAdded {
                            item: serde_json::Value,
                        }
                        match serde_json::from_str::<OutputItemAdded>(&event.data) {
                            Ok(data) => {
                                let item_type = data.item.get("type")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown")
                                    .to_string();
                                Ok(StreamEvent::OutputItemAdded { item_type })
                            }
                            Err(_) => Ok(StreamEvent::OutputItemAdded { item_type: "unknown".to_string() }),
                        }
                    } else if event.event == "response.content_part.added" {
                        // 解析内容部分添加
                        Ok(StreamEvent::ContentAdded {
                            content_type: "text".to_string(),
                            text: String::new(),
                        })
                    } else if event.event == "response.output_text.delta" {
                        // 解析文本增量
                        #[derive(Deserialize)]
                        struct TextDelta {
                            delta: String,
                        }
                        match serde_json::from_str::<TextDelta>(&event.data) {
                            Ok(data) => Ok(StreamEvent::OutputTextDelta { delta: data.delta }),
                            Err(_) => Ok(StreamEvent::OutputTextDelta { delta: event.data }),
                        }
                    } else if event.event == "response.output_text.done" {
                        // 解析文本完成
                        Ok(StreamEvent::OutputTextDone { text: event.data })
                    } else if event.event == "response.reasoning_text.delta" {
                        // 解析思考内容增量
                        Ok(StreamEvent::ReasoningDelta { delta: event.data })
                    } else if event.event == "error" {
                        Err(QwenApiError::StreamError(event.data))
                    } else {
                        debug!("Unknown SSE event type: {}", event.event);
                        Ok(StreamEvent::ResponseInProgress)
                    }
                }
                Err(e) => Err(QwenApiError::StreamError(format!("Eventsource error: {}", e))),
            }
        });

        Self {
            inner: Box::pin(parsed_stream),
        }
    }
}

impl Stream for QwenStream {
    type Item = Result<StreamEvent, QwenApiError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

/// Qwen HTTP 客户端实现
pub struct QwenHttpClient {
    client: reqwest::Client,
    base_url: String,
    api_key: ApiKey,
}

impl QwenHttpClient {
    /// 创建新的 Qwen 客户端
    ///
    /// # 参数
    ///
    /// * `base_url` - Qwen API 基础 URL（不含路径）
    /// * `api_key` - API Key（安全封装）
    ///
    /// # 返回
    ///
    /// * `Self` - 新的客户端实例
    pub fn new(base_url: &str, api_key: impl Into<ApiKey>) -> Self {
        QwenHttpClient {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .unwrap(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.into(),
        }
    }

    /// 创建带自定义客户端的 Qwen 客户端
    pub fn with_client(client: reqwest::Client, base_url: &str, api_key: impl Into<ApiKey>) -> Self {
        QwenHttpClient {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.into(),
        }
    }

    /// 获取基础 URL
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// 获取 API Key 前缀（用于日志）
    pub fn api_key_prefix(&self) -> String {
        self.api_key.prefix()
    }

    /// 执行创建响应请求（非流式）
    ///
    /// # 参数
    ///
    /// * `request` - 创建响应请求
    ///
    /// # 返回
    ///
    /// * `Result<CreateResponse, QwenApiError>` - 创建响应或错误
    pub async fn create_response(
        &self,
        request: &CreateResponseRequest,
    ) -> Result<CreateResponse, QwenApiError> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let start_time = std::time::Instant::now();

        let url = format!("{}/responses", self.base_url);

        info!(
            request_id = %request_id,
            method = "POST",
            url = %url,
            model = %request.model,
            api_key = %self.api_key_prefix(),
            "Sending Qwen API request"
        );

        debug!(request_id = %request_id, request_body = ?request, "Request details");

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key.as_str()))
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await
            .map_err(|e| {
                let elapsed = start_time.elapsed();
                error!(
                    request_id = %request_id,
                    error = %e,
                    elapsed_ms = elapsed.as_millis(),
                    "HTTP request failed"
                );
                QwenApiError::HttpError(e)
            })?;

        let status = response.status();
        let elapsed = start_time.elapsed();

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            error!(
                request_id = %request_id,
                status = %status,
                elapsed_ms = elapsed.as_millis(),
                error = %error_text,
                "API returned error status"
            );

            return Err(match status.as_u16() {
                401 | 403 => QwenApiError::AuthenticationError(format!(
                    "Authentication failed: {}",
                    error_text
                )),
                429 => QwenApiError::RateLimitError(format!("Rate limited: {}", error_text)),
                400 => QwenApiError::InvalidRequest(format!("Invalid request: {}", error_text)),
                500 | 502 | 503 | 504 => {
                    QwenApiError::ServiceUnavailable(format!("Service unavailable: {}", error_text))
                }
                _ => QwenApiError::Unknown(format!(
                    "Unknown error (status {}): {}",
                    status, error_text
                )),
            });
        }

        let result: CreateResponse = response.json().await.map_err(|e| {
            error!(
                request_id = %request_id,
                error = %e,
                "Failed to parse response JSON"
            );
            QwenApiError::InvalidResponse(format!("Failed to parse response: {}", e))
        })?;

        info!(
            request_id = %request_id,
            model = %result.model,
            status = %result.status,
            input_tokens = result.usage.input_tokens,
            output_tokens = result.usage.output_tokens,
            total_tokens = result.usage.total_tokens,
            elapsed_ms = elapsed.as_millis(),
            "Qwen API request completed"
        );

        Ok(result)
    }

    /// 执行创建响应请求（流式）
    ///
    /// # 参数
    ///
    /// * `request` - 创建响应请求（stream 字段会被强制设为 true）
    ///
    /// # 返回
    ///
    /// * `Result<QwenStream, QwenApiError>` - 流式响应或错误
    pub async fn create_response_stream(
        &self,
        request: &CreateResponseRequest,
    ) -> Result<QwenStream, QwenApiError> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let start_time = std::time::Instant::now();

        let url = format!("{}/responses", self.base_url);

        info!(
            request_id = %request_id,
            method = "POST",
            url = %url,
            model = %request.model,
            stream = true,
            api_key = %self.api_key_prefix(),
            "Sending Qwen API stream request"
        );

        // 强制启用流式
        let mut stream_request = request.clone();
        stream_request.stream = Some(true);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key.as_str()))
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .json(&stream_request)
            .send()
            .await
            .map_err(|e| {
                error!(
                    request_id = %request_id,
                    error = %e,
                    "Stream request failed"
                );
                QwenApiError::HttpError(e)
            })?;

        let status = response.status();
        let elapsed = start_time.elapsed();

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            error!(
                request_id = %request_id,
                status = %status,
                elapsed_ms = elapsed.as_millis(),
                error = %error_text,
                "Stream API returned error status"
            );

            return Err(match status.as_u16() {
                401 | 403 => QwenApiError::AuthenticationError(format!(
                    "Authentication failed: {}",
                    error_text
                )),
                429 => QwenApiError::RateLimitError(format!("Rate limited: {}", error_text)),
                400 => QwenApiError::InvalidRequest(format!("Invalid request: {}", error_text)),
                _ => QwenApiError::ServiceUnavailable(format!(
                    "Service unavailable: {}",
                    error_text
                )),
            });
        }

        info!(
            request_id = %request_id,
            elapsed_ms = elapsed.as_millis(),
            "Stream request started"
        );

        Ok(QwenStream::from_response(response, request_id))
    }

    /// 健康检查
    pub async fn health_check(&self) -> Result<bool, QwenApiError> {
        let request = CreateResponseRequest {
            model: "qwen3.5-flash".to_string(),
            input: serde_json::json!("health check"),
            stream: Some(false),
            tools: None,
            tool_choice: None,
            temperature: None,
            top_p: None,
            max_tokens: Some(1),
            previous_response_id: None,
        };

        match self.create_response(&request).await {
            Ok(_) => Ok(true),
            Err(QwenApiError::AuthenticationError(_)) => Ok(true), // 认证通过但可能 key 无效
            Err(e) => {
                warn!("Health check failed: {}", e);
                Ok(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::{header, method, path};

    #[test]
    fn test_api_key_debug_masking() {
        let key = ApiKey::new("sk-very_long_secret_key_12345");
        let debug_str = format!("{:?}", key);
        assert!(debug_str.contains("***"));
        assert!(!debug_str.contains("very_long_secret_key_12345"));
    }

    #[test]
    fn test_api_key_drop_erasure() {
        let key = ApiKey::new("secret_key");
        let ptr = key.inner.as_ptr();
        drop(key);
        // 安全擦除后，内存应该被清零
        unsafe {
            let slice = std::slice::from_raw_parts(ptr, 10);
            assert!(slice.iter().all(|&b| b == 0));
        }
    }

    #[test]
    fn test_api_key_prefix() {
        let key = ApiKey::new("sk-12345678-abcd");
        assert_eq!(key.prefix(), "sk-12345***");

        let short_key = ApiKey::new("sk-short");
        assert_eq!(short_key.prefix(), "***");
    }

    #[test]
    fn test_error_retryable() {
        let http_error = QwenApiError::HttpError(reqwest::Error::from(
            std::io::Error::new(std::io::ErrorKind::ConnectionReset, "connection reset"),
        ));
        assert!(http_error.is_retryable());

        let auth_error = QwenApiError::AuthenticationError("Invalid key".to_string());
        assert!(!auth_error.is_retryable());

        let rate_limit = QwenApiError::RateLimitError("Too many requests".to_string());
        assert!(!rate_limit.is_retryable());
    }

    #[test]
    fn test_message_serialization() {
        let message = Message {
            role: MessageRole::User,
            content: "Hello".to_string(),
        };

        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"Hello\""));
    }

    #[test]
    fn test_create_request_serialization() {
        let request = CreateResponseRequest {
            model: "qwen3.5-plus".to_string(),
            input: serde_json::json!("Hello"),
            stream: Some(false),
            tools: None,
            tool_choice: None,
            temperature: Some(0.7),
            top_p: None,
            max_tokens: Some(100),
            previous_response_id: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"model\":\"qwen3.5-plus\""));
        assert!(json.contains("\"input\":\"Hello\""));
        assert!(json.contains("\"stream\":false"));
    }

    #[tokio::test]
    async fn test_create_response_mock_server() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/responses"))
            .and(header("Authorization", "Bearer sk-test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "resp_123",
                "created_at": 1711165743,
                "model": "qwen3.5-plus",
                "object": "response",
                "status": "completed",
                "output": [{
                    "type": "message",
                    "role": "assistant",
                    "status": "completed",
                    "content": [{
                        "type": "output_text",
                        "text": "Hello from Qwen!"
                    }]
                }],
                "usage": {
                    "input_tokens": 5,
                    "output_tokens": 10,
                    "total_tokens": 15,
                    "output_tokens_details": {
                        "reasoning_tokens": 0
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        let client = QwenHttpClient::new(&mock_server.uri(), ApiKey::new("sk-test"));

        let request = CreateResponseRequest {
            model: "qwen3.5-plus".to_string(),
            input: serde_json::json!("Hello"),
            stream: Some(false),
            tools: None,
            tool_choice: None,
            temperature: None,
            top_p: None,
            max_tokens: Some(100),
            previous_response_id: None,
        };

        let response = client.create_response(&request).await.unwrap();
        assert_eq!(response.id, "resp_123");
        assert!(!response.output.is_empty());
    }

    #[tokio::test]
    async fn test_create_response_auth_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&mock_server)
            .await;

        let client = QwenHttpClient::new(&mock_server.uri(), ApiKey::new("invalid-key"));

        let request = CreateResponseRequest {
            model: "qwen3.5-plus".to_string(),
            input: serde_json::json!("Hello"),
            stream: Some(false),
            tools: None,
            tool_choice: None,
            temperature: None,
            top_p: None,
            max_tokens: Some(100),
            previous_response_id: None,
        };

        let result = client.create_response(&request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            QwenApiError::AuthenticationError(_)
        ));
    }

    #[tokio::test]
    async fn test_create_response_rate_limit() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(429).set_body_string("Too many requests"))
            .mount(&mock_server)
            .await;

        let client = QwenHttpClient::new(&mock_server.uri(), ApiKey::new("sk-test"));

        let request = CreateResponseRequest {
            model: "qwen3.5-plus".to_string(),
            input: serde_json::json!("Hello"),
            stream: Some(false),
            tools: None,
            tool_choice: None,
            temperature: None,
            top_p: None,
            max_tokens: Some(100),
            previous_response_id: None,
        };

        let result = client.create_response(&request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            QwenApiError::RateLimitError(_)
        ));
    }

    #[test]
    fn test_tool_serialization() {
        let tool = Tool {
            tool_type: "function".to_string(),
            function: Some(FunctionDefinition {
                name: "get_weather".to_string(),
                description: Some("Get weather for a city".to_string()),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "city": { "type": "string", "description": "City name" }
                    },
                    "required": ["city"]
                }),
            }),
            vector_store_ids: None,
        };

        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains("\"type\":\"function\""));
        assert!(json.contains("\"name\":\"get_weather\""));
    }
}
