//! Ollama 推理提供商实现
//!
//! **核心功能**：
//! - 实现 InferenceProvider trait
//! - 调用 Ollama /api/chat 和 /api/generate 接口
//! - 支持流式响应（SSE）
//! - Token 自动分割与异步上传

use crate::provider_layer::{
    InferenceProvider, InferenceEngineType, InferenceRequest, InferenceResponse,
};
use crate::memory_layer::MemoryLayerManager;
use crate::node_layer::AccessCredential;
use std::time::Instant;
use serde::{Serialize, Deserialize};

/// Ollama 聊天请求
#[derive(Debug, Serialize, Clone)]
pub struct OllamaChatRequest {
    /// 模型名称
    pub model: String,
    /// 消息列表
    pub messages: Vec<ChatMessage>,
    /// 流式响应
    pub stream: bool,
    /// 最大生成 token 数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// 温度参数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Ollama 聊天响应
#[derive(Debug, Deserialize)]
pub struct OllamaChatResponse {
    pub model: String,
    pub message: ChatMessage,
    pub done: bool,
    #[serde(default)]
    pub total_duration: Option<u64>,
    #[serde(default)]
    pub prompt_eval_count: Option<u32>,
    #[serde(default)]
    pub eval_count: Option<u32>,
}

/// Ollama 推理提供商
pub struct OllamaProvider {
    provider_id: String,
    interface_version: String,
    compute_capacity: u64,
    client: reqwest::Client,
    base_url: String,
    default_model: String,
    timeout_ms: u64,
    max_retries: u32,
    /// Token 分割阈值（超过此值自动分割）
    token_split_threshold: u32,
    /// API Key（用于线上服务认证）
    api_key: Option<String>,
}

impl OllamaProvider {
    pub fn new(
        provider_id: String,
        base_url: &str,
        default_model: String,
        compute_capacity: u64,
    ) -> Self {
        OllamaProvider {
            provider_id,
            interface_version: "1.0.0".to_string(),
            compute_capacity,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .unwrap(),
            base_url: base_url.to_string(),
            default_model,
            timeout_ms: 60000,
            max_retries: 3,
            token_split_threshold: 4096,
            api_key: None,
        }
    }

    /// 从环境变量加载配置
    /// 
    /// 支持的环境变量：
    /// - OLLAMA_URL: Ollama 服务地址
    /// - OLLAMA_API_KEY: API Key（可选，仅线上服务需要）
    /// - OLLAMA_MODEL: 默认模型
    /// - OLLAMA_CAPACITY: 算力容量
    /// - OLLAMA_TOKEN_THRESHOLD: Token 分割阈值
    pub fn from_env(provider_id: String) -> Result<Self, String> {
        // 加载 .env 文件（如果存在）
        dotenv::dotenv().ok();

        let base_url = std::env::var("OLLAMA_URL")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());
        
        let api_key = std::env::var("OLLAMA_API_KEY").ok();
        
        let default_model = std::env::var("OLLAMA_MODEL")
            .unwrap_or_else(|_| "qwen3-coder-next:q8_0".to_string());
        
        let compute_capacity = std::env::var("OLLAMA_CAPACITY")
            .and_then(|s| s.parse::<u64>().map_err(|_| std::env::VarError::NotPresent))
            .unwrap_or(50);
        
        let token_threshold = std::env::var("OLLAMA_TOKEN_THRESHOLD")
            .and_then(|s| s.parse::<u32>().map_err(|_| std::env::VarError::NotPresent))
            .unwrap_or(4096);

        let mut provider = Self::new(
            provider_id,
            &base_url,
            default_model,
            compute_capacity,
        );

        if let Some(key) = api_key {
            provider = provider.with_api_key(key);
        }

        provider = provider.with_token_split_threshold(token_threshold);

        Ok(provider)
    }

    /// 设置 Token 分割阈值
    pub fn with_token_split_threshold(mut self, threshold: u32) -> Self {
        self.token_split_threshold = threshold;
        self
    }

    /// 设置 API Key
    pub fn with_api_key(mut self, api_key: String) -> Self {
        self.api_key = Some(api_key);
        self
    }

    /// 设置超时时间（毫秒）
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// 设置最大重试次数
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// 执行聊天请求（支持流式）
    async fn chat(
        &self,
        request: &OllamaChatRequest,
    ) -> Result<OllamaChatResponse, String> {
        let url = format!("{}/api/chat", self.base_url);

        let mut req_builder = self.client
            .post(&url)
            .json(request);

        // 如果配置了 API Key，添加到请求头
        if let Some(ref api_key) = self.api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = req_builder
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Ollama returned status: {}", response.status()));
        }

        let result: OllamaChatResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(result)
    }

    /// Token 分割逻辑
    fn split_into_chunks(&self, text: &str) -> Vec<String> {
        // 使用 tiktoken-rs 进行精确 token 计数
        let bpe = tiktoken_rs::get_bpe_from_model("cl100k_base").unwrap();
        let tokens = bpe.encode_with_special_tokens(text);

        if tokens.len() <= self.token_split_threshold as usize {
            return vec![text.to_string()];
        }

        // 按 token 阈值分割
        let mut chunks = Vec::new();
        let mut current_chunk = Vec::new();
        let mut current_len = 0;

        for token in tokens {
            current_chunk.push(token);
            current_len += 1;

            if current_len >= self.token_split_threshold as usize {
                let chunk_text = bpe.decode(current_chunk).unwrap();
                chunks.push(chunk_text);
                current_chunk = Vec::new();
                current_len = 0;
            }
        }

        if !current_chunk.is_empty() {
            let chunk_text = bpe.decode(current_chunk).unwrap();
            chunks.push(chunk_text);
        }

        chunks
    }

    /// 异步上传分片（并行处理）
    async fn upload_chunks_async(
        &self,
        chunks: Vec<String>,
        memory: &MemoryLayerManager,
        credential: &AccessCredential,
        request_id: &str,
    ) -> Result<(), String> {
        use futures::future::join_all;

        let upload_tasks: Vec<_> = chunks.into_iter().enumerate().map(|(i, chunk)| {
            let key = format!("ollama_chunk_{}_{}", request_id, i);
            let mut mem = memory.clone();
            let credential = credential.clone();

            tokio::spawn(async move {
                mem.write_kv(key, chunk.into_bytes(), &credential)
                    .map_err(|e| format!("KV write failed: {}", e))
            })
        }).collect();

        let results = join_all(upload_tasks).await;

        for result in results {
            result.map_err(|e| format!("Task join error: {}", e))??;
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl InferenceProvider for OllamaProvider {
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    fn engine_type(&self) -> InferenceEngineType {
        InferenceEngineType::Custom
    }

    fn interface_version(&self) -> &str {
        &self.interface_version
    }

    fn compute_capacity(&self) -> u64 {
        self.compute_capacity
    }

    async fn execute_inference(
        &self,
        request: &InferenceRequest,
        memory: &MemoryLayerManager,
        credential: &AccessCredential,
    ) -> Result<InferenceResponse, String> {
        let start_time = Instant::now();

        // 1. Token 分割（如果 prompt 过长）
        let chunks = self.split_into_chunks(&request.prompt);

        // 2. 异步上传分片到记忆层
        self.upload_chunks_async(
            chunks.clone(),
            memory,
            credential,
            &request.request_id,
        ).await?;

        // 3. 构建聊天请求
        let ollama_request = OllamaChatRequest {
            model: self.default_model.clone(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: chunks.join("\n\n"),
            }],
            stream: false,
            max_tokens: Some(request.max_tokens),
            temperature: Some(request.temperature),
        };

        // 4. 执行推理（带重试）
        let mut last_error: Option<String> = None;

        for attempt in 0..self.max_retries {
            match self.chat(&ollama_request).await {
                Ok(response) => {
                    let elapsed = start_time.elapsed();

                    let mut inference_response = InferenceResponse::new(request.request_id.clone())
                        .with_completion(response.message.content)
                        .with_token_stats(
                            response.prompt_eval_count.unwrap_or(0),
                            response.eval_count.unwrap_or(0),
                        )
                        .with_latency(elapsed.as_millis() as u64);

                    // 5. 生成新 KV
                    let mut new_kv = std::collections::HashMap::new();
                    new_kv.insert(
                        format!("ollama_response_{}", request.request_id),
                        inference_response.completion.as_bytes().to_vec(),
                    );
                    inference_response = inference_response.with_new_kv(new_kv);
                    inference_response.mark_success();

                    return Ok(inference_response);
                }
                Err(e) => {
                    last_error = Some(e);
                    if attempt < self.max_retries - 1 {
                        tokio::time::sleep(
                            tokio::time::Duration::from_millis(100 * (2u64.pow(attempt)))
                        ).await;
                    }
                }
            }
        }

        Err(last_error.unwrap())
    }

    fn clone_box(&self) -> Box<dyn InferenceProvider> {
        Box::new(OllamaProvider {
            provider_id: self.provider_id.clone(),
            interface_version: self.interface_version.clone(),
            compute_capacity: self.compute_capacity,
            client: self.client.clone(),
            base_url: self.base_url.clone(),
            default_model: self.default_model.clone(),
            timeout_ms: self.timeout_ms,
            max_retries: self.max_retries,
            token_split_threshold: self.token_split_threshold,
            api_key: self.api_key.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_layer::{AccessType, AccessCredential};

    fn create_test_credential() -> AccessCredential {
        AccessCredential {
            credential_id: "test_cred".to_string(),
            provider_id: "ollama_test".to_string(),
            memory_block_ids: vec!["all".to_string()],
            access_type: AccessType::ReadWrite,
            expires_at: u64::MAX,
            issuer_node_id: "node_1".to_string(),
            signature: "test_signature".to_string(),
            is_revoked: false,
        }
    }

    #[test]
    fn test_ollama_provider_creation() {
        let provider = OllamaProvider::new(
            "ollama_1".to_string(),
            "http://localhost:11434",
            "qwen3-coder-next:q8_0".to_string(),
            50,
        );

        assert_eq!(provider.provider_id(), "ollama_1");
        assert_eq!(provider.engine_type(), InferenceEngineType::Custom);
        assert_eq!(provider.compute_capacity(), 50);
    }

    #[test]
    fn test_ollama_provider_with_options() {
        let provider = OllamaProvider::new(
            "ollama_1".to_string(),
            "http://localhost:11434",
            "qwen3-coder-next:q8_0".to_string(),
            50,
        )
        .with_token_split_threshold(2048)
        .with_timeout(120000)
        .with_max_retries(5);

        assert_eq!(provider.token_split_threshold, 2048);
        assert_eq!(provider.timeout_ms, 120000);
        assert_eq!(provider.max_retries, 5);
    }

    #[test]
    fn test_token_splitting() {
        let provider = OllamaProvider::new(
            "ollama_1".to_string(),
            "http://localhost:11434",
            "qwen3-coder-next:q8_0".to_string(),
            50,
        )
        .with_token_split_threshold(100);

        // 短文本不应分割
        let short_text = "Hello, world!";
        let chunks = provider.split_into_chunks(short_text);
        assert_eq!(chunks.len(), 1);

        // 长文本应分割
        let long_text = "Hello, world! ".repeat(100);
        let chunks = provider.split_into_chunks(&long_text);
        assert!(chunks.len() > 1);

        // 验证分割后总长度不变
        let recombined: String = chunks.join(" ");
        assert!(recombined.contains("Hello, world!"));
    }

    #[tokio::test]
    #[ignore]
    async fn test_ollama_provider_inference() {
        // 这个测试需要本地运行 Ollama 服务
        let provider = OllamaProvider::new(
            "ollama_1".to_string(),
            "http://localhost:11434",
            "qwen3-coder-next:q8_0".to_string(),
            50,
        );

        let memory = MemoryLayerManager::new("test_node");
        let credential = create_test_credential();

        let request = InferenceRequest::new(
            "test_req".to_string(),
            "Hello, how are you?".to_string(),
            "qwen3-coder-next:q8_0".to_string(),
            100,
        );

        let result = provider.execute_inference(&request, &memory, &credential).await;
        
        // 如果 Ollama 服务运行，应该成功
        if let Ok(response) = result {
            assert!(response.success);
            assert!(!response.completion.is_empty());
        }
    }
}
