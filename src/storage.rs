//! 持久化模块 - 支持区块链数据的 JSON 文件持久化
//!
//! **功能**：
//! - 将区块链数据序列化并保存到 JSON 文件
//! - 从 JSON 文件加载并恢复区块链状态
//! - 支持自动备份和增量更新
//!
//! **使用示例**：
//!
//! ```ignore
//! use block_chain_with_context::{Blockchain, BlockchainConfig};
//! use block_chain_with_context::storage::JsonStorage;
//!
//! // 创建区块链
//! let mut blockchain = Blockchain::new("user_123".to_string());
//!
//! // 保存到文件
//! let storage = JsonStorage::new("blockchain.json");
//! storage.save(&blockchain).unwrap();
//!
//! // 从文件加载
//! let loaded_blockchain = storage.load("user_123".to_string()).unwrap();
//! ```

use serde::{Serialize, Deserialize};
use std::fs;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use crate::block::Block;
use crate::blockchain::Blockchain;
use crate::reputation::ReputationManager;

/// 区块链持久化数据（用于序列化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockchainData {
    /// 区块列表
    pub chain: Vec<Block>,
    /// 节点信誉管理器
    pub reputation_manager: ReputationManager,
    /// 所有者地址
    pub owner_address: String,
    /// 区块链配置
    pub config: BlockchainConfigData,
}

/// 区块链配置数据（用于序列化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockchainConfigData {
    /// 可信阈值
    pub trust_threshold: f64,
    /// 区块最大交易数
    pub max_transactions_per_block: Option<usize>,
    /// 区块最大 Gas
    pub max_gas_per_block: Option<u64>,
}

impl From<&crate::blockchain::BlockchainConfig> for BlockchainConfigData {
    fn from(config: &crate::blockchain::BlockchainConfig) -> Self {
        BlockchainConfigData {
            trust_threshold: config.trust_threshold,
            max_transactions_per_block: config.max_transactions_per_block,
            max_gas_per_block: config.max_gas_per_block,
        }
    }
}

impl From<BlockchainConfigData> for crate::blockchain::BlockchainConfig {
    fn from(data: BlockchainConfigData) -> Self {
        crate::blockchain::BlockchainConfig {
            trust_threshold: data.trust_threshold,
            max_transactions_per_block: data.max_transactions_per_block,
            max_gas_per_block: data.max_gas_per_block,
            ..Default::default()
        }
    }
}

impl BlockchainData {
    /// 从区块链创建持久化数据
    pub fn from_blockchain(blockchain: &Blockchain) -> Self {
        BlockchainData {
            chain: blockchain.chain().to_vec(),
            reputation_manager: blockchain.reputation_manager().clone(),
            owner_address: blockchain.owner_address.clone(),
            config: BlockchainConfigData::from(blockchain.config()),
        }
    }

    /// 转换为区块链实例
    pub fn into_blockchain(self) -> Blockchain {
        let mut blockchain = Blockchain::with_config(
            self.owner_address,
            self.config.into(),
        );

        // 恢复区块列表 - 覆盖创世区块
        blockchain.chain = self.chain;

        // 恢复信誉管理器
        blockchain.reputation_manager = self.reputation_manager;

        blockchain
    }
}

/// JSON 文件存储管理器
pub struct JsonStorage {
    /// 文件路径
    file_path: PathBuf,
}

impl JsonStorage {
    /// 创建新的存储管理器
    pub fn new<P: AsRef<Path>>(file_path: P) -> Self {
        JsonStorage {
            file_path: file_path.as_ref().to_path_buf(),
        }
    }

    /// 获取文件路径
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    /// 保存区块链到 JSON 文件
    pub fn save(&self, blockchain: &Blockchain) -> Result<(), String> {
        let data = BlockchainData::from_blockchain(blockchain);
        
        // 创建父目录（如果不存在）
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }

        // 写入临时文件
        let temp_path = self.file_path.with_extension("json.tmp");
        let file = fs::File::create(&temp_path)
            .map_err(|e| format!("Failed to create file: {}", e))?;
        
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &data)
            .map_err(|e| format!("Failed to serialize: {}", e))?;

        // 原子替换原文件
        fs::rename(&temp_path, &self.file_path)
            .map_err(|e| format!("Failed to rename file: {}", e))?;

        Ok(())
    }

    /// 从 JSON 文件加载区块链
    pub fn load(&self, _owner_address: String) -> Result<Blockchain, String> {
        if !self.file_path.exists() {
            return Err(format!("File not found: {}", self.file_path.display()));
        }

        let file = fs::File::open(&self.file_path)
            .map_err(|e| format!("Failed to open file: {}", e))?;
        
        let reader = BufReader::new(file);
        let data: BlockchainData = serde_json::from_reader(reader)
            .map_err(|e| format!("Failed to deserialize: {}", e))?;

        Ok(data.into_blockchain())
    }

    /// 检查文件是否存在
    pub fn exists(&self) -> bool {
        self.file_path.exists()
    }

    /// 删除存储文件
    pub fn delete(&self) -> Result<(), String> {
        if self.exists() {
            fs::remove_file(&self.file_path)
                .map_err(|e| format!("Failed to delete file: {}", e))?;
        }
        Ok(())
    }

    /// 创建备份
    pub fn backup(&self, backup_path: Option<&Path>) -> Result<PathBuf, String> {
        if !self.exists() {
            return Err("No file to backup".to_string());
        }

        let backup_path_buf = backup_path.map(|p| p.to_path_buf()).unwrap_or_else(|| {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            self.file_path.with_extension(format!("json.backup.{}", timestamp))
        });

        fs::copy(&self.file_path, &backup_path_buf)
            .map_err(|e| format!("Failed to create backup: {}", e))?;

        Ok(backup_path_buf)
    }

    /// 从备份恢复
    pub fn restore_from_backup(&self, backup_path: &Path) -> Result<(), String> {
        if !backup_path.exists() {
            return Err(format!("Backup file not found: {}", backup_path.display()));
        }

        fs::copy(backup_path, &self.file_path)
            .map_err(|e| format!("Failed to restore from backup: {}", e))?;

        Ok(())
    }
}

/// 存储错误类型
#[derive(Debug)]
pub enum StorageError {
    IoError(std::io::Error),
    SerializationError(String),
    FileNotFound(String),
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageError::IoError(e) => write!(f, "IO error: {}", e),
            StorageError::SerializationError(e) => write!(f, "Serialization error: {}", e),
            StorageError::FileNotFound(path) => write!(f, "File not found: {}", path),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<std::io::Error> for StorageError {
    fn from(err: std::io::Error) -> Self {
        StorageError::IoError(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockchain::Blockchain;
    use crate::transaction::{Transaction, TransactionType, TransactionPayload};
    use crate::metadata::BlockMetadata;

    fn create_test_blockchain() -> Blockchain {
        let mut blockchain = Blockchain::new("test_user".to_string());
        
        // 添加一些测试数据
        blockchain.register_node("node_1".to_string());
        
        let tx = Transaction::new_internal(
            "user".to_string(),
            "assistant".to_string(),
            TransactionType::Internal,
            TransactionPayload::None,
        );
        blockchain.add_pending_transaction(tx);

        let metadata = BlockMetadata::default();
        let _ = blockchain.commit_inference(metadata, "node_1".to_string());

        blockchain
    }

    #[test]
    fn test_json_storage_save_and_load() {
        let temp_path = std::env::temp_dir().join("test_blockchain.json");
        let storage = JsonStorage::new(&temp_path);

        // 创建并保存区块链
        let original = create_test_blockchain();
        storage.save(&original).unwrap();

        // 验证文件存在
        assert!(storage.exists());

        // 加载区块链
        let loaded = storage.load("test_user".to_string()).unwrap();

        // 验证数据一致性
        assert_eq!(original.height(), loaded.height());
        assert_eq!(original.node_count(), loaded.node_count());
        assert!(loaded.verify_chain());

        // 清理
        storage.delete().unwrap();
    }

    #[test]
    fn test_json_storage_backup() {
        let temp_path = std::env::temp_dir().join("test_blockchain_backup.json");
        let storage = JsonStorage::new(&temp_path);

        // 创建并保存区块链
        let _original = create_test_blockchain();
        storage.save(&_original).unwrap();

        // 创建备份
        let backup_path = storage.backup(None).unwrap();
        assert!(backup_path.exists());

        // 删除原文件
        storage.delete().unwrap();
        assert!(!storage.exists());

        // 从备份恢复
        storage.restore_from_backup(&backup_path).unwrap();
        assert!(storage.exists());

        // 清理
        storage.delete().unwrap();
        let _ = fs::remove_file(&backup_path);
    }

    #[test]
    fn test_json_storage_not_found() {
        let temp_path = std::env::temp_dir().join("non_existent.json");
        let storage = JsonStorage::new(&temp_path);

        assert!(!storage.exists());
        
        let result = storage.load("user".to_string());
        assert!(result.is_err());
    }
}
