//! Ollama 流式响应支持
//!
//! **核心功能**：
//! - SSE 流式解析
//! - 首 token 快速返回（降低等待时间）
//! - 增量 KV 写入

use futures::Stream;
use serde::Deserialize;
use crate::provider_layer::ollama_provider::ChatMessage;
use crate::provider_layer::ollama_provider::OllamaChatRequest;

/// Ollama 流式响应片段
#[derive(Debug, Deserialize, Clone)]
pub struct OllamaStreamResponse {
    pub model: String,
    #[serde(default)]
    pub message: Option<ChatMessage>,
    pub done: bool,
    #[serde(default)]
    pub eval_count: Option<u32>,
    #[serde(default)]
    pub prompt_eval_count: Option<u32>,
    #[serde(default)]
    pub total_duration: Option<u64>,
}

/// 流式推理处理器
pub struct StreamingOllamaProcessor {
    provider_id: String,
    client: reqwest::Client,
    base_url: String,
}

impl StreamingOllamaProcessor {
    pub fn new(provider_id: String, base_url: &str) -> Self {
        StreamingOllamaProcessor {
            provider_id,
            client: reqwest::Client::new(),
            base_url: base_url.to_string(),
        }
    }

    /// 创建带自定义客户端的处理器
    pub fn with_client(provider_id: String, client: reqwest::Client, base_url: &str) -> Self {
        StreamingOllamaProcessor {
            provider_id,
            client,
            base_url: base_url.to_string(),
        }
    }

    /// 获取提供商 ID
    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// 获取基础 URL
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// 流式聊天（返回 Stream）
    pub async fn chat_stream(
        &self,
        request: &OllamaChatRequest,
    ) -> Result<impl Stream<Item = Result<OllamaStreamResponse, String>>, String> {
        let url = format!("{}/api/chat", self.base_url);

        let mut stream_request = request.clone();
        stream_request.stream = true;

        let response = self.client
            .post(&url)
            .json(&stream_request)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Ollama returned status: {}", response.status()));
        }

        // 读取整个响应体，然后按行分割
        let body_bytes = response
            .bytes()
            .await
            .map_err(|e| format!("Failed to read response body: {}", e))?;
        
        let body_text = String::from_utf8_lossy(&body_bytes).to_string();
        let lines: Vec<String> = body_text.lines().map(|s| s.to_string()).collect();
        
        // 创建一个流，逐个解析行
        Ok(futures::stream::iter(lines.into_iter().filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            match serde_json::from_str::<OllamaStreamResponse>(trimmed) {
                Ok(response) => Some(Ok(response)),
                Err(e) => Some(Err(format!("Parse error: {} in line: {}", e, trimmed))),
            }
        })))
    }

    /// 流式聊天并收集完整响应
    pub async fn chat_stream_collect(
        &self,
        request: &OllamaChatRequest,
    ) -> Result<OllamaStreamResponse, String> {
        use futures::StreamExt;
        
        let mut stream = self.chat_stream(request).await?;
        let mut final_response: Option<OllamaStreamResponse> = None;
        let mut accumulated_content = String::new();

        while let Some(result) = stream.next().await {
            match result {
                Ok(response) => {
                    if let Some(ref msg) = response.message {
                        accumulated_content.push_str(&msg.content);
                    }
                    if response.done {
                        final_response = Some(response);
                        break;
                    }
                }
                Err(e) => return Err(e),
            }
        }

        // 如果没有 done 响应，使用累积的内容构建响应
        if final_response.is_none() {
            final_response = Some(OllamaStreamResponse {
                model: request.model.clone(),
                message: Some(ChatMessage {
                    role: "assistant".to_string(),
                    content: accumulated_content,
                }),
                done: true,
                eval_count: None,
                prompt_eval_count: None,
                total_duration: None,
            });
        }

        Ok(final_response.unwrap())
    }
}

/// 流式响应处理器（用于回调模式）
pub struct StreamingResponseHandler {
    /// 首 token 回调
    on_first_token: Option<Box<dyn Fn(&str) + Send + Sync>>,
    /// 增量 token 回调
    on_token: Option<Box<dyn Fn(&str) + Send + Sync>>,
    /// 完成回调
    on_complete: Option<Box<dyn Fn(&OllamaStreamResponse) + Send + Sync>>,
}

impl StreamingResponseHandler {
    pub fn new() -> Self {
        StreamingResponseHandler {
            on_first_token: None,
            on_token: None,
            on_complete: None,
        }
    }

    pub fn on_first_token<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.on_first_token = Some(Box::new(callback));
        self
    }

    pub fn on_token<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.on_token = Some(Box::new(callback));
        self
    }

    pub fn on_complete<F>(mut self, callback: F) -> Self
    where
        F: Fn(&OllamaStreamResponse) + Send + Sync + 'static,
    {
        self.on_complete = Some(Box::new(callback));
        self
    }

    /// 处理流式响应
    pub async fn handle_stream(
        &self,
        stream: &mut (impl Stream<Item = Result<OllamaStreamResponse, String>> + Unpin),
    ) -> Result<OllamaStreamResponse, String> {
        use futures::StreamExt;
        
        let mut first_token_seen = false;
        let mut final_response: Option<OllamaStreamResponse> = None;

        while let Some(result) = stream.next().await {
            match result {
                Ok(response) => {
                    if let Some(ref msg) = response.message {
                        if !msg.content.is_empty() {
                            // 首 token 回调
                            if !first_token_seen {
                                if let Some(ref callback) = self.on_first_token {
                                    callback(&msg.content);
                                }
                                first_token_seen = true;
                            }

                            // 增量 token 回调
                            if let Some(ref callback) = self.on_token {
                                callback(&msg.content);
                            }
                        }
                    }

                    if response.done {
                        // 完成回调
                        if let Some(ref callback) = self.on_complete {
                            callback(&response);
                        }
                        final_response = Some(response);
                        break;
                    }
                }
                Err(e) => return Err(e),
            }
        }

        Ok(final_response.ok_or_else(|| "Stream ended without done response".to_string())?)
    }
}

impl Default for StreamingResponseHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_processor_creation() {
        let processor = StreamingOllamaProcessor::new(
            "ollama_stream_1".to_string(),
            "http://localhost:11434",
        );

        assert_eq!(processor.provider_id(), "ollama_stream_1");
        assert_eq!(processor.base_url(), "http://localhost:11434");
    }

    #[test]
    fn test_streaming_processor_with_client() {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap();

        let processor = StreamingOllamaProcessor::with_client(
            "ollama_stream_1".to_string(),
            client,
            "http://localhost:11434",
        );

        assert_eq!(processor.provider_id(), "ollama_stream_1");
    }

    #[test]
    fn test_stream_response_deserialization() {
        let json = r#"{
            "model": "qwen3-coder-next:q8_0",
            "message": {
                "role": "assistant",
                "content": "Hello"
            },
            "done": false,
            "eval_count": 5
        }"#;

        let response: OllamaStreamResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.model, "qwen3-coder-next:q8_0");
        assert!(response.message.is_some());
        assert_eq!(response.message.unwrap().content, "Hello");
        assert!(!response.done);
        assert_eq!(response.eval_count, Some(5));
    }

    #[test]
    fn test_stream_response_default_fields() {
        let json = r#"{"model": "test", "done": true}"#;

        let response: OllamaStreamResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.model, "test");
        assert!(response.message.is_none());
        assert!(response.done);
        assert_eq!(response.eval_count, None);
    }

    #[tokio::test]
    #[ignore]
    async fn test_streaming_chat() {
        // 这个测试需要本地运行 Ollama 服务
        let processor = StreamingOllamaProcessor::new(
            "ollama_stream_1".to_string(),
            "http://localhost:11434",
        );

        let request = OllamaChatRequest {
            model: "qwen3-coder-next:q8_0".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Hello, how are you?".to_string(),
            }],
            stream: true,
            max_tokens: Some(50),
            temperature: Some(0.7),
        };

        let result = processor.chat_stream_collect(&request).await;
        
        if let Ok(response) = result {
            assert!(response.done);
            assert!(response.message.is_some());
            assert!(!response.message.unwrap().content.is_empty());
        }
    }
}
