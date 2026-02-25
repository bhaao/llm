//! 记忆层模块 - 区块链化分布式上下文存储核心
//!
//! **核心定位**：以区块为单位存储 KV/上下文分片，支持哈希链式串联、分布式多副本存储
//!
//! # 双链架构说明
//!
//! 本项目采用"双链架构"，两条链各司其职：
//!
//! ## 1. 区块链（Blockchain）- 主链
//!
//! - **定位**：全局可信存证链，所有节点共享
//! - **存储内容**：
//!   - 推理请求/响应的元数据
//!   - KV Cache 的哈希存证（KvCacheProof）
//!   - 节点信誉记录
//!   - 共识仲裁结果
//! - **特点**：
//!   - 不可篡改，全网共识
//!   - 仅存证哈希，不存储实际 KV 数据
//!   - 异步提交，不阻塞推理主流程
//!
//! ## 2. 记忆链（MemoryChain）- 数据链
//!
//! - **定位**：分布式 KV 上下文存储，按节点分片
//! - **存储内容**：
//!   - 实际的 KV 数据（上下文分片）
//!   - KV 哈希链式串联（防篡改）
//!   - 版本控制和访问授权
//! - **特点**：
//!   - 每个节点维护自己的记忆链
//!   - 支持多副本容灾
//!   - 仅哈希上链，数据本地存储
//!
//! ## 两条链的关系
//!
//! ```text
//! 推理流程：
//! 1. 推理提供商 → 从记忆链读取 KV 上下文
//! 2. 推理提供商 → 执行 LLM 推理
//! 3. 推理提供商 → 向记忆链写入新 KV
//! 4. 记忆层 → 计算新 KV 哈希
//! 5. 协调器 → 将 KV 哈希作为存证提交到区块链
//! 6. 区块链 → 验证并记录存证（异步）
//!
//! 验证流程：
//! 1. 验证方 → 从区块链读取 KV 哈希存证
//! 2. 验证方 → 从记忆链读取实际 KV 数据
//! 3. 验证方 → 计算 KV 哈希并与链上存证比对
//! 4. 验证方 → 确认数据完整性
//! ```
//!
//! # 核心职责
//!
//! 1. **区块化存储**：将超长上下文/KV 按固定大小分片，每片作为"记忆区块"
//! 2. **链式串联**：所有记忆区块按推理顺序哈希串联，形成"记忆链"
//! 3. **分布式多副本**：每个记忆区块在≥3 个节点存储，容灾且避免单点故障
//! 4. **版本控制/访问授权**：维护版本号，仅允许授权访问最新版本
//!
//! # 关键约束
//!
//! - **仅对接节点层做哈希校验**：不直接处理业务逻辑
//! - **仅向推理提供商开放只读/写权限**：需持有效访问凭证
//! - **热点数据本地化缓存**：性能保障

pub mod tiered_storage;
pub mod kv_compression;

use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::{Arc, RwLock};
use crate::node_layer::{AccessCredential, AccessType};

/// 记忆区块头 - 包含元数据和链式连接信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryBlockHeader {
    /// 区块高度（从 0 开始）
    pub index: u64,
    /// 时间戳
    pub timestamp: u64,
    /// 父区块哈希（创世区块为"0"）
    pub parent_hash: String,
    /// 当前区块哈希
    pub hash: String,
    /// 生成节点 ID
    pub generator_node_id: String,
    /// KV 数据默克尔根
    pub kv_merkle_root: String,
    /// 版本号（支持多版本控制）
    pub version: u64,
    /// 访问权限列表（授权的提供商 ID）
    pub access_permissions: Vec<String>,
}

impl MemoryBlockHeader {
    pub fn new(
        index: u64,
        parent_hash: String,
        generator_node_id: String,
        kv_merkle_root: String,
        version: u64,
    ) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut header = MemoryBlockHeader {
            index,
            timestamp,
            parent_hash,
            hash: String::new(),
            generator_node_id,
            kv_merkle_root,
            version,
            access_permissions: Vec::new(),
        };

        header.hash = header.calculate_hash();
        header
    }

    /// 计算区块头哈希
    pub fn calculate_hash(&self) -> String {
        let data = format!(
            "{}:{}:{}:{}:{}:{}:{}",
            self.index,
            self.timestamp,
            self.parent_hash,
            self.generator_node_id,
            self.kv_merkle_root,
            self.version,
            self.access_permissions.join(",")
        );
        format!("{:x}", Sha256::digest(data.as_bytes()))
    }

    /// 添加访问权限
    pub fn add_permission(&mut self, provider_id: String) {
        if !self.access_permissions.contains(&provider_id) {
            self.access_permissions.push(provider_id);
            self.hash = self.calculate_hash();
        }
    }

    /// 检查是否有访问权限
    pub fn has_permission(&self, provider_id: &str) -> bool {
        self.access_permissions.is_empty() || self.access_permissions.iter().any(|p| p == provider_id)
    }
}

/// KV 分片数据 - 单个 KV 对
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvShard {
    /// KV 键
    pub key: String,
    /// KV 值（原始字节）
    pub value: Vec<u8>,
    /// KV 哈希（用于快速校验）
    pub hash: String,
    /// 创建时间戳
    pub created_at: u64,
    /// 最后修改时间
    pub updated_at: u64,
}

impl KvShard {
    pub fn new(key: String, value: Vec<u8>) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let hash = format!("{:x}", Sha256::digest(&value));

        KvShard {
            key,
            value,
            hash,
            created_at: timestamp,
            updated_at: timestamp,
        }
    }

    /// 更新 KV 值
    pub fn update(&mut self, new_value: Vec<u8>) {
        self.value = new_value;
        self.hash = format!("{:x}", Sha256::digest(&self.value));
        self.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// 验证 KV 完整性
    pub fn verify_integrity(&self) -> bool {
        let computed_hash = format!("{:x}", Sha256::digest(&self.value));
        computed_hash == self.hash
    }
}

/// 记忆区块 - 包含多个 KV 分片的完整区块
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryBlock {
    /// 区块头
    pub header: MemoryBlockHeader,
    /// KV 分片列表
    pub shards: Vec<KvShard>,
    /// 区块是否已密封（提交后不可修改）
    pub is_sealed: bool,
    /// 副本位置列表（节点 ID）
    pub replica_locations: Vec<String>,
    /// 区块是否已回滚
    pub is_rolled_back: bool,
    /// 回滚前的 KV 快照（用于恢复）
    pub rolled_back_shards: Option<Vec<KvShard>>,
}

impl MemoryBlock {
    /// 创建新的记忆区块
    pub fn new(
        index: u64,
        parent_hash: String,
        generator_node_id: String,
        version: u64,
    ) -> Self {
        let header = MemoryBlockHeader::new(
            index,
            parent_hash,
            generator_node_id,
            String::new(), // 初始为空
            version,
        );

        MemoryBlock {
            header,
            shards: Vec::new(),
            is_sealed: false,
            replica_locations: Vec::new(),
            is_rolled_back: false,
            rolled_back_shards: None,
        }
    }

    /// 创建创世区块
    pub fn genesis(generator_node_id: String) -> Self {
        let mut block = MemoryBlock::new(0, "0".to_string(), generator_node_id, 1);
        block.seal();
        block
    }

    /// 添加 KV 分片
    pub fn add_shard(&mut self, shard: KvShard) -> Result<(), String> {
        if self.is_sealed {
            return Err(format!(
                "Cannot modify sealed memory block at index {}",
                self.header.index
            ));
        }

        self.shards.push(shard);
        self.update_merkle_root();
        Ok(())
    }

    /// 获取 KV 分片
    pub fn get_shard(&self, key: &str) -> Option<&KvShard> {
        self.shards.iter().find(|s| s.key == key)
    }

    /// 获取 KV 分片（可变引用）
    pub fn get_shard_mut(&mut self, key: &str) -> Option<&mut KvShard> {
        self.shards.iter_mut().find(|s| s.key == key)
    }

    /// 更新默克尔根
    fn update_merkle_root(&mut self) {
        if self.shards.is_empty() {
            self.header.kv_merkle_root = String::new();
        } else {
            let hashes: Vec<String> = self.shards.iter().map(|s| s.hash.clone()).collect();
            self.header.kv_merkle_root = Self::compute_merkle_root(&hashes);
        }
        self.header.hash = self.header.calculate_hash();
    }

    /// 计算默克尔根
    fn compute_merkle_root(hashes: &[String]) -> String {
        if hashes.is_empty() {
            return "0000000000000000000000000000000000000000000000000000000000000000".to_string();
        }

        let mut current_level = hashes.to_vec();

        while current_level.len() > 1 {
            let mut next_level = Vec::new();
            for chunk in current_level.chunks(2) {
                let combined = match chunk.len() {
                    2 => format!("{}{}", chunk[0], chunk[1]),
                    1 => format!("{}{}", chunk[0], chunk[0]),
                    _ => unreachable!(),
                };
                next_level.push(format!("{:x}", Sha256::digest(combined.as_bytes())));
            }
            current_level = next_level;
        }

        current_level.into_iter().next().unwrap_or_default()
    }

    /// 密封区块（提交后不可修改）
    pub fn seal(&mut self) {
        self.is_sealed = true;
    }

    /// 检查区块是否已密封
    pub fn is_sealed(&self) -> bool {
        self.is_sealed
    }

    /// 验证区块完整性
    pub fn verify(&self) -> bool {
        // 验证哈希
        if self.header.hash != self.header.calculate_hash() {
            return false;
        }

        // 验证默克尔根
        if !self.shards.is_empty() {
            let computed_merkle = Self::compute_merkle_root(
                &self.shards.iter().map(|s| s.hash.clone()).collect::<Vec<_>>()
            );
            if computed_merkle != self.header.kv_merkle_root {
                return false;
            }
        }

        // 验证所有 KV 分片完整性
        self.shards.iter().all(|s| s.verify_integrity())
    }

    /// 验证访问权限
    pub fn verify_access(&self, credential: &AccessCredential) -> bool {
        // 检查凭证是否有效
        if !credential.is_valid() {
            return false;
        }

        // 检查访问类型
        match credential.access_type {
            AccessType::ReadOnly | AccessType::ReadWrite => {
                self.header.has_permission(&credential.provider_id)
            }
            AccessType::WriteOnly => {
                // 写入权限只需要凭证有效
                true
            }
        }
    }

    /// 获取 KV 数量
    pub fn shard_count(&self) -> usize {
        self.shards.len()
    }

    /// 获取总 token 数（估算）
    pub fn total_tokens(&self) -> u64 {
        self.shards.iter().map(|s| s.value.len() as u64).sum()
    }
}

/// 记忆层管理器 - 管理分布式记忆区块
#[derive(Debug)]
pub struct MemoryLayerManager {
    /// 记忆区块列表（按索引）
    blocks: HashMap<u64, MemoryBlock>,
    /// 最新区块索引
    latest_block_index: u64,
    /// 热点缓存（最近访问的 KV）
    hot_cache: HashMap<String, Arc<RwLock<KvShard>>>,
    /// 版本映射（支持多版本）
    version_map: HashMap<u64, u64>, // block_index -> version
}

impl Clone for MemoryLayerManager {
    fn clone(&self) -> Self {
        MemoryLayerManager {
            blocks: self.blocks.clone(),
            latest_block_index: self.latest_block_index,
            hot_cache: self.hot_cache.clone(),
            version_map: self.version_map.clone(),
        }
    }
}

impl MemoryLayerManager {
    /// 创建新的记忆层管理器
    pub fn new(generator_node_id: &str) -> Self {
        let genesis_block = MemoryBlock::genesis(generator_node_id.to_string());
        let mut blocks = HashMap::new();
        blocks.insert(0, genesis_block);

        MemoryLayerManager {
            blocks,
            latest_block_index: 0,
            hot_cache: HashMap::new(),
            version_map: HashMap::new(),
        }
    }

    /// 获取最新区块
    pub fn latest_block(&self) -> Option<&MemoryBlock> {
        self.blocks.get(&self.latest_block_index)
    }

    /// 获取最新区块（可变引用）
    pub fn latest_block_mut(&mut self) -> Option<&mut MemoryBlock> {
        self.blocks.get_mut(&self.latest_block_index)
    }

    /// 获取区块高度
    pub fn height(&self) -> u64 {
        self.latest_block_index + 1
    }

    /// 获取最新区块索引
    pub fn latest_block_index(&self) -> u64 {
        self.latest_block_index
    }

    /// 创建新的记忆区块
    pub fn create_new_block(&mut self, generator_node_id: &str) -> Option<&MemoryBlock> {
        let parent_hash = self.latest_block()?.header.hash.clone();
        let new_index = self.latest_block_index + 1;
        let new_version = self.version_map.get(&new_index).unwrap_or(&1) + 1;

        let new_block = MemoryBlock::new(
            new_index,
            parent_hash,
            generator_node_id.to_string(),
            new_version,
        );

        let version = new_block.header.version;
        self.blocks.insert(new_index, new_block);
        self.latest_block_index = new_index;
        self.version_map.insert(new_index, version);

        self.blocks.get(&new_index)
    }

    /// 获取区块（只读）
    pub fn get_block(&self, index: u64) -> Option<&MemoryBlock> {
        self.blocks.get(&index)
    }

    /// 获取区块（可变引用）
    pub fn get_block_mut(&mut self, index: u64) -> Option<&mut MemoryBlock> {
        self.blocks.get_mut(&index)
    }

    /// 写入 KV 数据
    pub fn write_kv(
        &mut self,
        key: String,
        value: Vec<u8>,
        provider_credential: &AccessCredential,
    ) -> Result<(), String> {
        // 验证写入权限
        if provider_credential.access_type != AccessType::WriteOnly
            && provider_credential.access_type != AccessType::ReadWrite
        {
            return Err("No write permission".to_string());
        }

        // 获取最新区块（用于写入）
        let latest_block = self.latest_block_mut()
            .ok_or_else(|| "Failed to get latest block".to_string())?;

        // 检查区块是否已密封
        if latest_block.is_sealed {
            // 创建新区块
            let generator_id = latest_block.header.generator_node_id.clone();
            self.create_new_block(&generator_id)
                .ok_or_else(|| "Failed to create new block".to_string())?;
        }

        // 写入 KV
        let block = self.latest_block_mut()
            .ok_or_else(|| "Failed to get latest block".to_string())?;

        // 检查是否已存在相同 key
        if let Some(existing_shard) = block.get_shard_mut(&key) {
            existing_shard.update(value);
        } else {
            let shard = KvShard::new(key, value);
            block.add_shard(shard)?;
        }

        Ok(())
    }

    /// 读取 KV 数据
    pub fn read_kv(
        &self,
        key: &str,
        provider_credential: &AccessCredential,
    ) -> Option<KvShard> {
        // 验证读取权限
        if provider_credential.access_type != AccessType::ReadOnly
            && provider_credential.access_type != AccessType::ReadWrite
        {
            return None;
        }

        // 先检查热点缓存
        if let Some(cached) = self.hot_cache.get(key) {
            if let Ok(shard) = cached.read() {
                return Some(shard.clone());
            }
            // 如果锁中毒，继续从主存储中读取
        }

        // 从后向前搜索（最新版本优先）
        for index in (0..=self.latest_block_index).rev() {
            if let Some(block) = self.blocks.get(&index) {
                // 验证访问权限
                if !block.verify_access(provider_credential) {
                    continue;
                }

                if let Some(shard) = block.get_shard(key) {
                    return Some(shard.clone());
                }
            }
        }

        None
    }

    /// 密封当前区块（提交到链上）
    pub fn seal_current_block(&mut self) {
        if let Some(block) = self.blocks.get_mut(&self.latest_block_index) {
            block.seal();
        }
    }

    /// 添加副本位置
    pub fn add_replica(&mut self, block_index: u64, node_id: String) -> Result<(), String> {
        let block = self.blocks.get_mut(&block_index)
            .ok_or_else(|| format!("Block {} not found", block_index))?;

        if !block.replica_locations.contains(&node_id) {
            block.replica_locations.push(node_id);
        }

        Ok(())
    }

    /// 获取副本位置
    pub fn get_replicas(&self, block_index: u64) -> Option<&Vec<String>> {
        self.blocks.get(&block_index).map(|b| &b.replica_locations)
    }

    /// 验证记忆链完整性
    pub fn verify_chain(&self) -> bool {
        for i in 1..=self.latest_block_index {
            let current = match self.blocks.get(&i) {
                Some(b) => b,
                None => return false,
            };

            let parent = match self.blocks.get(&(i - 1)) {
                Some(b) => b,
                None => return false,
            };

            // 验证父哈希链接
            if current.header.parent_hash != parent.header.hash {
                return false;
            }

            // 验证区块完整性
            if !current.verify() {
                return false;
            }
        }

        true
    }

    /// 哈希校验（与链上存证对比）
    pub fn verify_hash(&self, block_index: u64, expected_hash: &str) -> bool {
        self.blocks.get(&block_index)
            .map(|b| b.header.hash == expected_hash)
            .unwrap_or(false)
    }

    /// 获取所有 KV 证明（用于上链存证）
    pub fn get_all_kv_proofs(&self) -> Vec<KvProof> {
        let mut proofs = Vec::new();

        for block in self.blocks.values() {
            for shard in &block.shards {
                proofs.push(KvProof::new(
                    format!("block_{}_{}", block.header.index, shard.key),
                    shard.hash.clone(),
                    block.header.generator_node_id.clone(),
                    shard.value.len() as u64,
                ));
            }
        }

        proofs
    }

    /// 添加到热点缓存
    pub fn add_to_hot_cache(&mut self, key: String, shard: KvShard) {
        self.hot_cache.insert(key, Arc::new(RwLock::new(shard)));
    }

    /// 获取区块数量
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// 获取总 KV 数量
    pub fn total_kv_count(&self) -> usize {
        self.blocks.values().map(|b| b.shard_count()).sum()
    }

    /// 标记当前区块为已回滚（用于异步上链失败时的回滚）
    /// 
    /// **回滚逻辑**：
    /// 1. 保存当前 KV 快照（用于审计）
    /// 2. 清空当前区块的 KV 分片
    /// 3. 标记区块为已回滚状态
    /// 4. 创建新区块供后续使用
    pub fn mark_current_block_as_rolled_back(&mut self) -> Result<(), String> {
        let block_index = self.latest_block_index;
        
        // 获取当前区块
        let block = self.blocks.get_mut(&block_index)
            .ok_or_else(|| format!("Block {} not found", block_index))?;
        
        // 如果已经回滚过，跳过
        if block.is_rolled_back {
            println!(
                "[MemoryLayer Rollback] Block {} already rolled back, skipping",
                block_index
            );
            return Ok(());
        }
        
        // 保存 KV 快照（用于审计和可能的恢复）
        let snapshot = block.shards.clone();
        
        // 标记为已回滚
        block.is_rolled_back = true;
        block.rolled_back_shards = Some(snapshot);
        
        // 清空 KV 分片（撤销写入）
        block.shards.clear();
        block.header.kv_merkle_root = String::new();
        block.header.hash = block.header.calculate_hash();
        
        println!(
            "[MemoryLayer Rollback] Block {} rolled back, {} KV shards cleared",
            block_index, block.rolled_back_shards.as_ref().map(|s| s.len()).unwrap_or(0)
        );
        
        // 创建新区块供后续使用
        let generator_id = block.header.generator_node_id.clone();
        let _ = block; // 释放可变借用
        self.create_new_block(&generator_id);
        
        Ok(())
    }
    
    /// 获取回滚的 KV 快照（用于审计）
    pub fn get_rolled_back_snapshot(&self, block_index: u64) -> Option<Vec<KvShard>> {
        self.blocks.get(&block_index)
            .filter(|b| b.is_rolled_back)
            .and_then(|b| b.rolled_back_shards.clone())
    }
    
    /// 检查区块是否已回滚
    pub fn is_block_rolled_back(&self, block_index: u64) -> bool {
        self.blocks.get(&block_index)
            .map(|b| b.is_rolled_back)
            .unwrap_or(false)
    }
}

/// KV 证明 - 用于上链存证的简化结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvProof {
    /// KV 块标识
    pub kv_block_id: String,
    /// KV 数据哈希
    pub kv_hash: String,
    /// 所属节点 ID
    pub node_id: String,
    /// KV 块大小（字节数）
    pub kv_size: u64,
}

impl KvProof {
    pub fn new(kv_block_id: String, kv_hash: String, node_id: String, kv_size: u64) -> Self {
        KvProof {
            kv_block_id,
            kv_hash,
            node_id,
            kv_size,
        }
    }

    /// 验证 KV 数据完整性
    pub fn verify_kv_integrity(&self, kv_data: &[u8]) -> bool {
        let computed_hash = format!("{:x}", Sha256::digest(kv_data));
        computed_hash == self.kv_hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_layer::{AccessType, AccessCredential};

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
    fn test_memory_block_creation() {
        let block = MemoryBlock::genesis("node_1".to_string());

        assert_eq!(block.header.index, 0);
        assert_eq!(block.header.parent_hash, "0");
        assert!(block.is_sealed);
        assert!(block.verify());
    }

    #[test]
    fn test_kv_shard_integrity() {
        let mut shard = KvShard::new("key_1".to_string(), b"value_1".to_vec());
        assert!(shard.verify_integrity());

        // 篡改数据
        shard.value = b"tampered".to_vec();
        assert!(!shard.verify_integrity());
    }

    #[test]
    fn test_memory_layer_write_read() {
        let mut manager = MemoryLayerManager::new("node_1");
        let credential = create_test_credential();

        // 写入 KV
        manager.write_kv("key_1".to_string(), b"value_1".to_vec(), &credential).unwrap();
        manager.write_kv("key_2".to_string(), b"value_2".to_vec(), &credential).unwrap();

        // 读取 KV
        let shard = manager.read_kv("key_1", &credential).unwrap();
        assert_eq!(shard.key, "key_1");
        assert_eq!(shard.value, b"value_1");
    }

    #[test]
    fn test_memory_chain_verification() {
        let mut manager = MemoryLayerManager::new("node_1");
        let credential = create_test_credential();

        // 写入多个区块
        for i in 0..5 {
            manager.write_kv(
                format!("key_{}", i),
                format!("value_{}", i).into_bytes(),
                &credential,
            ).unwrap();

            // 密封当前区块，强制创建新区块
            if i % 2 == 0 {
                manager.seal_current_block();
            }
        }

        // 验证链完整性
        assert!(manager.verify_chain());
    }

    #[test]
    fn test_block_sealing() {
        let mut manager = MemoryLayerManager::new("node_1");
        let credential = create_test_credential();

        manager.write_kv("key_1".to_string(), b"value_1".to_vec(), &credential).unwrap();
        manager.seal_current_block();

        // 密封后尝试修改应该失败
        let block = manager.get_block(manager.latest_block_index).unwrap();
        assert!(block.is_sealed);
    }

    #[test]
    fn test_replica_management() {
        let mut manager = MemoryLayerManager::new("node_1");

        // 添加副本
        manager.add_replica(0, "node_2".to_string()).unwrap();
        manager.add_replica(0, "node_3".to_string()).unwrap();

        let replicas = manager.get_replicas(0).unwrap();
        assert_eq!(replicas.len(), 2);
        assert!(replicas.contains(&"node_2".to_string()));
        assert!(replicas.contains(&"node_3".to_string()));
    }

    #[test]
    fn test_hash_verification() {
        let manager = MemoryLayerManager::new("node_1");
        let expected_hash = manager.latest_block()
            .map(|b| b.header.hash.clone())
            .unwrap_or_default();

        assert!(manager.verify_hash(0, &expected_hash));
        assert!(!manager.verify_hash(0, "invalid_hash"));
    }
}
