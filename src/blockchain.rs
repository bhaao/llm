use std::sync::{Arc, RwLock};
use crate::block::{Block, KvCacheProof};
use crate::metadata::BlockMetadata;
use crate::transaction::Transaction;
use crate::traits::{Verifiable, AttestationMetadata, AttestationType};
use crate::quality_assessment::{QualityAssessor, QualityAssessment, MultiNodeComparator};
use crate::reputation::{ReputationManager};

/// 共识投票结果
#[derive(Debug, Clone, PartialEq)]
pub enum ConsensusDecision {
    /// 一致同意（所有节点结果相同）
    Unanimous { winner_id: String },
    /// 多数同意（超过阈值比例的节点结果相同）
    Majority { winner_id: String, agreement_ratio: f64 },
    /// 无法达成共识（需要仲裁）
    NoConsensus { requires_arbitration: bool },
}

/// 共识引擎 - 实现简单的多数投票共识
pub struct ConsensusEngine {
    /// 共识阈值（默认 0.67，即 2/3 多数）
    threshold: f64,
    /// 最小节点数（少于该数量无法达成有效共识）
    min_nodes: usize,
}

impl ConsensusEngine {
    /// 创建新的共识引擎
    pub fn new(threshold: f64, min_nodes: usize) -> Self {
        ConsensusEngine {
            threshold: threshold.clamp(0.5, 1.0),
            min_nodes: if min_nodes < 2 { 2 } else { min_nodes },
        }
    }

    /// 创建默认共识引擎（阈值 2/3，最小 3 节点）
    pub fn default() -> Self {
        ConsensusEngine {
            threshold: 0.67,
            min_nodes: 3,
        }
    }

    /// 执行共识投票
    ///
    /// 参数：
    /// - node_results: 节点结果列表 (node_id, output_hash, quality_score)
    ///
    /// 返回：
    /// - 共识决策
    pub fn vote(&self, node_results: &[(String, String, f64)]) -> ConsensusDecision {
        if node_results.len() < self.min_nodes {
            return ConsensusDecision::NoConsensus { requires_arbitration: true };
        }

        // 按质量分数排序，选择最佳结果
        let mut sorted_results: Vec<_> = node_results.iter().collect();
        sorted_results.sort_by(|a, b| {
            b.2.partial_cmp(&a.2)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let best_result = sorted_results[0];

        // 计算与最佳结果哈希相同的节点数量
        let agreeing_nodes = node_results.iter()
            .filter(|(_, hash, _)| hash == &best_result.1)
            .count();

        let agreement_ratio = agreeing_nodes as f64 / node_results.len() as f64;

        if agreement_ratio >= self.threshold {
            if agreement_ratio >= 0.99 {
                ConsensusDecision::Unanimous {
                    winner_id: best_result.0.clone()
                }
            } else {
                ConsensusDecision::Majority {
                    winner_id: best_result.0.clone(),
                    agreement_ratio,
                }
            }
        } else {
            ConsensusDecision::NoConsensus { requires_arbitration: true }
        }
    }

    /// 获取共识阈值
    pub fn threshold(&self) -> f64 {
        self.threshold
    }

    /// 获取最小节点数
    pub fn min_nodes(&self) -> usize {
        self.min_nodes
    }
}

/// 日志配置
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// 日志级别 (error, warn, info, debug, trace)
    pub level: String,
    /// 是否启用文件日志
    pub enable_file_logging: bool,
    /// 日志文件路径（如果启用）
    pub log_file_path: Option<String>,
    /// 是否启用日志轮转
    pub enable_rotation: bool,
    /// 日志轮转周期（天）
    pub rotation_days: u32,
}

/// 超时配置
#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    /// 推理超时（毫秒）
    pub inference_timeout_ms: u64,
    /// 上链超时（毫秒）
    pub commit_timeout_ms: u64,
    /// 共识超时（毫秒）
    pub consensus_timeout_ms: u64,
    /// KV 读取超时（毫秒）
    pub kv_read_timeout_ms: u64,
}

/// 重试配置
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// 最大重试次数
    pub max_retries: u32,
    /// 初始重试延迟（毫秒）
    pub initial_delay_ms: u64,
    /// 最大重试延迟（毫秒）
    pub max_delay_ms: u64,
    /// 重试延迟倍增器
    pub delay_multiplier: f64,
}

/// 连接池配置
#[derive(Debug, Clone)]
pub struct ConnectionPoolConfig {
    /// 最小连接数
    pub min_connections: usize,
    /// 最大连接数
    pub max_connections: usize,
    /// 连接超时（毫秒）
    pub connection_timeout_ms: u64,
    /// 连接空闲超时（毫秒）
    pub idle_timeout_ms: u64,
}

/// 共识配置
#[derive(Debug, Clone)]
pub struct ConsensusConfig {
    /// 共识阈值（0.5-1.0，默认 0.67 即 2/3 多数）
    pub threshold: f64,
    /// 最小节点数
    pub min_nodes: usize,
    /// 是否启用仲裁
    pub enable_arbitration: bool,
}

/// 区块链配置结构体
///
/// 用于集中管理区块链的运行参数：
/// - 可信阈值：节点调度的最低信誉要求
/// - 超时配置：推理/上链/共识超时
/// - 重试配置：失败重试策略
/// - 日志配置：日志级别和输出
/// - 连接池配置：数据库连接池参数
/// - 共识配置：多数投票共识参数
#[derive(Debug, Clone)]
pub struct BlockchainConfig {
    /// 可信阈值（低于此值的节点不会被调度）
    pub trust_threshold: f64,
    /// 区块最大交易数（可选限制）
    pub max_transactions_per_block: Option<usize>,
    /// 区块最大 Gas 限制（可选）
    pub max_gas_per_block: Option<u64>,
    /// 超时配置
    pub timeout: TimeoutConfig,
    /// 重试配置
    pub retry: RetryConfig,
    /// 日志配置
    pub log: LogConfig,
    /// 连接池配置
    pub connection_pool: ConnectionPoolConfig,
    /// 共识配置
    pub consensus: ConsensusConfig,
}

impl Default for BlockchainConfig {
    fn default() -> Self {
        BlockchainConfig {
            trust_threshold: 0.7,
            max_transactions_per_block: None,
            max_gas_per_block: None,
            timeout: TimeoutConfig::default(),
            retry: RetryConfig::default(),
            log: LogConfig::default(),
            connection_pool: ConnectionPoolConfig::default(),
            consensus: ConsensusConfig::default(),
        }
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        LogConfig {
            level: "info".to_string(),
            enable_file_logging: false,
            log_file_path: None,
            enable_rotation: false,
            rotation_days: 7,
        }
    }
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        TimeoutConfig {
            inference_timeout_ms: 30000,      // 30 秒
            commit_timeout_ms: 10000,         // 10 秒
            consensus_timeout_ms: 5000,       // 5 秒
            kv_read_timeout_ms: 1000,         // 1 秒
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        RetryConfig {
            max_retries: 3,
            initial_delay_ms: 100,
            max_delay_ms: 5000,
            delay_multiplier: 2.0,
        }
    }
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        ConnectionPoolConfig {
            min_connections: 5,
            max_connections: 20,
            connection_timeout_ms: 5000,
            idle_timeout_ms: 60000,
        }
    }
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        ConsensusConfig {
            threshold: 0.67,
            min_nodes: 3,
            enable_arbitration: true,
        }
    }
}

impl BlockchainConfig {
    /// 创建新配置
    pub fn new(trust_threshold: f64) -> Self {
        BlockchainConfig {
            trust_threshold,
            max_transactions_per_block: None,
            max_gas_per_block: None,
            timeout: TimeoutConfig::default(),
            retry: RetryConfig::default(),
            log: LogConfig::default(),
            connection_pool: ConnectionPoolConfig::default(),
            consensus: ConsensusConfig::default(),
        }
    }

    /// 设置区块最大交易数
    pub fn with_max_transactions(mut self, max: usize) -> Self {
        self.max_transactions_per_block = Some(max);
        self
    }

    /// 设置区块最大 Gas
    pub fn with_max_gas(mut self, max: u64) -> Self {
        self.max_gas_per_block = Some(max);
        self
    }

    /// 设置推理超时
    pub fn with_inference_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout.inference_timeout_ms = timeout_ms;
        self
    }

    /// 设置上链超时
    pub fn with_commit_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout.commit_timeout_ms = timeout_ms;
        self
    }

    /// 设置最大重试次数
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.retry.max_retries = retries;
        self
    }

    /// 设置日志级别
    pub fn with_log_level(mut self, level: &str) -> Self {
        self.log.level = level.to_string();
        self
    }

    /// 启用文件日志
    pub fn with_file_logging(mut self, path: &str) -> Self {
        self.log.enable_file_logging = true;
        self.log.log_file_path = Some(path.to_string());
        self
    }

    /// 设置共识阈值
    pub fn with_consensus_threshold(mut self, threshold: f64) -> Self {
        self.consensus.threshold = threshold.clamp(0.5, 1.0);
        self
    }

    /// 设置最小共识节点数
    pub fn with_min_consensus_nodes(mut self, min_nodes: usize) -> Self {
        self.consensus.min_nodes = if min_nodes < 2 { 2 } else { min_nodes };
        self
    }
}

// 注意：NodeReputation 已移至 reputation.rs 模块
// 此处重新导出以便兼容
pub use crate::reputation::NodeReputation;

/// 区块链结构体（线程安全版本）
///
/// **定位**：全局可信存证主链，与记忆链（MemoryChain）共同构成双链架构
///
/// # 与记忆链的关系
///
/// | 维度 | 区块链（Blockchain） | 记忆链（MemoryChain） |
/// |------|---------------------|----------------------|
/// | 定位 | 全局可信存证链 | 分布式 KV 数据链 |
/// | 存储内容 | KV 哈希存证、元数据、信誉记录 | 实际 KV 数据、上下文分片 |
/// | 存储位置 | 全网共识，所有节点共享 | 每个节点本地维护 |
/// | 篡改防护 | 不可篡改，全网共识 | 哈希链式串联，哈希上链存证 |
/// | 性能 | 异步提交，不阻塞主流程 | 本地读写，低延迟 |
///
/// # 核心功能
///
/// - 每个区块包含推理请求/响应记录
/// - KV Cache 存证上链，防止数据篡改（与记忆链配合）
/// - 节点信誉上链，支持可信调度
/// - 质量评估结果上链，支持追溯审计
///
/// **架构说明**：
/// - 推理负责算得对：分布式推理模块负责高效计算
/// - 评估器负责验得准：质量评估器负责验证结果
/// - 多节点负责保安全：并行计算 + 结果比对
/// - 区块链负责记可信：不可篡改的存证记录
pub struct Blockchain {
    /// 区块列表（pub(crate) 用于 storage 模块访问）
    pub(crate) chain: Vec<Block>,
    /// 待处理交易池
    pub(crate) pending_transactions: Vec<Transaction>,
    /// 待提交 KV Cache 存证
    pub(crate) pending_kv_proofs: Vec<KvCacheProof>,
    /// 节点信誉管理器（链上信誉管理）
    pub(crate) reputation_manager: ReputationManager,
    /// 所有者地址
    pub(crate) owner_address: String,
    /// 区块链配置
    pub(crate) config: BlockchainConfig,
    /// 质量评估器（用于多节点结果验证）
    assessor: Option<Box<dyn QualityAssessor>>,
    /// 共识引擎（用于多节点共识决策）
    pub(crate) consensus_engine: ConsensusEngine,
    /// 测试模式：模拟提交失败（仅用于测试）
    #[cfg(test)]
    pub(crate) simulate_commit_failure: bool,
}

// 实现 Clone 以便在线程间传递
impl Clone for Blockchain {
    fn clone(&self) -> Self {
        Blockchain {
            chain: self.chain.clone(),
            pending_transactions: self.pending_transactions.clone(),
            pending_kv_proofs: self.pending_kv_proofs.clone(),
            reputation_manager: self.reputation_manager.clone(),
            owner_address: self.owner_address.clone(),
            config: self.config.clone(),
            assessor: self.assessor.as_ref().map(|a| a.clone_box()),
            consensus_engine: ConsensusEngine::new(self.consensus_engine.threshold, self.consensus_engine.min_nodes),
            #[cfg(test)]
            simulate_commit_failure: self.simulate_commit_failure,
        }
    }
}

/// 线程安全的区块链包装器
pub type SafeBlockchain = Arc<RwLock<Blockchain>>;

impl Blockchain {
    /// 创建新的区块链（使用默认配置）
    pub fn new(owner_address: String) -> Self {
        Self::with_config(owner_address, BlockchainConfig::default())
    }

    /// 创建新的区块链（使用自定义配置）
    pub fn with_config(owner_address: String, config: BlockchainConfig) -> Self {
        let reputation_manager = ReputationManager::new(config.trust_threshold, 0.6);

        let mut blockchain = Blockchain {
            chain: Vec::new(),
            pending_transactions: Vec::new(),
            pending_kv_proofs: Vec::new(),
            reputation_manager,
            owner_address,
            config,
            assessor: None,
            consensus_engine: ConsensusEngine::default(),
            #[cfg(test)]
            simulate_commit_failure: false,
        };

        // 创建创世区块
        let genesis_metadata = BlockMetadata::new(
            String::from("Genesis"),
            String::from("1.0.0"),
            0,
            0,
            0,
            0.0,
            String::from("System"),
        );
        let genesis_block = Block::genesis(genesis_metadata);
        blockchain.chain.push(genesis_block);

        blockchain
    }

    /// 创建新的区块链（带质量评估器）
    pub fn with_assessor(
        owner_address: String,
        config: BlockchainConfig,
        assessor: Box<dyn QualityAssessor>,
    ) -> Self {
        let mut blockchain = Self::with_config(owner_address, config);
        blockchain.assessor = Some(assessor);
        blockchain
    }

    /// 创建线程安全的区块链实例
    pub fn new_safe(owner_address: String) -> SafeBlockchain {
        Arc::new(RwLock::new(Self::new(owner_address)))
    }

    /// 设置质量评估器
    pub fn set_assessor(&mut self, assessor: Box<dyn QualityAssessor>) {
        self.assessor = Some(assessor);
    }

    /// 设置共识引擎
    pub fn set_consensus_engine(&mut self, engine: ConsensusEngine) {
        self.consensus_engine = engine;
    }

    /// 获取共识引擎（只读）
    pub fn consensus_engine(&self) -> &ConsensusEngine {
        &self.consensus_engine
    }

    /// 获取最新区块
    pub fn latest_block(&self) -> Option<&Block> {
        self.chain.last()
    }

    /// 获取最新区块（可变引用）
    pub fn latest_block_mut(&mut self) -> Option<&mut Block> {
        self.chain.last_mut()
    }

    /// 获取区块高度
    pub fn height(&self) -> u64 {
        self.chain.len() as u64
    }

    /// 获取整个链
    pub fn chain(&self) -> &[Block] {
        &self.chain
    }

    /// 添加待处理交易
    pub fn add_pending_transaction(&mut self, transaction: Transaction) {
        self.pending_transactions.push(transaction);
    }

    /// 添加 KV Cache 存证（创新 A）
    pub fn add_kv_proof(&mut self, proof: KvCacheProof) {
        self.pending_kv_proofs.push(proof);
    }

    /// 获取待处理交易数量
    pub fn pending_transaction_count(&self) -> usize {
        self.pending_transactions.len()
    }

    /// 获取待处理交易列表（只读）
    pub fn pending_transactions(&self) -> &Vec<Transaction> {
        &self.pending_transactions
    }

    /// 注册节点（用于分布式推理）
    pub fn register_node(&mut self, node_id: String) {
        self.reputation_manager.register_node(node_id);
    }

    /// 注册节点（带地址）
    pub fn register_node_with_address(&mut self, node_id: String, node_address: String) {
        self.reputation_manager.register_node_with_address(node_id, node_address);
    }

    /// 获取节点信誉
    pub fn get_node_reputation(&self, node_id: &str) -> Option<&NodeReputation> {
        self.reputation_manager.get_node(node_id)
    }

    /// 获取可信节点列表（用于调度）
    pub fn get_trustworthy_nodes(&self) -> Vec<&NodeReputation> {
        self.reputation_manager.get_trustworthy_nodes()
    }

    /// 获取需要多节点复核的节点
    pub fn get_nodes_needing_review(&self) -> Vec<&NodeReputation> {
        self.reputation_manager.get_nodes_needing_review()
    }

    /// 设置可信阈值
    pub fn set_trust_threshold(&mut self, threshold: f64) {
        self.reputation_manager.set_trust_threshold(threshold);
        self.config.trust_threshold = threshold;
    }

    /// 获取当前配置
    pub fn config(&self) -> &BlockchainConfig {
        &self.config
    }

    /// 提交推理记录到链上（单节点模式）
    ///
    /// 这是区块链作为"可信增强工具"的核心入口：
    /// - 将推理记录打包成区块
    /// - 将 KV Cache 存证上链（创新 A）
    /// - 更新节点信誉（创新 B）
    ///
    /// 参数：
    /// - metadata: 推理元数据
    /// - attester_id: 存证者 ID（执行推理的节点）
    pub fn commit_inference(
        &mut self,
        metadata: BlockMetadata,
        attester_id: String,
    ) -> Result<&Block, String> {
        // 测试模式：模拟提交失败
        #[cfg(test)]
        {
            if self.simulate_commit_failure {
                return Err("Simulated commit failure for testing".to_string());
            }
        }

        if self.pending_transactions.is_empty() {
            return Err("No pending transactions to commit".to_string());
        }

        // Gas 限制检查 1：检查交易数量
        if let Some(max_txs) = self.config.max_transactions_per_block {
            if self.pending_transactions.len() > max_txs {
                return Err(format!(
                    "Too many transactions: {} (max: {})",
                    self.pending_transactions.len(),
                    max_txs
                ));
            }
        }

        // Gas 限制检查 2：检查总 Gas 使用量
        let total_gas: u64 = self.pending_transactions.iter()
            .map(|tx| tx.gas_used)
            .sum();
        
        if let Some(max_gas) = self.config.max_gas_per_block {
            if total_gas > max_gas {
                return Err(format!(
                    "Gas limit exceeded: {} (max: {})",
                    total_gas,
                    max_gas
                ));
            }
        }

        // 确保节点已注册
        if self.reputation_manager.get_node(&attester_id).is_none() {
            self.register_node(attester_id.clone());
        }

        // 阶段 1：准备数据（避免借用冲突）
        // 先克隆需要的数据，再执行可变操作
        let node_info = self.reputation_manager.get_node(&attester_id)
            .map(|n| (n.score, n.completed_tasks, n.total_tokens_processed))
            .unwrap_or((1.0, 0, 0));

        // 创建存证元数据（创新 A/B/C/D 的链上记录）
        let attestation = AttestationMetadata::new(
            attester_id.clone(),
            AttestationType::KvCache,
            node_info.0,
        );

        // 准备区块数据
        let transactions = self.pending_transactions.clone();
        let kv_proofs = self.pending_kv_proofs.clone();

        let latest_block = self.latest_block()
            .ok_or_else(|| "Blockchain is empty, cannot create new block".to_string())?;
        let new_block = Block::new(
            latest_block.index + 1,
            latest_block.hash.clone(),
            transactions,
            metadata,
            kv_proofs,
            attestation,
        );

        // 验证区块
        if !new_block.verify() {
            return Err("New block verification failed".to_string());
        }

        // 获取 token 数
        let tokens = new_block.total_tokens();

        // 添加到链上
        self.chain.push(new_block);

        // 密封区块（提交后不可修改）
        if let Some(last_block) = self.chain.last_mut() {
            last_block.seal();
        } else {
            return Err("Failed to get last block after pushing".to_string());
        }

        // 更新节点信誉（任务成功）
        let block_height = self.height();
        if let Some(node) = self.reputation_manager.get_node_mut(&attester_id) {
            node.on_task_success(tokens, Some(block_height));
        } else {
            return Err(format!("Node {} not found", attester_id));
        }

        // 清空待处理池
        self.pending_transactions.clear();
        self.pending_kv_proofs.clear();

        self.chain.last().ok_or_else(|| "Failed to get last block".to_string())
    }

    /// 提交多节点推理结果（多节点并行模式）
    ///
    /// **架构逻辑**：
    /// - 推理负责算得对：多个节点并行计算
    /// - 评估器负责验得准：质量评估器验证每个结果
    /// - 多节点负责保安全：比较结果，选择最优
    /// - 区块链负责记可信：记录最终结果和异常节点
    /// - **共识引擎负责决策**：多数投票决定获胜者
    ///
    /// 参数：
    /// - metadata: 推理元数据
    /// - results: 多节点结果列表 (node_id, output, kv_proof)
    /// - expected_tokens: 预期 token 数量（可选）
    ///
    /// 返回：
    /// - 获胜节点的 ID 和区块引用
    pub fn commit_multi_node_inference(
        &mut self,
        metadata: BlockMetadata,
        results: Vec<(String, String, KvCacheProof)>,
        expected_tokens: Option<u64>,
    ) -> Result<(String, &Block), String> {
        if results.is_empty() {
            return Err("No results to commit".to_string());
        }

        if self.assessor.is_none() {
            return Err("Quality assessor not configured".to_string());
        }

        let assessor = self.assessor.as_ref()
            .ok_or_else(|| "Quality assessor not available".to_string())?;
        let block_height = self.height() + 1;

        // 阶段 1：质量评估
        let assessments: Vec<(String, String, KvCacheProof, QualityAssessment)> = results
            .into_iter()
            .map(|(node_id, output, kv_proof)| {
                let assessment = assessor.assess(&output, &kv_proof, expected_tokens);
                (node_id, output, kv_proof, assessment)
            })
            .collect();

        // 阶段 2：提取评估结果用于比较
        let assessment_refs: Vec<QualityAssessment> = assessments.iter().map(|(_, _, _, a)| a.clone()).collect();

        // 阶段 3：使用共识引擎投票决定获胜者
        // 准备投票数据：(node_id, output_hash, quality_score)
        let vote_data: Vec<(String, String, f64)> = assessments.iter()
            .map(|(node_id, output, _, assessment)| {
                use sha2::{Digest, Sha256};
                let output_hash = format!("{:x}", Sha256::digest(output.as_bytes()));
                (node_id.clone(), output_hash, assessment.overall_score)
            })
            .collect();

        let consensus_decision = self.consensus_engine.vote(&vote_data);

        // 根据共识决策处理
        let (winner_index, consensus_reached) = match &consensus_decision {
            ConsensusDecision::Unanimous { winner_id } => {
                // 一致同意，找到获胜节点索引
                let index = assessments.iter()
                    .position(|(id, _, _, _)| id == winner_id)
                    .ok_or_else(|| format!("Winner node {} not found", winner_id))?;
                (index, true)
            }
            ConsensusDecision::Majority { winner_id, agreement_ratio } => {
                // 多数同意
                println!(
                    "Consensus reached with majority: {} (agreement: {:.2}%)",
                    winner_id,
                    agreement_ratio * 100.0
                );
                let index = assessments.iter()
                    .position(|(id, _, _, _)| id == winner_id)
                    .ok_or_else(|| format!("Winner node {} not found", winner_id))?;
                (index, true)
            }
            ConsensusDecision::NoConsensus { requires_arbitration } => {
                // 无法达成共识
                if *requires_arbitration {
                    println!("No consensus reached, requires arbitration");
                    // fallback: 选择质量分数最高的节点
                    let best_index = MultiNodeComparator::select_best(&assessment_refs)
                        .ok_or_else(|| "All results were tampered or invalid".to_string())?;
                    (best_index, false)
                } else {
                    return Err("Consensus not reached and no arbitration available".to_string());
                }
            }
        };

        let (winner_id, winner_output, winner_kv_proof, _winner_assessment) = &assessments[winner_index];

        // 阶段 4：记录异常节点（KV 校验失败或质量过低的节点）
        for (i, (node_id, _, _, assessment)) in assessments.iter().enumerate() {
            if i != winner_index {
                if assessment.is_tampered {
                    // 恶意行为：KV 校验失败或语义检查失败
                    self.reputation_manager
                        .get_node_mut(node_id)
                        .ok_or_else(|| format!("Node {} not found", node_id))?
                        .on_malicious_behavior("多节点验证失败", Some(block_height));
                } else if assessment.needs_multi_node_review() {
                    // 质量中等，需要复核
                    self.reputation_manager
                        .get_node_mut(node_id)
                        .ok_or_else(|| format!("Node {} not found", node_id))?
                        .on_multi_node_disagreement(false, Some(block_height));
                }
            }
        }

        // 阶段 5：创建交易（包含获胜结果）
        let tx = Transaction::new_internal(
            winner_id.clone(),
            winner_output.clone(),
            crate::transaction::TransactionType::Internal,
            crate::transaction::TransactionPayload::None,
        );
        self.add_pending_transaction(tx);

        // 添加 KV Cache 存证
        self.add_kv_proof(winner_kv_proof.clone());

        // 阶段 6：提交到链上
        self.commit_inference(metadata, winner_id.clone())?;

        // 记录获胜节点的额外奖励
        {
            let node = self.reputation_manager
                .get_node_mut(winner_id)
                .ok_or_else(|| format!("Node {} not found", winner_id))?;

            // 如果达成共识，给予额外奖励
            if consensus_reached {
                // 共识达成，正常奖励
            } else {
                // 未达成共识但被选为赢家，轻微惩罚（可能是特殊情况）
                node.on_multi_node_disagreement(true, Some(block_height));
            }
        }

        let block_ref = self.chain.last()
            .ok_or_else(|| "Failed to get last block".to_string())?;
        Ok((winner_id.clone(), block_ref))
    }

    /// 报告节点任务失败（用于信誉惩罚）
    pub fn report_node_failure(&mut self, node_id: &str) -> Result<(), String> {
        let block_height = self.height();
        let node = self.reputation_manager
            .get_node_mut(node_id)
            .ok_or_else(|| format!("Node {} not found", node_id))?;

        node.on_task_failed(Some(block_height));
        Ok(())
    }

    /// 报告节点恶意行为（KV 校验失败、语义检查失败等）
    pub fn report_malicious_behavior(
        &mut self,
        node_id: &str,
        reason: &str,
    ) -> Result<(), String> {
        let block_height = self.height();
        let node = self.reputation_manager
            .get_node_mut(node_id)
            .ok_or_else(|| format!("Node {} not found", node_id))?;

        node.on_malicious_behavior(reason, Some(block_height));
        Ok(())
    }

    /// 获取信誉管理器（只读）
    pub fn reputation_manager(&self) -> &ReputationManager {
        &self.reputation_manager
    }

    /// 获取所有者地址
    pub fn owner_address(&self) -> &str {
        &self.owner_address
    }

    /// 获取信誉管理器（可变引用）
    pub fn reputation_manager_mut(&mut self) -> &mut ReputationManager {
        &mut self.reputation_manager
    }

    /// 获取节点数量
    pub fn node_count(&self) -> usize {
        self.reputation_manager.node_count()
    }

    /// 获取活跃节点数量
    pub fn active_node_count(&self) -> usize {
        self.reputation_manager.active_node_count()
    }

    /// 获取被剔除节点数量
    pub fn blacklisted_count(&self) -> usize {
        self.reputation_manager.blacklisted_count()
    }

    /// 验证整个区块链的有效性
    pub fn verify_chain(&self) -> bool {
        for i in 1..self.chain.len() {
            let current_block = &self.chain[i];
            let previous_block = &self.chain[i - 1];

            if current_block.previous_hash != previous_block.hash {
                return false;
            }

            if !current_block.verify() {
                return false;
            }
        }

        true
    }

    /// 验证并返回错误信息
    pub fn verify_chain_with_error(&self) -> Result<(), String> {
        for i in 1..self.chain.len() {
            let current_block = &self.chain[i];
            let previous_block = &self.chain[i - 1];

            if current_block.previous_hash != previous_block.hash {
                return Err(format!(
                    "Block {} previous_hash mismatch",
                    current_block.index
                ));
            }

            current_block
                .verify_with_error()
                .map_err(|e| format!("Block {} invalid: {}", current_block.index, e))?;
        }

        Ok(())
    }

    /// 根据索引获取区块
    pub fn get_block(&self, index: u64) -> Option<&Block> {
        self.chain.get(index as usize)
    }

    /// 获取所有 KV Cache 存证
    pub fn get_all_kv_proofs(&self) -> Vec<&KvCacheProof> {
        self.chain
            .iter()
            .flat_map(|block| &block.kv_proofs)
            .collect()
    }

    /// 验证 KV Cache 完整性（创新 A 的核心功能）
    pub fn verify_kv_integrity(&self, kv_block_id: &str, kv_data: &[u8]) -> bool {
        self.get_all_kv_proofs()
            .iter()
            .find(|p| p.kv_block_id == kv_block_id)
            .map(|p| p.verify_kv_integrity(kv_data))
            .unwrap_or(false)
    }

    /// 获取总推理 token 数
    pub fn total_inference_tokens(&self) -> u64 {
        self.chain
            .iter()
            .map(|block| block.total_tokens())
            .sum()
    }

    /// 获取总推理成本
    pub fn total_inference_cost(&self) -> f64 {
        self.chain
            .iter()
            .map(|block| block.metadata.compute_cost)
            .sum()
    }

    /// 获取待处理交易的总 Gas 使用量
    pub fn pending_gas_used(&self) -> u64 {
        self.pending_transactions.iter().map(|tx| tx.gas_used).sum()
    }

    /// 获取待处理交易的总 token 数
    pub fn pending_tokens(&self) -> u64 {
        self.pending_transactions.iter()
            .filter_map(|tx| {
                if let crate::transaction::TransactionPayload::InferenceRequest { max_tokens, .. } = &tx.payload {
                    Some(*max_tokens as u64)
                } else {
                    None
                }
            })
            .sum()
    }

    /// 检查交易是否可以添加到待处理池（基于 Gas 限制）
    pub fn can_add_transaction(&self, tx: &Transaction) -> Result<(), String> {
        // 检查交易数量限制
        if let Some(max_txs) = self.config.max_transactions_per_block {
            if self.pending_transactions.len() >= max_txs {
                return Err(format!("Transaction pool is full (max: {})", max_txs));
            }
        }

        // 检查 Gas 限制
        if let Some(max_gas) = self.config.max_gas_per_block {
            let new_total_gas = self.pending_gas_used() + tx.gas_used;
            if new_total_gas > max_gas {
                return Err(format!(
                    "Adding transaction would exceed gas limit (current: {}, new: {}, max: {})",
                    self.pending_gas_used(),
                    tx.gas_used,
                    max_gas
                ));
            }
        }

        Ok(())
    }

    /// 启用模拟提交失败（用于测试）
    #[cfg(test)]
    pub fn enable_simulated_commit_failure(&mut self) {
        self.simulate_commit_failure = true;
    }

    /// 禁用模拟提交失败（用于测试）
    #[cfg(test)]
    pub fn disable_simulated_commit_failure(&mut self) {
        self.simulate_commit_failure = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction::{Transaction, TransactionType, TransactionPayload};

    #[test]
    fn test_blockchain_creation() {
        let blockchain = Blockchain::new(String::from("user_123"));

        assert_eq!(blockchain.height(), 1);
        assert!(blockchain.verify_chain());
        assert_eq!(blockchain.node_count(), 0);
    }

    #[test]
    fn test_register_node_and_reputation() {
        let mut blockchain = Blockchain::new(String::from("user_123"));

        blockchain.register_node(String::from("node_1"));
        blockchain.register_node(String::from("node_2"));

        assert_eq!(blockchain.node_count(), 2);

        let node_1 = blockchain.get_node_reputation("node_1").unwrap();
        assert_eq!(node_1.score, 1.0);
        assert!(node_1.is_trustworthy(0.7));
    }

    #[test]
    fn test_commit_inference_with_attestation() {
        let mut blockchain = Blockchain::new(String::from("user_123"));

        blockchain.register_node(String::from("node_1"));

        let tx = Transaction::new_internal(
            String::from("user"),
            String::from("assistant"),
            crate::transaction::TransactionType::Internal,
            crate::transaction::TransactionPayload::None,
        );
        blockchain.add_pending_transaction(tx);

        let kv_proof = KvCacheProof::new(
            String::from("kv_001"),
            String::from("kv_hash_123"),
            String::from("node_1"),
            1024,
        );
        blockchain.add_kv_proof(kv_proof);

        let metadata = BlockMetadata::new(
            String::from("test-model"),
            String::from("1.0.0"),
            100,
            200,
            500,
            0.002,
            String::from("test-provider"),
        );

        let result = blockchain.commit_inference(metadata, String::from("node_1"));

        assert!(result.is_ok());
        assert_eq!(blockchain.height(), 2);

        let node_1 = blockchain.get_node_reputation("node_1").unwrap();
        assert!(node_1.score > 0.95);
        assert_eq!(node_1.completed_tasks, 1);
    }

    #[test]
    fn test_node_failure_penalty() {
        let mut blockchain = Blockchain::new(String::from("user_123"));

        blockchain.register_node(String::from("node_1"));
        blockchain.report_node_failure("node_1").unwrap();

        let node_1 = blockchain.get_node_reputation("node_1").unwrap();
        assert_eq!(node_1.failed_tasks, 1);
        assert_eq!(node_1.score, 0.9);
    }

    #[test]
    fn test_trustworthy_nodes_filtering() {
        let mut blockchain = Blockchain::new(String::from("user_123"));

        blockchain.register_node(String::from("good_node"));
        blockchain.register_node(String::from("bad_node"));

        for _ in 0..5 {
            blockchain.report_node_failure("bad_node").unwrap();
        }

        let trustworthy = blockchain.get_trustworthy_nodes();
        assert_eq!(trustworthy.len(), 1);
        assert_eq!(trustworthy[0].node_id, "good_node");
    }

    #[test]
    fn test_safe_blockchain() {
        let blockchain = Blockchain::new_safe(String::from("user_123"));
        
        // 测试多线程读取
        let blockchain_read = blockchain.clone();
        let handle = std::thread::spawn(move || {
            let bl = blockchain_read.read().unwrap();
            bl.height()
        });
        
        assert_eq!(handle.join().unwrap(), 1);
        
        // 测试多线程写入
        let blockchain_write = blockchain.clone();
        let handle = std::thread::spawn(move || {
            let mut bl = blockchain_write.write().unwrap();
            bl.register_node(String::from("node_1"));
            bl.node_count()
        });
        
        assert_eq!(handle.join().unwrap(), 1);
    }

    #[test]
    fn test_gas_limit_transaction_count() {
        let config = BlockchainConfig::default().with_max_transactions(2);
        let mut blockchain = Blockchain::with_config(String::from("user_123"), config);

        // 添加 2 个交易（达到限制）
        for i in 0..2 {
            let tx = Transaction::new_internal(
                format!("user_{}", i),
                format!("assistant_{}", i),
                TransactionType::Internal,
                TransactionPayload::None,
            );
            blockchain.add_pending_transaction(tx);
        }

        // 尝试添加第 3 个交易应该失败
        let tx = Transaction::new_internal(
            "user_2".to_string(),
            "assistant_2".to_string(),
            TransactionType::Internal,
            TransactionPayload::None,
        );
        
        // can_add_transaction 应该返回错误
        assert!(blockchain.can_add_transaction(&tx).is_err());

        // commit 应该成功（因为有 2 个交易，未超过限制）
        let metadata = BlockMetadata::default();
        let result = blockchain.commit_inference(metadata, "node_1".to_string());
        assert!(result.is_ok());
    }

    #[test]
    fn test_gas_limit_exceeded() {
        let config = BlockchainConfig::default().with_max_gas(100);
        let mut blockchain = Blockchain::with_config(String::from("user_123"), config);

        // 添加一个 gas 使用量为 60 的交易
        let mut tx1 = Transaction::new_internal(
            "user_1".to_string(),
            "assistant_1".to_string(),
            TransactionType::Internal,
            TransactionPayload::None,
        );
        tx1.gas_used = 60;
        blockchain.add_pending_transaction(tx1);

        // 添加一个 gas 使用量为 50 的交易（总和会超过 100）
        let mut tx2 = Transaction::new_internal(
            "user_2".to_string(),
            "assistant_2".to_string(),
            TransactionType::Internal,
            TransactionPayload::None,
        );
        tx2.gas_used = 50;
        blockchain.add_pending_transaction(tx2);

        // commit 应该失败（总 gas = 110 > 100）
        let metadata = BlockMetadata::default();
        let result = blockchain.commit_inference(metadata, "node_1".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Gas limit exceeded"));
    }

    #[test]
    fn test_pending_gas_calculation() {
        let mut blockchain = Blockchain::new(String::from("user_123"));

        // 添加不同 gas 使用量的交易
        let mut tx1 = Transaction::new_internal(
            "user_1".to_string(),
            "assistant_1".to_string(),
            TransactionType::Internal,
            TransactionPayload::None,
        );
        tx1.gas_used = 30;
        blockchain.add_pending_transaction(tx1);

        let mut tx2 = Transaction::new_internal(
            "user_2".to_string(),
            "assistant_2".to_string(),
            TransactionType::Internal,
            TransactionPayload::None,
        );
        tx2.gas_used = 50;
        blockchain.add_pending_transaction(tx2);

        // 验证总 gas 使用量
        assert_eq!(blockchain.pending_gas_used(), 80);
    }

    #[test]
    fn test_can_add_transaction() {
        let config = BlockchainConfig::default()
            .with_max_transactions(5)
            .with_max_gas(200);
        let mut blockchain = Blockchain::with_config(String::from("user_123"), config);

        // 添加一个交易
        let mut tx = Transaction::new_internal(
            "user_1".to_string(),
            "assistant_1".to_string(),
            TransactionType::Internal,
            TransactionPayload::None,
        );
        tx.gas_used = 50;

        // 应该可以添加
        assert!(blockchain.can_add_transaction(&tx).is_ok());

        // 添加交易
        blockchain.add_pending_transaction(tx);

        // 再次检查（仍然可以添加，因为未达到限制）
        let tx2 = Transaction::new_internal(
            "user_2".to_string(),
            "assistant_2".to_string(),
            TransactionType::Internal,
            TransactionPayload::None,
        );
        assert!(blockchain.can_add_transaction(&tx2).is_ok());
    }
}
