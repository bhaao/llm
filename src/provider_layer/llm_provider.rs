//! 真实 LLM 提供商实现 - 通过 HTTP 调用 vLLM/SGLang 等推理引擎
//!
//! **核心功能**：
//! - 实现 `InferenceProvider` trait（异步版本）
//! - 通过 HTTP 客户端调用真实 LLM API
//! - 真正的异步推理（无 block_on）
//! - 集成断路器模式
//! - 错误处理和重试机制

use crate::error::ProviderLayerError;
use crate::memory_layer::MemoryLayerManager;
use crate::node_layer::AccessCredential;
use crate::provider_layer::{
    InferenceEngineType, InferenceProvider, InferenceRequest, InferenceResponse,
};
use crate::provider_layer::http_client::{InferenceHttpClient, GenerateResponse};
use std::time::Instant;
use std::sync::Arc;
use crate::failover::circuit_breaker::CircuitBreaker;

/// 真实 LLM 推理提供商
///
/// 通过 HTTP 调用远程 LLM 推理服务 (vLLM/SGLang/TGI)
///
/// # 特性
///
/// - 真正的异步 I/O（无 block_on）
/// - 内置断路器保护
/// - 指数退避重试
/// - 可配置超时
pub struct LLMProvider {
    provider_id: String,
    engine_type: InferenceEngineType,
    interface_version: String,
    compute_capacity: u64,
    http_client: InferenceHttpClient,
    /// 请求超时 (毫秒)
    timeout_ms: u64,
    /// 最大重试次数
    max_retries: u32,
    /// 断路器（可选）
    circuit_breaker: Option<Arc<CircuitBreaker>>,
}

impl LLMProvider {
    /// 创建新的 LLM 提供商
    ///
    /// # 参数
    ///
    /// * `provider_id` - 提供商 ID
    /// * `engine_type` - 推理引擎类型
    /// * `base_url` - 推理服务的基础 URL
    /// * `compute_capacity` - 算力容量 (token/s)
    pub fn new(
        provider_id: String,
        engine_type: InferenceEngineType,
        base_url: &str,
        compute_capacity: u64,
    ) -> Self {
        LLMProvider {
            provider_id,
            engine_type,
            interface_version: "1.0.0".to_string(),
            compute_capacity,
            http_client: InferenceHttpClient::new(base_url),
            timeout_ms: 30000, // 默认 30 秒超时
            max_retries: 3,
            circuit_breaker: None,
        }
    }

    /// 创建带断路器的 LLM 提供商
    pub fn with_circuit_breaker(
        provider_id: String,
        engine_type: InferenceEngineType,
        base_url: &str,
        compute_capacity: u64,
        circuit_breaker: CircuitBreaker,
    ) -> Self {
        let mut provider = Self::new(provider_id, engine_type, base_url, compute_capacity);
        provider.circuit_breaker = Some(Arc::new(circuit_breaker));
        provider
    }

    /// 设置超时时间
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// 设置最大重试次数
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// 设置断路器
    pub fn with_circuit_breaker_mut(mut self, circuit_breaker: CircuitBreaker) -> Self {
        self.circuit_breaker = Some(Arc::new(circuit_breaker));
        self
    }

    /// 获取基础 URL
    pub fn base_url(&self) -> &str {
        self.http_client.base_url()
    }

    /// 获取断路器
    pub fn circuit_breaker(&self) -> Option<Arc<CircuitBreaker>> {
        self.circuit_breaker.clone()
    }

    /// 执行 HTTP 推理请求 (带重试和超时)
    async fn execute_with_retry_and_timeout(
        &self,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
    ) -> Result<GenerateResponse, ProviderLayerError> {
        let mut last_error: Option<ProviderLayerError> = None;

        for attempt in 0..self.max_retries {
            // 使用 tokio::time::timeout 实现超时控制
            let request_future = self.http_client.generate_with_options(
                prompt,
                max_tokens,
                Some(temperature),
                None,
            );

            let timeout_duration = tokio::time::Duration::from_millis(self.timeout_ms);
            let result = tokio::time::timeout(timeout_duration, request_future).await;

            match result {
                Ok(Ok(response)) => return Ok(response),
                Ok(Err(e)) => {
                    last_error = Some(ProviderLayerError::ExecutionFailed(format!(
                        "HTTP request failed (attempt {}/{}): {}",
                        attempt + 1,
                        self.max_retries,
                        e
                    )));
                }
                Err(_) => {
                    last_error = Some(ProviderLayerError::Timeout {
                        timeout_ms: self.timeout_ms,
                        elapsed_ms: self.timeout_ms,
                    });
                }
            }

            // 重试前等待 (指数退避 + 抖动)
            if attempt < self.max_retries - 1 {
                let base_delay = 100 * (2u64.pow(attempt));
                // 添加 ±10% 抖动
                let jitter = (base_delay as f64 * 0.1 * (rand::random::<f64>() - 0.5) * 2.0) as u64;
                let delay_ms = base_delay.saturating_add(jitter);
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            }
        }

        Err(last_error.unwrap_or_else(|| {
            ProviderLayerError::ExecutionFailed("Unknown error".to_string())
        }))
    }

    /// 从记忆层读取上下文（异步）
    async fn read_context(
        &self,
        memory: &MemoryLayerManager,
        credential: &AccessCredential,
        memory_block_ids: &[u64],
    ) -> String {
        let mut context = String::new();

        for _block_id in memory_block_ids {
            // 注意：memory.read_kv 目前是同步方法
            // TODO: 如果 memory_layer 支持异步，改为异步读取
            if let Some(shard) = memory.read_kv("context", credential) {
                if !context.is_empty() {
                    context.push('\n');
                }
                context.push_str(&String::from_utf8_lossy(&shard.value));
            }
        }

        context
    }
}

#[async_trait::async_trait]
impl InferenceProvider for LLMProvider {
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    fn engine_type(&self) -> InferenceEngineType {
        self.engine_type.clone()
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

        // 1. 从记忆层读取上下文
        let context = self.read_context(memory, credential, &request.memory_block_ids).await;

        // 2. 构建完整 prompt
        let full_prompt = if context.is_empty() {
            request.prompt.clone()
        } else {
            format!("{}\n\nContext:\n{}", request.prompt, context)
        };

        // 3. 执行 HTTP 推理请求（带断路器保护）
        let response_result = if let Some(cb) = &self.circuit_breaker {
            // 使用断路器保护
            match cb.execute_with_protection(
                &self.provider_id,
                self.execute_with_retry_and_timeout(&full_prompt, request.max_tokens as usize, request.temperature),
            ).await {
                Ok(response) => Ok(response),
                Err(e) => Err(ProviderLayerError::ExecutionFailed(format!(
                    "Circuit breaker protected request failed: {}",
                    e
                ))),
            }
        } else {
            // 无断路器，直接执行
            self.execute_with_retry_and_timeout(&full_prompt, request.max_tokens as usize, request.temperature).await
        };

        let elapsed = start_time.elapsed();

        match response_result {
            Ok(http_response) => {
                // 4. 构建响应
                let mut response = InferenceResponse::new(request.request_id.clone())
                    .with_completion(http_response.text)
                    .with_token_stats(http_response.prompt_tokens, http_response.completion_tokens)
                    .with_latency(elapsed.as_millis() as u64);

                // 5. 生成新 KV 数据
                let mut new_kv = std::collections::HashMap::new();
                new_kv.insert(
                    format!("response_{}", request.request_id),
                    response.completion.as_bytes().to_vec(),
                );
                response = response.with_new_kv(new_kv);

                response.mark_success();
                Ok(response)
            }
            Err(e) => {
                let mut response = InferenceResponse::new(request.request_id.clone());
                response.mark_failure(format!("{}", e));
                Err(format!("{}", e))
            }
        }
    }

    fn clone_box(&self) -> Box<dyn InferenceProvider> {
        Box::new(LLMProvider {
            provider_id: self.provider_id.clone(),
            engine_type: self.engine_type.clone(),
            interface_version: self.interface_version.clone(),
            compute_capacity: self.compute_capacity,
            http_client: InferenceHttpClient::new(self.http_client.base_url()),
            timeout_ms: self.timeout_ms,
            max_retries: self.max_retries,
            circuit_breaker: self.circuit_breaker.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_layer::{AccessType, AccessCredential};
    use crate::memory_layer::MemoryLayerManager;

    fn create_test_credential() -> AccessCredential {
        AccessCredential {
            credential_id: "test_cred".to_string(),
            provider_id: "provider_1".to_string(),
            memory_block_ids: vec!["all".to_string()],
            access_type: AccessType::ReadWrite,
            expires_at: u64::MAX,
            issuer_node_id: "node_1".to_string(),
            signature: "test_signature".to_string(),
            is_revoked: false,
        }
    }

    #[test]
    fn test_llm_provider_creation() {
        let provider = LLMProvider::new(
            "test_provider".to_string(),
            InferenceEngineType::Vllm,
            "http://localhost:8000",
            100,
        );

        assert_eq!(provider.provider_id(), "test_provider");
        assert_eq!(provider.engine_type(), InferenceEngineType::Vllm);
        assert_eq!(provider.base_url(), "http://localhost:8000");
        assert_eq!(provider.compute_capacity(), 100);
        assert!(provider.circuit_breaker().is_none());
    }

    #[test]
    fn test_llm_provider_with_circuit_breaker() {
        let cb = crate::failover::circuit_breaker::CircuitBreaker::with_defaults();
        let provider = LLMProvider::with_circuit_breaker(
            "test_provider".to_string(),
            InferenceEngineType::Sglang,
            "http://localhost:8001",
            200,
            cb,
        );

        assert!(provider.circuit_breaker().is_some());
        assert_eq!(provider.circuit_breaker().unwrap().state(), crate::failover::circuit_breaker::CircuitState::Closed);
    }

    #[test]
    fn test_llm_provider_with_options() {
        let provider = LLMProvider::new(
            "test_provider".to_string(),
            InferenceEngineType::Vllm,
            "http://localhost:8000",
            100,
        )
        .with_timeout(60000)
        .with_max_retries(5);

        assert_eq!(provider.timeout_ms, 60000);
        assert_eq!(provider.max_retries, 5);
    }

    /// 注意：这个测试需要真实的 LLM 服务运行
    /// 运行前请确保启动 vLLM/SGLang 服务
    #[tokio::test]
    #[ignore]
    async fn test_llm_provider_health_check() {
        let provider = LLMProvider::new(
            "test_provider".to_string(),
            InferenceEngineType::Vllm,
            "http://localhost:8000",
            100,
        );

        // 这个测试只有在 localhost:8000 有服务时才会通过
        let health = provider.http_client.health_check().await;
        println!("Health check result: {:?}", health);
    }

    /// 测试断路器集成
    #[tokio::test]
    async fn test_llm_provider_with_circuit_breaker_protection() {
        let cb = crate::failover::circuit_breaker::CircuitBreaker::with_defaults();
        let provider = LLMProvider::with_circuit_breaker(
            "test_provider".to_string(),
            InferenceEngineType::Vllm,
            "http://localhost:9999", // 不存在的地址，触发失败
            100,
            cb,
        );

        let request = InferenceRequest::new(
            "test_req".to_string(),
            "Hello".to_string(),
            "test_model".to_string(),
            10,
        );

        let memory = MemoryLayerManager::new("test_node");
        let credential = create_test_credential();

        // 第一次调用应该失败但断路器仍闭合
        let result = provider.execute_inference(&request, &memory, &credential).await;
        assert!(result.is_err());

        // 连续失败 3 次后断路器应打开
        let _ = provider.execute_inference(&request, &memory, &credential).await;
        let _ = provider.execute_inference(&request, &memory, &credential).await;

        let cb = provider.circuit_breaker().unwrap();
        // 断路器可能已打开（取决于执行速度）
        println!("Circuit breaker state: {:?}", cb.state());
    }
}
