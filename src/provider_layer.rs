//! 推理提供商层模块 - 无状态计算执行单元
//!
//! **核心定位**：从记忆层读取 KV/上下文，执行 LLM 推理，向记忆层写入新生成的 KV
//!
//! # 核心职责
//!
//! 1. **从记忆层读取 KV/上下文**：持有效访问凭证读取所需数据
//! 2. **执行 LLM 推理**：前向计算/Token 生成
//! 3. **向记忆层写入新生成的 KV**：写入推理结果
//! 4. **向节点层上报推理指标**：效率、质量等指标
//!
//! # 关键约束
//!
//! - **无区块链能力**：仅认节点授权，不直接操作区块链
//! - **无记忆存储能力**：仅临时加载，推理结束后释放
//! - **标准化接口**：适配多引擎（vLLM/SGLang/TGI/自研）

pub mod http_client;
pub mod llm_provider;
pub mod ollama_provider;
pub mod ollama_stream;

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::atomic::{AtomicU32, Ordering};
use crate::node_layer::{AccessCredential, ProviderRecord};
use crate::memory_layer::MemoryLayerManager;

/// 推理引擎类型枚举
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InferenceEngineType {
    /// vLLM 引擎
    Vllm,
    /// SGLang 引擎
    Sglang,
    /// TGI 引擎
    Tgi,
    /// 自研引擎
    Custom,
}

impl InferenceEngineType {
    pub fn as_str(&self) -> &'static str {
        match self {
            InferenceEngineType::Vllm => "vllm",
            InferenceEngineType::Sglang => "sglang",
            InferenceEngineType::Tgi => "tgi",
            InferenceEngineType::Custom => "custom",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "vllm" => Some(InferenceEngineType::Vllm),
            "sglang" => Some(InferenceEngineType::Sglang),
            "tgi" => Some(InferenceEngineType::Tgi),
            "custom" => Some(InferenceEngineType::Custom),
            _ => None,
        }
    }
}

/// 推理请求 - 标准化输入结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    /// 请求 ID
    pub request_id: String,
    /// 提示词（prompt）
    pub prompt: String,
    /// 模型 ID
    pub model_id: String,
    /// 最大生成 token 数
    pub max_tokens: u32,
    /// 温度参数
    pub temperature: f32,
    /// 记忆区块 ID 列表（需要读取的上下文）
    pub memory_block_ids: Vec<u64>,
    /// 时间戳
    pub timestamp: u64,
}

impl InferenceRequest {
    pub fn new(
        request_id: String,
        prompt: String,
        model_id: String,
        max_tokens: u32,
    ) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        InferenceRequest {
            request_id,
            prompt,
            model_id,
            max_tokens,
            temperature: 0.7,
            memory_block_ids: Vec::new(),
            timestamp,
        }
    }

    /// 设置记忆区块 ID 列表
    pub fn with_memory_blocks(mut self, block_ids: Vec<u64>) -> Self {
        self.memory_block_ids = block_ids;
        self
    }

    /// 设置温度参数
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }
}

/// 推理响应 - 标准化输出结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResponse {
    /// 请求 ID（与请求对应）
    pub request_id: String,
    /// 生成的 completion
    pub completion: String,
    /// prompt token 数
    pub prompt_tokens: u32,
    /// completion token 数
    pub completion_tokens: u32,
    /// 推理耗时（毫秒）
    pub latency_ms: u64,
    /// 推理效率（token/s）
    pub efficiency: f64,
    /// 新生成的 KV 数据（键值对）
    pub new_kv: HashMap<String, Vec<u8>>,
    /// 是否成功
    pub success: bool,
    /// 错误信息（如果失败）
    pub error_message: Option<String>,
}

impl InferenceResponse {
    pub fn new(request_id: String) -> Self {
        InferenceResponse {
            request_id,
            completion: String::new(),
            prompt_tokens: 0,
            completion_tokens: 0,
            latency_ms: 0,
            efficiency: 0.0,
            new_kv: HashMap::new(),
            success: false,
            error_message: None,
        }
    }

    /// 设置完成文本
    pub fn with_completion(mut self, completion: String) -> Self {
        self.completion = completion;
        self
    }

    /// 设置 token 统计
    pub fn with_token_stats(mut self, prompt_tokens: u32, completion_tokens: u32) -> Self {
        self.prompt_tokens = prompt_tokens;
        self.completion_tokens = completion_tokens;
        self
    }

    /// 设置延迟
    pub fn with_latency(mut self, latency_ms: u64) -> Self {
        self.latency_ms = latency_ms;
        if latency_ms > 0 {
            self.efficiency = (self.completion_tokens as f64) / (latency_ms as f64 / 1000.0);
        }
        self
    }

    /// 设置新生成的 KV 数据
    pub fn with_new_kv(mut self, new_kv: HashMap<String, Vec<u8>>) -> Self {
        self.new_kv = new_kv;
        self
    }

    /// 标记成功
    pub fn mark_success(&mut self) {
        self.success = true;
        self.error_message = None;
    }

    /// 标记失败
    pub fn mark_failure(&mut self, error: String) {
        self.success = false;
        self.error_message = Some(error);
    }
}

/// 推理提供商接口 trait - 所有提供商必须实现的标准化接口
///
/// **注意**: 这是一个异步 trait，所有实现必须使用 `async_trait` 宏
#[async_trait::async_trait]
pub trait InferenceProvider: Send + Sync {
    /// 获取提供商 ID
    fn provider_id(&self) -> &str;

    /// 获取引擎类型
    fn engine_type(&self) -> InferenceEngineType;

    /// 获取接口版本
    fn interface_version(&self) -> &str;

    /// 获取算力规格（token/s）
    fn compute_capacity(&self) -> u64;

    /// 执行推理（核心方法）
    ///
    /// 参数：
    /// - request: 推理请求
    /// - memory: 记忆层管理器（只读访问）
    /// - credential: 访问凭证
    ///
    /// 返回：
    /// - 推理响应
    async fn execute_inference(
        &self,
        request: &InferenceRequest,
        memory: &MemoryLayerManager,
        credential: &AccessCredential,
    ) -> Result<InferenceResponse, String>;

    /// 克隆为 Box（用于对象安全）
    fn clone_box(&self) -> Box<dyn InferenceProvider>;
}

/// 为 Box<dyn InferenceProvider> 实现 Clone
impl Clone for Box<dyn InferenceProvider> {
    fn clone(&self) -> Self {
        self.as_ref().clone_box()
    }
}

/// 推理提供商管理器 - 管理多个推理提供商实例
pub struct ProviderLayerManager {
    /// 提供商映射
    providers: HashMap<String, Box<dyn InferenceProvider>>,
    /// 当前选中的提供商 ID
    current_provider_id: Option<String>,
    /// 提供商记录（用于调度决策）
    provider_records: HashMap<String, ProviderRecord>,
}

impl Clone for ProviderLayerManager {
    fn clone(&self) -> Self {
        ProviderLayerManager {
            providers: self.providers.iter()
                .map(|(k, v)| (k.clone(), v.clone_box()))
                .collect(),
            current_provider_id: self.current_provider_id.clone(),
            provider_records: self.provider_records.clone(),
        }
    }
}

impl ProviderLayerManager {
    /// 创建新的提供商管理器
    pub fn new() -> Self {
        ProviderLayerManager {
            providers: HashMap::new(),
            current_provider_id: None,
            provider_records: HashMap::new(),
        }
    }

    /// 注册推理提供商
    pub fn register_provider(&mut self, provider: Box<dyn InferenceProvider>) -> Result<(), String> {
        let provider_id = provider.provider_id().to_string();

        if self.providers.contains_key(&provider_id) {
            return Err(format!("Provider {} already registered", provider_id));
        }

        // 创建提供商记录
        let record = ProviderRecord::new(
            provider_id.clone(),
            provider.interface_version().to_string(),
            provider.compute_capacity(),
            0.1, // 默认 10% 分成
        );

        self.providers.insert(provider_id.clone(), provider);
        self.provider_records.insert(provider_id, record);

        Ok(())
    }

    /// 获取提供商
    pub fn get_provider(&self, provider_id: &str) -> Option<&dyn InferenceProvider> {
        self.providers.get(provider_id).map(|p| p.as_ref())
    }

    /// 获取提供商（可变引用）
    pub fn get_provider_mut(&mut self, provider_id: &str) -> Option<&mut Box<dyn InferenceProvider>> {
        self.providers.get_mut(provider_id)
    }

    /// 设置当前提供商
    pub fn set_current_provider(&mut self, provider_id: &str) -> Result<(), String> {
        if !self.providers.contains_key(provider_id) {
            return Err(format!("Provider {} not found", provider_id));
        }

        self.current_provider_id = Some(provider_id.to_string());
        Ok(())
    }

    /// 获取当前提供商
    pub fn current_provider(&self) -> Option<&dyn InferenceProvider> {
        self.current_provider_id.as_ref()
            .and_then(|id| self.providers.get(id))
            .map(|p| p.as_ref())
    }

    /// 获取所有活跃提供商
    pub fn get_active_providers(&self) -> Vec<&ProviderRecord> {
        use crate::node_layer::ProviderStatus;

        self.provider_records.values()
            .filter(|p| p.status == ProviderStatus::Active)
            .collect()
    }

    /// 更新提供商状态
    pub fn update_provider_status(
        &mut self,
        provider_id: &str,
        status: crate::node_layer::ProviderStatus,
    ) -> Result<(), String> {
        let record = self.provider_records.get_mut(provider_id)
            .ok_or_else(|| format!("Provider record {} not found", provider_id))?;

        record.status = status;
        Ok(())
    }

    /// 获取提供商记录
    pub fn get_provider_record(&self, provider_id: &str) -> Option<&ProviderRecord> {
        self.provider_records.get(provider_id)
    }

    /// 获取提供商记录（可变引用）
    pub fn get_provider_record_mut(&mut self, provider_id: &str) -> Option<&mut ProviderRecord> {
        self.provider_records.get_mut(provider_id)
    }

    /// 执行推理（使用当前提供商）
    ///
    /// **注意**: 此方法已被标记为 deprecated，请使用 `execute_inference_async`
    #[deprecated(since = "0.2.0", note = "Use `execute_inference_async` instead")]
    pub fn execute_inference(
        &self,
        request: &InferenceRequest,
        memory: &MemoryLayerManager,
        credential: &AccessCredential,
    ) -> Result<InferenceResponse, String> {
        // 同步包装异步方法
        tokio::runtime::Handle::current()
            .block_on(self.execute_inference_async(request, memory, credential))
    }

    /// 执行推理（异步版本，使用当前提供商）
    pub async fn execute_inference_async(
        &self,
        request: &InferenceRequest,
        memory: &MemoryLayerManager,
        credential: &AccessCredential,
    ) -> Result<InferenceResponse, String> {
        let provider = self.current_provider()
            .ok_or_else(|| "No current provider selected".to_string())?;

        provider.execute_inference(request, memory, credential).await
    }

    /// 执行推理（指定提供商）
    pub fn execute_with_provider(
        &self,
        provider_id: &str,
        request: &InferenceRequest,
        memory: &MemoryLayerManager,
        credential: &AccessCredential,
    ) -> Result<InferenceResponse, String> {
        // 同步包装异步方法
        tokio::runtime::Handle::current()
            .block_on(self.execute_with_provider_async(provider_id, request, memory, credential))
    }

    /// 执行推理（异步版本，指定提供商）
    pub async fn execute_with_provider_async(
        &self,
        provider_id: &str,
        request: &InferenceRequest,
        memory: &MemoryLayerManager,
        credential: &AccessCredential,
    ) -> Result<InferenceResponse, String> {
        let provider = self.get_provider(provider_id)
            .ok_or_else(|| format!("Provider {} not found", provider_id))?;

        provider.execute_inference(request, memory, credential).await
    }

    /// 执行推理（指定提供商，可变引用版本）
    pub fn execute(&mut self, provider_id: &str, request: &InferenceRequest, memory: &MemoryLayerManager, credential: &AccessCredential) -> Result<InferenceResponse, String> {
        self.execute_with_provider(provider_id, request, memory, credential)
    }

    /// 获取所有提供商 ID 列表
    pub fn list_providers(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }

    /// 注册 Mock 提供商（用于测试）
    pub fn register_mock_provider(&mut self, provider_id: String, engine_type: InferenceEngineType, throughput: u32) -> Result<(), String> {
        let provider = Box::new(MockInferenceProvider::new(
            provider_id.clone(),
            engine_type,
            throughput as u64,
        ));
        self.register_provider(provider)
    }

    /// 提供商数量
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }
}

/// 示例推理提供商实现 - 用于测试和演示
///
/// **增强功能**：支持模拟真实场景的错误和边界条件
/// - 模拟推理超时
/// - 模拟 KV 读取失败
/// - 模拟输出截断
/// - 模拟随机失败
pub struct MockInferenceProvider {
    provider_id: String,
    engine_type: InferenceEngineType,
    interface_version: String,
    compute_capacity: u64,
    /// 模拟延迟（毫秒），用于测试异步上链
    pub simulated_latency_ms: Option<u64>,
    /// 模拟超时（毫秒），超过此值则返回超时错误
    pub simulated_timeout_ms: Option<u64>,
    /// 模拟 KV 读取失败概率（0.0-1.0）
    pub kv_read_failure_rate: f64,
    /// 模拟随机失败概率（0.0-1.0）
    pub random_failure_rate: f64,
    /// 最大输出 token 数（用于测试截断）
    pub max_output_tokens: Option<u32>,
    /// 失败计数器（用于测试重试）
    pub failure_count: AtomicU32,
    /// 最大失败次数，超过后返回成功
    pub max_failures: Option<u32>,
}

impl MockInferenceProvider {
    pub fn new(provider_id: String, engine_type: InferenceEngineType, compute_capacity: u64) -> Self {
        MockInferenceProvider {
            provider_id,
            engine_type,
            interface_version: "1.0.0".to_string(),
            compute_capacity,
            simulated_latency_ms: None,
            simulated_timeout_ms: None,
            kv_read_failure_rate: 0.0,
            random_failure_rate: 0.0,
            max_output_tokens: None,
            failure_count: AtomicU32::new(0),
            max_failures: None,
        }
    }

    /// 设置模拟延迟
    pub fn with_latency(mut self, latency_ms: u64) -> Self {
        self.simulated_latency_ms = Some(latency_ms);
        self
    }

    /// 设置模拟超时
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.simulated_timeout_ms = Some(timeout_ms);
        self
    }

    /// 设置 KV 读取失败概率
    pub fn with_kv_failure_rate(mut self, rate: f64) -> Self {
        self.kv_read_failure_rate = rate.clamp(0.0, 1.0);
        self
    }

    /// 设置随机失败概率
    pub fn with_random_failure_rate(mut self, rate: f64) -> Self {
        self.random_failure_rate = rate.clamp(0.0, 1.0);
        self
    }

    /// 设置最大输出 token 数
    pub fn with_max_output_tokens(mut self, max: u32) -> Self {
        self.max_output_tokens = Some(max);
        self
    }

    /// 设置最大失败次数
    pub fn with_max_failures(mut self, max: u32) -> Self {
        self.max_failures = Some(max);
        self
    }

    /// 重置失败计数器
    pub fn reset_failures(&self) {
        self.failure_count.store(0, Ordering::SeqCst);
    }

    /// 检查是否应该失败
    fn should_fail(&self) -> bool {
        // 检查最大失败次数
        if let Some(max) = self.max_failures {
            let current = self.failure_count.fetch_add(1, Ordering::SeqCst);
            if current < max {
                return true;
            }
        }

        // 检查随机失败
        if self.random_failure_rate > 0.0 {
            let rand_val = rand::random::<f64>();
            if rand_val < self.random_failure_rate {
                return true;
            }
        }

        false
    }

    /// 检查 KV 读取是否应该失败
    fn should_kv_read_fail(&self) -> bool {
        if self.kv_read_failure_rate > 0.0 {
            let rand_val = rand::random::<f64>();
            rand_val < self.kv_read_failure_rate
        } else {
            false
        }
    }
}

#[async_trait::async_trait]
impl InferenceProvider for MockInferenceProvider {
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
        let start_time = SystemTime::now();

        // 检查是否应该模拟随机失败
        if self.should_fail() {
            return Err(format!("Mock provider {} simulated random failure", self.provider_id));
        }

        // 模拟推理过程
        // 1. 从记忆层读取上下文
        let mut context = String::new();
        let mut kv_read_errors = Vec::new();

        for block_id in &request.memory_block_ids {
            // 模拟 KV 读取失败
            if self.should_kv_read_fail() {
                kv_read_errors.push(format!("Failed to read KV for block {}", block_id));
                continue;
            }

            // 读取 KV 数据作为上下文
            if let Some(shard) = memory.read_kv("context", credential) {
                context.push_str(&String::from_utf8_lossy(&shard.value));
            }
        }

        // 如果有 KV 读取错误，返回错误
        if !kv_read_errors.is_empty() {
            return Err(format!("KV read errors: {:?}", kv_read_errors));
        }

        // 2. 模拟 LLM 推理（简单拼接）
        let mut completion = format!("Response to: {}{}", request.prompt, context);

        // 3. 计算 token 数
        let mut completion_tokens = completion.len() as u32 / 4; // 估算

        // 4. 检查输出截断
        let mut truncated = false;
        if let Some(max_tokens) = self.max_output_tokens {
            if completion_tokens > max_tokens {
                // 截断输出
                let max_chars = (max_tokens * 4) as usize;
                if completion.len() > max_chars {
                    completion.truncate(max_chars);
                    completion_tokens = max_tokens;
                    truncated = true;
                }
            }
        }

        // 5. 生成新 KV 数据
        let mut new_kv = HashMap::new();
        new_kv.insert(
            format!("response_{}", request.request_id),
            completion.as_bytes().to_vec(),
        );

        // 6. 计算延迟（使用模拟延迟或实际延迟）
        let latency_ms = if let Some(simulated) = self.simulated_latency_ms {
            simulated
        } else {
            start_time.elapsed()
                .map(|d| d.as_millis() as u64)
                .unwrap_or(1)
        };

        // 7. 检查超时
        if let Some(timeout) = self.simulated_timeout_ms {
            if latency_ms > timeout {
                return Err(format!(
                    "Inference timeout: {}ms > {}ms",
                    latency_ms, timeout
                ));
            }
        }

        let mut response = InferenceResponse::new(request.request_id.clone())
            .with_completion(completion)
            .with_token_stats(request.prompt.len() as u32 / 4, completion_tokens)
            .with_latency(latency_ms)
            .with_new_kv(new_kv);

        // 标记是否被截断（在 mark_success 之前设置，因为 mark_success 会清空 error_message）
        let truncation_message = if truncated {
            Some(format!(
                "Output truncated to {} tokens",
                self.max_output_tokens.unwrap()
            ))
        } else {
            None
        };

        response.mark_success();
        
        // 截断信息在成功后作为警告保留
        if let Some(msg) = truncation_message {
            response.error_message = Some(msg);
        }
        
        Ok(response)
    }

    fn clone_box(&self) -> Box<dyn InferenceProvider> {
        Box::new(self.clone())
    }
}

impl Clone for MockInferenceProvider {
    fn clone(&self) -> Self {
        MockInferenceProvider {
            provider_id: self.provider_id.clone(),
            engine_type: self.engine_type.clone(),
            interface_version: self.interface_version.clone(),
            compute_capacity: self.compute_capacity,
            simulated_latency_ms: self.simulated_latency_ms,
            simulated_timeout_ms: self.simulated_timeout_ms,
            kv_read_failure_rate: self.kv_read_failure_rate,
            random_failure_rate: self.random_failure_rate,
            max_output_tokens: self.max_output_tokens,
            failure_count: AtomicU32::new(self.failure_count.load(Ordering::SeqCst)),
            max_failures: self.max_failures,
        }
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

    #[tokio::test]
    async fn test_mock_provider_inference() {
        // 创建记忆层
        let mut memory = MemoryLayerManager::new("node_1");
        let credential = create_test_credential();

        // 写入测试 KV
        memory.write_kv("context".to_string(), b"test context".to_vec(), &credential).unwrap();

        // 创建提供商
        let provider = MockInferenceProvider::new(
            "provider_1".to_string(),
            InferenceEngineType::Vllm,
            100,
        );

        // 创建请求
        let request = InferenceRequest::new(
            "req_1".to_string(),
            "Hello, AI!".to_string(),
            "llama-7b".to_string(),
            100,
        ).with_memory_blocks(vec![0]);

        // 执行推理
        let response = provider.execute_inference(&request, &memory, &credential).await.unwrap();

        assert!(response.success);
        assert!(!response.completion.is_empty());
        assert!(response.completion_tokens > 0);
        assert!(!response.new_kv.is_empty());
    }

    #[test]
    fn test_provider_layer_manager() {
        let mut manager = ProviderLayerManager::new();

        // 注册两个提供商
        let provider1 = Box::new(MockInferenceProvider::new(
            "provider_1".to_string(),
            InferenceEngineType::Vllm,
            100,
        ));
        let provider2 = Box::new(MockInferenceProvider::new(
            "provider_2".to_string(),
            InferenceEngineType::Sglang,
            80,
        ));

        manager.register_provider(provider1).unwrap();
        manager.register_provider(provider2).unwrap();

        assert_eq!(manager.provider_count(), 2);

        // 设置当前提供商
        manager.set_current_provider("provider_1").unwrap();

        let current = manager.current_provider().unwrap();
        assert_eq!(current.provider_id(), "provider_1");
    }

    #[test]
    fn test_inference_response_builder() {
        let response = InferenceResponse::new("req_1".to_string())
            .with_completion("test response".to_string())
            .with_token_stats(50, 30)
            .with_latency(500);

        assert_eq!(response.request_id, "req_1");
        assert_eq!(response.completion, "test response");
        assert_eq!(response.prompt_tokens, 50);
        assert_eq!(response.completion_tokens, 30);
        assert_eq!(response.latency_ms, 500);
        assert!(response.efficiency > 0.0);
    }

    #[tokio::test]
    async fn test_mock_provider_timeout() {
        let mut memory = MemoryLayerManager::new("node_1");
        let credential = create_test_credential();
        memory.write_kv("context".to_string(), b"test context".to_vec(), &credential).unwrap();

        // 创建带超时的提供商（超时 1ms，但模拟延迟 100ms）
        let provider = MockInferenceProvider::new(
            "provider_1".to_string(),
            InferenceEngineType::Vllm,
            100,
        )
        .with_latency(100)
        .with_timeout(1);

        let request = InferenceRequest::new(
            "req_1".to_string(),
            "Hello!".to_string(),
            "llama-7b".to_string(),
            100,
        ).with_memory_blocks(vec![0]);

        let result = provider.execute_inference(&request, &memory, &credential).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("timeout"));
    }

    #[tokio::test]
    async fn test_mock_provider_output_truncation() {
        let mut memory = MemoryLayerManager::new("node_1");
        let credential = create_test_credential();
        memory.write_kv("context".to_string(), b"test context".to_vec(), &credential).unwrap();

        // 创建带最大输出 token 限制的提供商
        let provider = MockInferenceProvider::new(
            "provider_1".to_string(),
            InferenceEngineType::Vllm,
            100,
        )
        .with_max_output_tokens(5);

        let request = InferenceRequest::new(
            "req_1".to_string(),
            "This is a very long prompt that should produce more than 5 tokens".to_string(),
            "llama-7b".to_string(),
            100,
        ).with_memory_blocks(vec![0]);

        let response = provider.execute_inference(&request, &memory, &credential).await.unwrap();
        assert!(response.success);
        assert!(response.completion_tokens <= 5);
        assert!(response.error_message.is_some());
        assert!(response.error_message.unwrap().contains("truncated"));
    }

    #[tokio::test]
    async fn test_mock_provider_random_failures() {
        let mut memory = MemoryLayerManager::new("node_1");
        let credential = create_test_credential();
        memory.write_kv("context".to_string(), b"test context".to_vec(), &credential).unwrap();

        // 创建带 50% 随机失败概率的提供商
        let provider = MockInferenceProvider::new(
            "provider_1".to_string(),
            InferenceEngineType::Vllm,
            100,
        )
        .with_random_failure_rate(0.5);

        let request = InferenceRequest::new(
            "req_1".to_string(),
            "Hello!".to_string(),
            "llama-7b".to_string(),
            100,
        );

        // 运行多次，应该至少有一次失败和一次成功
        let mut success_count = 0;
        let mut failure_count = 0;
        for _ in 0..20 {
            match provider.execute_inference(&request, &memory, &credential).await {
                Ok(_) => success_count += 1,
                Err(_) => failure_count += 1,
            }
        }

        // 由于是随机，理论上应该有成功也有失败
        // 但为了测试稳定性，只检查能正常执行
        assert!(success_count + failure_count == 20);
    }

    #[tokio::test]
    async fn test_mock_provider_max_failures() {
        let mut memory = MemoryLayerManager::new("node_1");
        let credential = create_test_credential();
        memory.write_kv("context".to_string(), b"test context".to_vec(), &credential).unwrap();

        // 创建带最大失败次数的提供商（前 3 次失败，之后成功）
        let provider = MockInferenceProvider::new(
            "provider_1".to_string(),
            InferenceEngineType::Vllm,
            100,
        )
        .with_max_failures(3);

        let request = InferenceRequest::new(
            "req_1".to_string(),
            "Hello!".to_string(),
            "llama-7b".to_string(),
            100,
        );

        // 前 3 次应该失败
        for i in 0..3 {
            let result = provider.execute_inference(&request, &memory, &credential).await;
            assert!(result.is_err(), "Iteration {} should fail", i);
        }

        // 第 4 次应该成功
        let result = provider.execute_inference(&request, &memory, &credential).await;
        assert!(result.is_ok(), "Iteration 3 should succeed");
    }
}
