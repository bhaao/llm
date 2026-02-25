use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use ed25519_dalek::{SigningKey, Signer};
use crate::traits::{Hashable, Serializable, Verifiable, SignatureVerifier, Ed25519Verifier};

/// 交易类型枚举
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionType {
    /// 普通转账
    Transfer,
    /// 合约调用
    ContractCall,
    /// 合约部署
    ContractDeploy,
    /// 代币发行
    TokenMint,
    /// 代币销毁
    TokenBurn,
    /// 跨链交易
    CrossChain,
    /// 治理投票
    GovernanceVote,
    /// 质押交易
    Stake,
    /// 解质押交易
    Unstake,
    /// 推理请求（分布式 LLM 专用）
    InferenceRequest,
    /// 推理响应（分布式 LLM 专用）
    InferenceResponse,
    /// 内部系统交易（无需签名）
    Internal,
}

impl TransactionType {
    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            TransactionType::Transfer => "transfer",
            TransactionType::ContractCall => "contract_call",
            TransactionType::ContractDeploy => "contract_deploy",
            TransactionType::TokenMint => "token_mint",
            TransactionType::TokenBurn => "token_burn",
            TransactionType::CrossChain => "cross_chain",
            TransactionType::GovernanceVote => "governance_vote",
            TransactionType::Stake => "stake",
            TransactionType::Unstake => "unstake",
            TransactionType::InferenceRequest => "inference_request",
            TransactionType::InferenceResponse => "inference_response",
            TransactionType::Internal => "internal",
        }
    }

    /// 从字符串解析
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "transfer" => Some(TransactionType::Transfer),
            "contract_call" => Some(TransactionType::ContractCall),
            "contract_deploy" => Some(TransactionType::ContractDeploy),
            "token_mint" => Some(TransactionType::TokenMint),
            "token_burn" => Some(TransactionType::TokenBurn),
            "cross_chain" => Some(TransactionType::CrossChain),
            "governance_vote" => Some(TransactionType::GovernanceVote),
            "stake" => Some(TransactionType::Stake),
            "unstake" => Some(TransactionType::Unstake),
            "inference_request" => Some(TransactionType::InferenceRequest),
            "inference_response" => Some(TransactionType::InferenceResponse),
            "internal" => Some(TransactionType::Internal),
            _ => None,
        }
    }
    
    /// 是否需要签名验证
    pub fn requires_signature(&self) -> bool {
        !matches!(self, TransactionType::Internal)
    }
}

/// 交易负载，根据交易类型不同而不同
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionPayload {
    /// 转账负载
    Transfer {
        amount: u128,
        token_id: Option<String>,
        memo: Option<String>,
    },
    /// 合约调用负载
    ContractCall {
        contract_address: String,
        method: String,
        args: Vec<String>,
    },
    /// 合约部署负载
    ContractDeploy {
        bytecode: String,
        abi: String,
    },
    /// 代币发行负载
    TokenMint {
        token_name: String,
        token_symbol: String,
        supply: u128,
    },
    /// 跨链交易负载
    CrossChain {
        target_chain: String,
        target_address: String,
        amount: u128,
    },
    /// 治理投票负载
    GovernanceVote {
        proposal_id: String,
        vote: bool,
    },
    /// 质押交易负载
    Stake {
        validator: String,
        amount: u128,
    },
    /// 推理请求负载
    InferenceRequest {
        prompt: String,
        model_id: String,
        max_tokens: u32,
    },
    /// 推理响应负载
    InferenceResponse {
        response_id: String,
        completion: String,
        prompt_tokens: u32,
        completion_tokens: u32,
    },
    /// 空负载（用于内部交易）
    None,
}

impl Default for TransactionPayload {
    fn default() -> Self {
        TransactionPayload::None
    }
}

/// 交易结构体
#[derive(Serialize, Deserialize)]
pub struct Transaction {
    /// 交易 ID
    pub id: String,
    /// 交易时间
    pub timestamp: u64,
    /// 发送方地址
    pub sender: String,
    /// 接收方地址
    pub receiver: String,
    /// 交易类型
    pub tx_type: TransactionType,
    /// 交易负载
    pub payload: TransactionPayload,
    /// 数字签名（hex 编码）
    pub signature: String,
    /// 消耗的 Gas
    pub gas_used: u64,
    /// 交易状态
    pub status: TransactionStatus,
    /// 签名验证器（transient，不序列化）
    #[serde(skip)]
    verifier: Option<Box<dyn SignatureVerifier>>,
    /// 公钥（hex 编码，用于 Ed25519 验证）
    pub public_key: String,
}

impl std::fmt::Debug for Transaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transaction")
            .field("id", &self.id)
            .field("timestamp", &self.timestamp)
            .field("sender", &self.sender)
            .field("receiver", &self.receiver)
            .field("tx_type", &self.tx_type)
            .field("signature", &self.signature)
            .field("gas_used", &self.gas_used)
            .field("status", &self.status)
            .field("public_key", &self.public_key)
            .finish()
    }
}

impl Clone for Transaction {
    fn clone(&self) -> Self {
        // 注意：verifier 是 transient 状态，克隆后需要重新设置
        // 如需保留验证器，请使用 clone_with_verifier() 方法
        Transaction {
            id: self.id.clone(),
            timestamp: self.timestamp,
            sender: self.sender.clone(),
            receiver: self.receiver.clone(),
            tx_type: self.tx_type.clone(),
            payload: self.payload.clone(),
            signature: self.signature.clone(),
            gas_used: self.gas_used,
            status: self.status.clone(),
            verifier: None,
            public_key: self.public_key.clone(),
        }
    }
}

/// 扩展方法：支持保留验证器的克隆
impl Transaction {
    /// 克隆并保留验证器（如需要）
    ///
    /// **注意**：标准 `clone()` 方法不会复制签名验证器。
    /// 如果需要在克隆后继续验证签名，请使用此方法。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// // 不推荐：克隆后无法验证签名
    /// let tx_clone = tx.clone();
    ///
    /// // 推荐：保留验证器
    /// let tx_clone = tx.clone_with_verifier();
    /// ```
    pub fn clone_with_verifier(&self) -> Self {
        Transaction {
            id: self.id.clone(),
            timestamp: self.timestamp,
            sender: self.sender.clone(),
            receiver: self.receiver.clone(),
            tx_type: self.tx_type.clone(),
            payload: self.payload.clone(),
            signature: self.signature.clone(),
            gas_used: self.gas_used,
            status: self.status.clone(),
            verifier: self.verifier.as_ref().map(|v| v.clone_box()),
            public_key: self.public_key.clone(),
        }
    }
}

/// 交易状态
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionStatus {
    /// 待处理
    Pending,
    /// 已确认
    Confirmed,
    /// 已失败
    Failed,
    /// 已回滚
    Reverted,
}

impl Default for TransactionStatus {
    fn default() -> Self {
        TransactionStatus::Pending
    }
}

impl Transaction {
    /// 创建新交易
    pub fn new(
        sender: String,
        receiver: String,
        tx_type: TransactionType,
        payload: TransactionPayload,
    ) -> Self {
        let timestamp = Self::current_timestamp();
        let id = Self::generate_id(&sender, &receiver, timestamp, &tx_type);

        Transaction {
            id,
            timestamp,
            sender,
            receiver,
            tx_type,
            payload,
            signature: String::new(),
            gas_used: 0,
            status: TransactionStatus::Pending,
            verifier: None,
            public_key: String::new(),
        }
    }

    /// 创建内部交易（无需签名）
    pub fn new_internal(
        sender: String,
        receiver: String,
        tx_type: TransactionType,
        payload: TransactionPayload,
    ) -> Self {
        let mut tx = Self::new(sender, receiver, tx_type, payload);
        tx.signature = "internal".to_string();
        tx
    }

    /// 设置签名验证器
    pub fn with_verifier(mut self, verifier: Box<dyn SignatureVerifier>) -> Self {
        self.verifier = Some(verifier);
        self
    }

    /// 设置签名验证器（可变引用版本）
    pub fn set_verifier(&mut self, verifier: Box<dyn SignatureVerifier>) {
        self.verifier = Some(verifier);
    }

    /// 使用 Ed25519 私钥对交易进行签名
    ///
    /// 参数：
    /// - private_key: Ed25519 私钥（32 字节，hex 编码或原始字节）
    ///
    /// 返回：
    /// - 公钥（hex 编码），用于后续验证
    pub fn sign_ed25519(&mut self, private_key: &[u8]) -> Result<String, String> {
        // 创建签名密钥
        let signing_key = SigningKey::from_bytes(
            &private_key.try_into()
                .map_err(|_| "Invalid private key length (expected 32 bytes)")?
        );

        // 获取对应的公钥
        let verifying_key = signing_key.verifying_key();
        let public_key_hex = hex::encode(verifying_key.to_bytes());

        // 获取签名消息并签名
        let message = self.signing_message();
        let signature = signing_key.sign(message.as_bytes());

        // 存储签名和公钥
        self.signature = hex::encode(signature.to_bytes());
        self.public_key = public_key_hex.clone();

        Ok(public_key_hex)
    }

    /// 对交易进行签名（旧方法，保留用于向后兼容）
    ///
    /// **注意**：此方法使用简单的 SHA256 模拟签名，仅用于测试
    /// 生产环境请使用 `sign_ed25519()` 方法
    #[deprecated(since = "0.1.1", note = "请使用 sign_ed25519() 方法")]
    pub fn sign(&mut self, private_key: &str) {
        let message = self.signing_message();
        let signature = format!("{:x}", Sha256::digest(format!("{}:{}", message, private_key).as_bytes()));
        self.signature = signature;
    }

    /// 获取签名消息
    fn signing_message(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}",
            self.id,
            self.timestamp,
            self.sender,
            self.receiver,
            self.tx_type.as_str()
        )
    }

    /// 确认交易
    pub fn confirm(&mut self) {
        self.status = TransactionStatus::Confirmed;
    }

    /// 标记交易失败
    pub fn fail(&mut self) {
        self.status = TransactionStatus::Failed;
    }

    /// 获取当前时间戳
    fn current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    /// 生成交易 ID
    fn generate_id(sender: &str, receiver: &str, timestamp: u64, tx_type: &TransactionType) -> String {
        let data = format!("{}:{}:{}:{}", sender, receiver, timestamp, tx_type.as_str());
        format!("tx_{}", format!("{:x}", Sha256::digest(data.as_bytes())))
    }
}

impl Hashable for Transaction {
    fn hash(&self) -> String {
        let data = format!(
            "{}:{}:{}:{}:{}:{}:{}",
            self.id,
            self.timestamp,
            self.sender,
            self.receiver,
            self.tx_type.as_str(),
            self.gas_used,
            self.signature
        );
        format!("{:x}", Sha256::digest(data.as_bytes()))
    }
}

impl Verifiable for Transaction {
    fn verify(&self) -> bool {
        self.verify_with_error().is_ok()
    }

    fn verify_with_error(&self) -> Result<(), String> {
        if self.sender.is_empty() {
            return Err("Sender address is empty".to_string());
        }
        if self.receiver.is_empty() {
            return Err("Receiver address is empty".to_string());
        }

        if self.tx_type == TransactionType::Internal {
            return Ok(());
        }

        if self.signature.is_empty() {
            return Err("Transaction is not signed".to_string());
        }

        if self.public_key.is_empty() {
            // 向后兼容：如果没有公钥，使用旧验证方式
            if let Some(verifier) = &self.verifier {
                let message = self.signing_message();
                if !verifier.verify_signature(&message, &self.signature, &self.sender) {
                    return Err("Signature verification failed".to_string());
                }
            }
            return Ok(());
        }

        // 使用 Ed25519 验证签名
        let verifier = Ed25519Verifier::new();
        let message = self.signing_message();
        if !verifier.verify_signature(&message, &self.signature, &self.public_key) {
            return Err("Ed25519 signature verification failed".to_string());
        }

        Ok(())
    }
}

impl Serializable for Transaction {
    fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| e.to_string())
    }

    fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_creation() {
        let tx = Transaction::new(
            "sender_123".to_string(),
            "receiver_456".to_string(),
            TransactionType::Transfer,
            TransactionPayload::Transfer {
                amount: 1000,
                token_id: None,
                memo: Some("test".to_string()),
            },
        );

        assert!(tx.id.starts_with("tx_"));
        assert_eq!(tx.sender, "sender_123");
        assert_eq!(tx.receiver, "receiver_456");
        assert_eq!(tx.status, TransactionStatus::Pending);
    }

    #[test]
    fn test_internal_transaction_no_signature() {
        let tx = Transaction::new_internal(
            "system".to_string(),
            "node_1".to_string(),
            TransactionType::Internal,
            TransactionPayload::None,
        );

        assert!(tx.verify());
    }

    #[test]
    fn test_transaction_ed25519_signature() {
        let mut tx = Transaction::new(
            "sender".to_string(),
            "receiver".to_string(),
            TransactionType::Transfer,
            TransactionPayload::None,
        );

        // 生成随机私钥（32 字节）
        let mut key_bytes = [0u8; 32];
        for byte in key_bytes.iter_mut() {
            *byte = rand::random();
        }
        let _signing_key = SigningKey::from_bytes(&key_bytes);

        // 使用 Ed25519 签名
        let public_key = tx.sign_ed25519(&key_bytes).unwrap();
        assert!(!public_key.is_empty());
        assert!(!tx.signature.is_empty());

        // 验证签名
        assert!(tx.verify());
    }

    #[test]
    fn test_transaction_invalid_signature() {
        let mut tx = Transaction::new(
            "sender".to_string(),
            "receiver".to_string(),
            TransactionType::Transfer,
            TransactionPayload::None,
        );

        // 使用错误的私钥签名
        let wrong_key = [0u8; 32];
        tx.sign_ed25519(&wrong_key).unwrap();

        // 篡改消息后验证应该失败
        let mut tx_tampered = tx.clone();
        tx_tampered.sender = "attacker".to_string();
        assert!(!tx_tampered.verify());
    }

    #[test]
    fn test_transaction_hash() {
        let tx = Transaction::new(
            "sender".to_string(),
            "receiver".to_string(),
            TransactionType::Transfer,
            TransactionPayload::None,
        );

        let hash = tx.hash();
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_transaction_serialization() {
        let tx = Transaction::new(
            "sender".to_string(),
            "receiver".to_string(),
            TransactionType::Transfer,
            TransactionPayload::None,
        );

        let json = tx.to_json().unwrap();
        let restored: Transaction = Transaction::from_json(&json).unwrap();

        assert_eq!(tx.id, restored.id);
        assert_eq!(tx.sender, restored.sender);
    }

    #[test]
    fn test_inference_request_payload() {
        let tx = Transaction::new(
            "user_1".to_string(),
            "node_1".to_string(),
            TransactionType::InferenceRequest,
            TransactionPayload::InferenceRequest {
                prompt: "Hello, AI!".to_string(),
                model_id: "llama-7b".to_string(),
                max_tokens: 100,
            },
        );

        assert_eq!(tx.tx_type, TransactionType::InferenceRequest);

        if let TransactionPayload::InferenceRequest { prompt, .. } = &tx.payload {
            assert_eq!(prompt, "Hello, AI!");
        } else {
            panic!("Wrong payload type");
        }
    }
}
