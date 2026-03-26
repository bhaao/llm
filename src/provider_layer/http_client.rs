//! HTTP 推理客户端模块 - 调用远程 LLM 推理服务
//!
//! **核心功能**：
//! - 通过 HTTP/REST API 调用远程推理服务 (vLLM/SGLang/TGI)
//! - 标准化请求/响应格式
//! - 支持异步推理
//!
//! # 使用示例
//!
//! ```ignore
//! use block_chain_with_context::provider_layer::http_client::InferenceHttpClient;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = InferenceHttpClient::new("http://localhost:8000");
//! let response = client.generate("Hello, AI!", 100).await?;
//! println!("Response: {}", response.text);
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// HTTP 客户端错误
#[derive(Error, Debug)]
pub enum HttpClientError {
    #[error("HTTP 请求失败：{0}")]
    HttpError(#[from] reqwest::Error),

    #[error("服务不可用：{0}")]
    ServiceUnavailable(String),

    #[error("无效响应：{0}")]
    InvalidResponse(String),
}

/// HTTP 推理客户端
pub struct InferenceHttpClient {
    client: reqwest::Client,
    base_url: String,
}

/// 生成请求体
#[derive(Debug, Serialize)]
pub struct GenerateRequest {
    /// 提示词
    pub prompt: String,
    /// 最大生成 token 数
    pub max_tokens: usize,
    /// 温度参数 (可选)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// 停止词 (可选)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
}

/// 生成响应体
#[derive(Debug, Deserialize)]
pub struct GenerateResponse {
    /// 生成的文本
    pub text: String,
    /// prompt token 数
    #[serde(default)]
    pub prompt_tokens: u32,
    /// completion token 数
    #[serde(default)]
    pub completion_tokens: u32,
    /// 推理耗时 (毫秒)
    #[serde(default)]
    pub latency_ms: u64,
}

/// HTTP 推理客户端实现
impl InferenceHttpClient {
    /// 创建新的 HTTP 客户端
    ///
    /// # 参数
    ///
    /// * `base_url` - 推理服务的基础 URL，例如 "http://localhost:8000"
    ///
    /// # 返回
    ///
    /// * `Self` - 新的客户端实例
    pub fn new(base_url: &str) -> Self {
        InferenceHttpClient {
            client: reqwest::Client::new(),
            base_url: base_url.to_string(),
        }
    }

    /// 创建带自定义客户端的 HTTP 客户端
    ///
    /// # 参数
    ///
    /// * `client` - 自定义的 reqwest::Client
    /// * `base_url` - 推理服务的基础 URL
    ///
    /// # 返回
    ///
    /// * `Self` - 新的客户端实例
    pub fn with_client(client: reqwest::Client, base_url: &str) -> Self {
        InferenceHttpClient {
            client,
            base_url: base_url.to_string(),
        }
    }

    /// 获取基础 URL
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// 执行生成请求
    ///
    /// # 参数
    ///
    /// * `prompt` - 提示词
    /// * `max_tokens` - 最大生成 token 数
    ///
    /// # 返回
    ///
    /// * `Result<GenerateResponse, HttpClientError>` - 生成响应或错误
    pub async fn generate(
        &self,
        prompt: &str,
        max_tokens: usize,
    ) -> Result<GenerateResponse, HttpClientError> {
        let request = GenerateRequest {
            prompt: prompt.to_string(),
            max_tokens,
            temperature: None,
            stop: None,
        };

        let url = format!("{}/generate", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| HttpClientError::HttpError(e))?;

        if !response.status().is_success() {
            return Err(HttpClientError::ServiceUnavailable(format!(
                "Service returned status: {}",
                response.status()
            )));
        }

        let result: GenerateResponse = response
            .json()
            .await
            .map_err(|e| HttpClientError::InvalidResponse(format!("Failed to parse response: {}", e)))?;

        Ok(result)
    }

    /// 执行生成请求 (带温度和停止词)
    ///
    /// # 参数
    ///
    /// * `prompt` - 提示词
    /// * `max_tokens` - 最大生成 token 数
    /// * `temperature` - 温度参数 (控制随机性)
    /// * `stop` - 停止词列表
    ///
    /// # 返回
    ///
    /// * `Result<GenerateResponse, HttpClientError>` - 生成响应或错误
    pub async fn generate_with_options(
        &self,
        prompt: &str,
        max_tokens: usize,
        temperature: Option<f32>,
        stop: Option<Vec<String>>,
    ) -> Result<GenerateResponse, HttpClientError> {
        let request = GenerateRequest {
            prompt: prompt.to_string(),
            max_tokens,
            temperature,
            stop,
        };

        let url = format!("{}/generate", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| HttpClientError::HttpError(e))?;

        if !response.status().is_success() {
            return Err(HttpClientError::ServiceUnavailable(format!(
                "Service returned status: {}",
                response.status()
            )));
        }

        let result: GenerateResponse = response
            .json()
            .await
            .map_err(|e| HttpClientError::InvalidResponse(format!("Failed to parse response: {}", e)))?;

        Ok(result)
    }

    /// 健康检查
    ///
    /// # 返回
    ///
    /// * `Result<bool, HttpClientError>` - 服务是否健康
    pub async fn health_check(&self) -> Result<bool, HttpClientError> {
        let url = format!("{}/health", self.base_url);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| HttpClientError::HttpError(e))?;

        Ok(response.status().is_success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::{method, path};
    use serde_json::json;

    #[tokio::test]
    async fn test_client_creation() {
        let client = InferenceHttpClient::new("http://localhost:8000");
        assert_eq!(client.base_url(), "http://localhost:8000");
    }

    #[tokio::test]
    async fn test_client_with_custom_client() {
        let custom_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap();
        let client = InferenceHttpClient::with_client(custom_client, "http://localhost:8001");
        assert_eq!(client.base_url(), "http://localhost:8001");
    }

    #[test]
    fn test_generate_request_serialization() {
        let request = GenerateRequest {
            prompt: "Hello, AI!".to_string(),
            max_tokens: 100,
            temperature: Some(0.7),
            stop: Some(vec!["\n".to_string(), ".".to_string()]),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"prompt\":\"Hello, AI!\""));
        assert!(json.contains("\"max_tokens\":100"));
        assert!(json.contains("\"temperature\":0.7"));
    }

    #[test]
    fn test_generate_response_deserialization() {
        let json = r#"{
            "text": "Hello, human!",
            "prompt_tokens": 10,
            "completion_tokens": 20,
            "latency_ms": 500
        }"#;

        let response: GenerateResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.text, "Hello, human!");
        assert_eq!(response.prompt_tokens, 10);
        assert_eq!(response.completion_tokens, 20);
        assert_eq!(response.latency_ms, 500);
    }

    #[test]
    fn test_generate_response_default_fields() {
        let json = r#"{"text": "Hello!"}"#;

        let response: GenerateResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.text, "Hello!");
        assert_eq!(response.prompt_tokens, 0);
        assert_eq!(response.completion_tokens, 0);
        assert_eq!(response.latency_ms, 0);
    }

    #[test]
    fn test_http_client_error_display() {
        let err = HttpClientError::ServiceUnavailable("Server down".to_string());
        assert!(format!("{}", err).contains("服务不可用"));
    }

    #[tokio::test]
    async fn test_generate_with_mock_server() {
        // 启动 Mock 服务器
        let mock_server = MockServer::start().await;

        // 设置 Mock 响应
        Mock::given(method("POST"))
            .and(path("/generate"))
            .respond_with(ResponseTemplate::new(200)
                .set_body_json(json!({
                    "text": "Mock response from AI",
                    "prompt_tokens": 5,
                    "completion_tokens": 10,
                    "latency_ms": 100
                })))
            .mount(&mock_server)
            .await;

        // 创建客户端
        let client = InferenceHttpClient::new(&mock_server.uri());

        // 执行请求
        let response = client.generate("Hello!", 50).await.unwrap();

        // 验证响应
        assert_eq!(response.text, "Mock response from AI");
        assert_eq!(response.prompt_tokens, 5);
        assert_eq!(response.completion_tokens, 10);
        assert_eq!(response.latency_ms, 100);
    }

    #[tokio::test]
    async fn test_generate_with_options_mock_server() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/generate"))
            .respond_with(ResponseTemplate::new(200)
                .set_body_json(json!({
                    "text": "Response with options",
                    "prompt_tokens": 8,
                    "completion_tokens": 15,
                    "latency_ms": 150
                })))
            .mount(&mock_server)
            .await;

        let client = InferenceHttpClient::new(&mock_server.uri());

        let response = client.generate_with_options(
            "Hello!",
            50,
            Some(0.8),
            Some(vec!["\n".to_string()]),
        ).await.unwrap();

        assert_eq!(response.text, "Response with options");
        assert_eq!(response.completion_tokens, 15);
    }

    #[tokio::test]
    async fn test_health_check_mock_server() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/health"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = InferenceHttpClient::new(&mock_server.uri());
        let healthy = client.health_check().await.unwrap();
        assert!(healthy);
    }

    #[tokio::test]
    async fn test_health_check_failure() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/health"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&mock_server)
            .await;

        let client = InferenceHttpClient::new(&mock_server.uri());
        let healthy = client.health_check().await.unwrap();
        assert!(!healthy);
    }

    #[tokio::test]
    async fn test_generate_server_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/generate"))
            .respond_with(ResponseTemplate::new(500)
                .set_body_string("Internal Server Error"))
            .mount(&mock_server)
            .await;

        let client = InferenceHttpClient::new(&mock_server.uri());
        let result = client.generate("Hello!", 50).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, HttpClientError::ServiceUnavailable(_)));
    }
}
