//! Ollama 集成测试
//!
//! **注意**：这些测试需要本地运行 Ollama 服务
//! 运行前请确保：
//! 1. 已安装 Ollama (`curl -fsSL https://ollama.com/install.sh | sh`)
//! 2. 已拉取模型 (`ollama pull qwen3-coder-next:q8_0`)
//! 3. Ollama 服务正在运行 (`ollama serve`)

use block_chain_with_context::provider_layer::{
    ProviderLayerManager, InferenceProvider, InferenceRequest, InferenceEngineType,
};
use block_chain_with_context::provider_layer::ollama_provider::OllamaProvider;
use block_chain_with_context::provider_layer::ollama_stream::{
    StreamingOllamaProcessor, OllamaChatRequest, ChatMessage,
};
use block_chain_with_context::memory_layer::MemoryLayerManager;
use block_chain_with_context::node_layer::{AccessCredential, AccessType};

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

/// 测试 Ollama 提供商创建和注册
#[test]
fn test_ollama_provider_registration() {
    let mut manager = ProviderLayerManager::new();

    let provider = OllamaProvider::new(
        "ollama_test".to_string(),
        "http://localhost:11434",
        "qwen3-coder-next:q8_0".to_string(),
        50,
    );

    manager.register_provider(Box::new(provider)).unwrap();
    
    assert_eq!(manager.provider_count(), 1);
    assert!(manager.get_provider("ollama_test").is_some());
}

/// 测试 Ollama 提供商配置选项
#[test]
fn test_ollama_provider_with_options() {
    let provider = OllamaProvider::new(
        "ollama_config_test".to_string(),
        "http://localhost:11434",
        "qwen3-coder-next:q8_0".to_string(),
        100,
    )
    .with_token_split_threshold(2048)
    .with_timeout(120000)
    .with_max_retries(5);

    assert_eq!(provider.compute_capacity(), 100);
    assert_eq!(provider.token_split_threshold, 2048);
    assert_eq!(provider.timeout_ms, 120000);
    assert_eq!(provider.max_retries, 5);
}

/// 测试 Token 分割功能
#[test]
fn test_token_splitting_short_text() {
    let provider = OllamaProvider::new(
        "ollama_split_test".to_string(),
        "http://localhost:11434",
        "qwen3-coder-next:q8_0".to_string(),
        50,
    )
    .with_token_split_threshold(4096);

    let short_text = "Hello, world! This is a short text.";
    let chunks = provider.split_into_chunks(short_text);
    
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0], short_text);
}

#[test]
fn test_token_splitting_long_text() {
    let provider = OllamaProvider::new(
        "ollama_split_test".to_string(),
        "http://localhost:11434",
        "qwen3-coder-next:q8_0".to_string(),
        50,
    )
    .with_token_split_threshold(100);

    // 生成足够长的文本（超过 100 tokens）
    let long_text = "The quick brown fox jumps over the lazy dog. ".repeat(50);
    let chunks = provider.split_into_chunks(&long_text);
    
    assert!(chunks.len() > 1);
    
    // 验证分割后内容完整性
    let recombined: String = chunks.join(" ");
    assert!(recombined.contains("The quick brown fox"));
}

/// 测试流式处理器创建
#[test]
fn test_streaming_processor_creation() {
    let processor = StreamingOllamaProcessor::new(
        "ollama_stream_test".to_string(),
        "http://localhost:11434",
    );

    assert_eq!(processor.provider_id(), "ollama_stream_test");
    assert_eq!(processor.base_url(), "http://localhost:11434");
}

/// 测试流式请求构建
#[test]
fn test_streaming_request_builder() {
    let request = OllamaChatRequest {
        model: "qwen3-coder-next:q8_0".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Hello!".to_string(),
        }],
        stream: true,
        max_tokens: Some(100),
        temperature: Some(0.7),
    };

    assert_eq!(request.model, "qwen3-coder-next:q8_0");
    assert!(request.stream);
    assert_eq!(request.max_tokens, Some(100));
    assert_eq!(request.temperature, Some(0.7));
}

/// 测试聊天消息序列化
#[test]
fn test_chat_message_serialization() {
    let message = ChatMessage {
        role: "user".to_string(),
        content: "Hello, how are you?".to_string(),
    };

    let json = serde_json::to_string(&message).unwrap();
    assert!(json.contains("\"role\":\"user\""));
    assert!(json.contains("\"content\":\"Hello, how are you?\""));
}

/// 测试聊天请求序列化
#[test]
fn test_chat_request_serialization() {
    let request = OllamaChatRequest {
        model: "test-model".to_string(),
        messages: vec![ChatMessage {
            role: "system".to_string(),
            content: "You are a helpful assistant.".to_string(),
        }],
        stream: false,
        max_tokens: Some(50),
        temperature: None,
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"model\":\"test-model\""));
    assert!(json.contains("\"stream\":false"));
    assert!(json.contains("\"max_tokens\":50"));
    // temperature 为 None 时应被跳过
    assert!(!json.contains("\"temperature\""));
}

/// 集成测试：完整的推理流程（需要 Ollama 服务）
#[tokio::test]
#[ignore]
async fn test_ollama_full_inference_flow() {
    let provider = OllamaProvider::new(
        "ollama_integration".to_string(),
        "http://localhost:11434",
        "qwen3-coder-next:q8_0".to_string(),
        50,
    );

    let memory = MemoryLayerManager::new("test_node");
    let credential = create_test_credential();

    // 写入测试 KV 数据
    memory.write_kv(
        "context".to_string(),
        b"Test context for inference".to_vec(),
        &credential,
    ).unwrap();

    let request = InferenceRequest::new(
        "integration_test_req".to_string(),
        "Hello, how are you today?".to_string(),
        "qwen3-coder-next:q8_0".to_string(),
        100,
    )
    .with_memory_blocks(vec![0])
    .with_temperature(0.7);

    let response = provider.execute_inference(&request, &memory, &credential).await;

    // 如果 Ollama 服务可用，应该成功
    if let Ok(resp) = response {
        assert!(resp.success);
        assert!(!resp.completion.is_empty());
        assert!(resp.completion_tokens > 0);
        assert!(!resp.new_kv.is_empty());
        println!("Inference response: {}", resp.completion);
    }
}

/// 集成测试：流式推理（需要 Ollama 服务）
#[tokio::test]
#[ignore]
async fn test_ollama_streaming_inference() {
    let processor = StreamingOllamaProcessor::new(
        "ollama_stream_integration".to_string(),
        "http://localhost:11434",
    );

    let request = OllamaChatRequest {
        model: "qwen3-coder-next:q8_0".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Count from 1 to 5.".to_string(),
        }],
        stream: true,
        max_tokens: Some(50),
        temperature: Some(0.5),
    };

    let result = processor.chat_stream_collect(&request).await;

    if let Ok(response) = result {
        assert!(response.done);
        assert!(response.message.is_some());
        let content = response.message.unwrap().content;
        assert!(!content.is_empty());
        println!("Streaming response: {}", content);
    }
}

/// 集成测试：ProviderLayerManager 与 Ollama 提供商集成
#[tokio::test]
#[ignore]
async fn test_provider_manager_with_ollama() {
    let mut manager = ProviderLayerManager::new();

    // 注册 Ollama 提供商
    let provider = OllamaProvider::new(
        "ollama_main".to_string(),
        "http://localhost:11434",
        "qwen3-coder-next:q8_0".to_string(),
        50,
    );
    manager.register_provider(Box::new(provider)).unwrap();
    manager.set_current_provider("ollama_main").unwrap();

    let memory = MemoryLayerManager::new("test_node");
    let credential = create_test_credential();

    let request = InferenceRequest::new(
        "manager_test".to_string(),
        "What is 2 + 2?".to_string(),
        "qwen3-coder-next:q8_0".to_string(),
        50,
    );

    let response = manager.execute_inference_async(&request, &memory, &credential).await;

    if let Ok(resp) = response {
        assert!(resp.success);
        assert!(resp.completion.contains("4") || resp.completion.to_lowercase().contains("four"));
    }
}

/// 测试：Mock 场景 - 模拟 Ollama 服务不可用
#[tokio::test]
async fn test_ollama_provider_service_unavailable() {
    let provider = OllamaProvider::new(
        "ollama_unavailable".to_string(),
        "http://localhost:9999", // 不存在的端口
        "qwen3-coder-next:q8_0".to_string(),
        50,
    )
    .with_max_retries(1)
    .with_timeout(1000); // 短超时

    let memory = MemoryLayerManager::new("test_node");
    let credential = create_test_credential();

    let request = InferenceRequest::new(
        "test_req".to_string(),
        "Hello".to_string(),
        "qwen3-coder-next:q8_0".to_string(),
        10,
    );

    let result = provider.execute_inference(&request, &memory, &credential).await;
    
    // 应该失败（服务不可用）
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.contains("HTTP request failed") || error.contains("timeout"));
}

/// 测试：并发上传分片
#[tokio::test]
async fn test_concurrent_chunk_upload() {
    let provider = OllamaProvider::new(
        "ollama_concurrent".to_string(),
        "http://localhost:11434",
        "qwen3-coder-next:q8_0".to_string(),
        50,
    )
    .with_token_split_threshold(50);

    let memory = MemoryLayerManager::new("test_node");
    let credential = create_test_credential();

    // 生成长文本以触发分割
    let long_text = "This is a test chunk. ".repeat(100);
    let chunks = provider.split_into_chunks(&long_text);
    
    assert!(chunks.len() > 1);

    // 测试并发上传
    let result = provider.upload_chunks_async(
        chunks,
        &memory,
        &credential,
        "concurrent_test",
    ).await;

    // 应该成功（即使没有真实的 Ollama 服务）
    assert!(result.is_ok());
}
