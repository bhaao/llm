//! 上下文分片模块 - 支持跨节点分布式存储长上下文
//!
//! **核心功能**：
//! - 将超长上下文（100K+ tokens）分割成多个分片
//! - 每个分片可以存储在不同的节点上
//! - 支持分片的重新组装
//! - 与记忆层集成，支持分片级别的 KV 存储
//!
//! # 架构设计
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │  原始上下文 (100K tokens)                            │
//! │  [token_0, token_1, ..., token_99999]                │
//! └─────────────────────────────────────────────────────┘
//!                      ↓ slice_context()
//! ┌─────────────────────────────────────────────────────┐
//! │  ContextShard 分片列表                               │
//! │  ┌──────────────┐  ┌──────────────┐  ┌───────────┐  │
//! │  │ Shard 0      │  │ Shard 1      │  │ Shard N   │  │
//! │  │ node_id=A    │  │ node_id=B    │  │ node_id=C │  │
//! │  │ tokens=0-999 │  │ tokens=1000- │  │ ...       │  │
//! │  │              │  │        1999  │  │           │  │
//! │  └──────────────┘  └──────────────┘  └───────────┘  │
//! └─────────────────────────────────────────────────────┘
//! ```
//!
//! # 使用示例
//!
//! ```ignore
//! use block_chain_with_context::memory_layer::context_sharding::{
//!     ContextShard, ContextShardManager,
//! };
//!
//! // 创建分片管理器
//! let manager = ContextShardManager::new();
//!
//! // 准备 tokens 数据
//! let tokens: Vec<u64> = (0..100_000).collect();
//!
//! // 分割成 10 个分片
//! let shards = manager.slice_context(&tokens, 10).await.unwrap();
//! assert_eq!(shards.len(), 10);
//!
//! // 每个分片可以存储到不同节点
//! for shard in &shards {
//!     println!("Shard {}: tokens {}-{}, stored on {}",
//!              shard.shard_id, shard.token_range.0, shard.token_range.1,
//!              shard.node_id);
//! }
//!
//! // 重新组装上下文
//! let shard_ids: Vec<u64> = shards.iter().map(|s| s.shard_id).collect();
//! let reassembled = manager.reassemble_context(&shard_ids).await.unwrap();
//! assert_eq!(reassembled, tokens);
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use anyhow::{Result, Context};

/// Token ID 类型别名
pub type TokenId = u64;

/// 上下文分片 - 存储长上下文的一部分
///
/// **设计说明**：
/// - 每个分片包含原始 tokens 的一个连续子序列
/// - 分片可以存储在不同的节点上
/// - 分片数据经过压缩以节省存储空间
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextShard {
    /// 分片唯一标识
    pub shard_id: u64,
    /// 存储此分片的节点 ID
    pub node_id: String,
    /// Token 范围 (start, end)，end  exclusive
    pub token_range: (usize, usize),
    /// 分片数据（压缩后的字节）
    pub kv_data: Vec<u8>,
    /// 分片哈希（用于校验完整性）
    pub shard_hash: String,
    /// 创建时间戳
    pub created_at: u64,
    /// 版本号（支持多版本）
    pub version: u64,
}

impl ContextShard {
    /// 创建新的上下文分片
    pub fn new(
        shard_id: u64,
        node_id: String,
        token_range: (usize, usize),
        kv_data: Vec<u8>,
    ) -> Self {
        use sha2::{Sha256, Digest};
        use std::time::{SystemTime, UNIX_EPOCH};

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let hash = format!("{:x}", Sha256::digest(&kv_data));

        ContextShard {
            shard_id,
            node_id,
            token_range,
            kv_data,
            shard_hash: hash,
            created_at: timestamp,
            version: 1,
        }
    }

    /// 验证分片完整性
    pub fn verify_integrity(&self) -> bool {
        use sha2::{Sha256, Digest};
        let computed_hash = format!("{:x}", Sha256::digest(&self.kv_data));
        computed_hash == self.shard_hash
    }

    /// 获取分片中的 token 数量
    pub fn token_count(&self) -> usize {
        self.token_range.1 - self.token_range.0
    }
}

/// 上下文分片管理器 - 管理分片的创建、存储和重新组装
///
/// **核心职责**：
/// 1. 将长上下文分割成多个分片
/// 2. 为每个分片分配存储节点
/// 3. 重新组装分片为完整上下文
/// 4. 维护分片元数据和索引
pub struct ContextShardManager {
    /// 分片存储（内存缓存）
    shards: Arc<RwLock<HashMap<u64, ContextShard>>>,
    /// 分片索引：context_id -> shard_ids
    context_index: Arc<RwLock<HashMap<String, Vec<u64>>>>,
    /// 节点分片映射：node_id -> shard_ids
    node_shard_map: Arc<RwLock<HashMap<String, Vec<u64>>>>,
    /// 下一个可用的分片 ID
    next_shard_id: Arc<RwLock<u64>>,
}

impl ContextShardManager {
    /// 创建新的上下文分片管理器
    pub fn new() -> Self {
        ContextShardManager {
            shards: Arc::new(RwLock::new(HashMap::new())),
            context_index: Arc::new(RwLock::new(HashMap::new())),
            node_shard_map: Arc::new(RwLock::new(HashMap::new())),
            next_shard_id: Arc::new(RwLock::new(0)),
        }
    }

    /// 获取下一个分片 ID
    async fn get_next_shard_id(&self) -> u64 {
        let mut next_id = self.next_shard_id.write().await;
        let id = *next_id;
        *next_id += 1;
        id
    }

    /// 将上下文分割成多个分片
    ///
    /// # 参数
    ///
    /// * `context_id` - 上下文的唯一标识
    /// * `tokens` - Token 数据
    /// * `num_shards` - 期望的分片数量
    /// * `node_ids` - 可用的节点 ID 列表（用于分配分片）
    ///
    /// # 返回
    ///
    /// * `Result<Vec<ContextShard>>` - 分片列表或错误
    ///
    /// # 分片策略
    ///
    /// - 均匀分割：每个分片包含大致相同数量的 tokens
    /// - 轮询分配：分片轮流分配到不同节点
    pub async fn slice_context(
        &self,
        context_id: &str,
        tokens: &[TokenId],
        num_shards: usize,
        node_ids: &[String],
    ) -> Result<Vec<ContextShard>> {
        if num_shards == 0 {
            anyhow::bail!("Number of shards must be greater than 0");
        }
        if node_ids.is_empty() {
            anyhow::bail!("At least one node ID must be provided");
        }

        let mut shards = Vec::with_capacity(num_shards);
        let tokens_per_shard = (tokens.len() + num_shards - 1) / num_shards; // 向上取整
        let mut shard_ids = Vec::with_capacity(num_shards);

        for i in 0..num_shards {
            let start = i * tokens_per_shard;
            if start >= tokens.len() {
                break;
            }
            let end = std::cmp::min((i + 1) * tokens_per_shard, tokens.len());

            // 轮询分配节点
            let node_id = node_ids[i % node_ids.len()].clone();

            // 创建分片数据（这里简单地将 token ID 序列化为字节）
            // 实际应用中应该存储 KV 数据
            let mut kv_data = Vec::new();
            for &token in &tokens[start..end] {
                kv_data.extend_from_slice(&token.to_le_bytes());
            }

            let shard_id = self.get_next_shard_id().await;
            let shard = ContextShard::new(
                shard_id,
                node_id.clone(),
                (start, end),
                kv_data,
            );

            // 存储分片
            {
                let mut shards_map = self.shards.write().await;
                shards_map.insert(shard_id, shard.clone());
            }

            // 更新节点映射
            {
                let mut node_map = self.node_shard_map.write().await;
                node_map.entry(node_id).or_insert_with(Vec::new).push(shard_id);
            }

            shard_ids.push(shard_id);
            shards.push(shard);
        }

        // 更新上下文索引
        {
            let mut index = self.context_index.write().await;
            index.insert(context_id.to_string(), shard_ids);
        }

        Ok(shards)
    }

    /// 重新组装上下文
    ///
    /// # 参数
    ///
    /// * `shard_ids` - 分片 ID 列表（必须按顺序提供）
    ///
    /// # 返回
    ///
    /// * `Result<Vec<TokenId>>` - 重新组装的 tokens 或错误
    pub async fn reassemble_context(&self, shard_ids: &[u64]) -> Result<Vec<TokenId>> {
        let shards_map = self.shards.read().await;
        let mut tokens = Vec::new();

        for &shard_id in shard_ids {
            let shard = shards_map.get(&shard_id)
                .with_context(|| format!("Shard {} not found", shard_id))?;

            // 验证分片完整性
            if !shard.verify_integrity() {
                anyhow::bail!("Shard {} integrity check failed", shard_id);
            }

            // 反序列化 token 数据
            let mut offset = 0;
            while offset < shard.kv_data.len() {
                if offset + 8 > shard.kv_data.len() {
                    anyhow::bail!("Invalid shard data format");
                }
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&shard.kv_data[offset..offset + 8]);
                tokens.push(TokenId::from_le_bytes(bytes));
                offset += 8;
            }
        }

        Ok(tokens)
    }

    /// 根据 context_id 重新组装上下文
    pub async fn reassemble_by_context_id(&self, context_id: &str) -> Result<Vec<TokenId>> {
        let index = self.context_index.read().await;
        let shard_ids = index.get(context_id)
            .with_context(|| format!("Context {} not found", context_id))?;

        self.reassemble_context(shard_ids).await
    }

    /// 获取分片
    pub async fn get_shard(&self, shard_id: u64) -> Option<ContextShard> {
        let shards_map = self.shards.read().await;
        shards_map.get(&shard_id).cloned()
    }

    /// 获取节点上的所有分片
    pub async fn get_shards_by_node(&self, node_id: &str) -> Vec<ContextShard> {
        let node_map = self.node_shard_map.read().await;
        let shard_ids = match node_map.get(node_id) {
            Some(ids) => ids.clone(),
            None => return Vec::new(),
        };

        let shards_map = self.shards.read().await;
        shard_ids.iter()
            .filter_map(|id| shards_map.get(id).cloned())
            .collect()
    }

    /// 删除分片
    pub async fn delete_shard(&self, shard_id: u64) -> Result<()> {
        let mut shards_map = self.shards.write().await;
        let shard = shards_map.remove(&shard_id)
            .with_context(|| format!("Shard {} not found", shard_id))?;

        // 从节点映射中移除
        {
            let mut node_map = self.node_shard_map.write().await;
            if let Some(shard_ids) = node_map.get_mut(&shard.node_id) {
                shard_ids.retain(|&id| id != shard_id);
            }
        }

        // 从上下文索引中移除
        {
            let mut index = self.context_index.write().await;
            for shard_ids in index.values_mut() {
                shard_ids.retain(|&id| id != shard_id);
            }
        }

        Ok(())
    }

    /// 获取分片数量
    pub async fn shard_count(&self) -> usize {
        let shards_map = self.shards.read().await;
        shards_map.len()
    }

    /// 获取所有分片
    pub async fn get_all_shards(&self) -> Vec<ContextShard> {
        let shards_map = self.shards.read().await;
        shards_map.values().cloned().collect()
    }

    /// 清空所有分片
    pub async fn clear(&self) {
        let mut shards_map = self.shards.write().await;
        shards_map.clear();

        let mut index = self.context_index.write().await;
        index.clear();

        let mut node_map = self.node_shard_map.write().await;
        node_map.clear();

        let mut next_id = self.next_shard_id.write().await;
        *next_id = 0;
    }
}

impl Default for ContextShardManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_context_shard_creation() {
        let shard = ContextShard::new(
            0,
            "node_1".to_string(),
            (0, 1000),
            b"test_data".to_vec(),
        );

        assert_eq!(shard.shard_id, 0);
        assert_eq!(shard.node_id, "node_1");
        assert_eq!(shard.token_range, (0, 1000));
        assert!(shard.verify_integrity());
    }

    #[tokio::test]
    async fn test_slice_context() {
        let manager = ContextShardManager::new();
        let tokens: Vec<TokenId> = (0..1000).collect();
        let node_ids = vec!["node_1".to_string(), "node_2".to_string()];

        let shards = manager.slice_context("ctx_1", &tokens, 5, &node_ids).await.unwrap();

        assert_eq!(shards.len(), 5);
        
        // 验证分片分配
        for (i, shard) in shards.iter().enumerate() {
            assert_eq!(shard.token_range.0, i * 200);
            assert_eq!(shard.token_range.1, (i + 1) * 200);
            // 轮询分配节点
            assert_eq!(shard.node_id, format!("node_{}", (i % 2) + 1));
        }
    }

    #[tokio::test]
    async fn test_reassemble_context() {
        let manager = ContextShardManager::new();
        let tokens: Vec<TokenId> = (0..1000).collect();
        let node_ids = vec!["node_1".to_string()];

        let shards = manager.slice_context("ctx_1", &tokens, 5, &node_ids).await.unwrap();
        let shard_ids: Vec<u64> = shards.iter().map(|s| s.shard_id).collect();

        let reassembled = manager.reassemble_context(&shard_ids).await.unwrap();
        assert_eq!(reassembled, tokens);
    }

    #[tokio::test]
    async fn test_reassemble_by_context_id() {
        let manager = ContextShardManager::new();
        let tokens: Vec<TokenId> = (0..500).collect();
        let node_ids = vec!["node_1".to_string()];

        manager.slice_context("ctx_test", &tokens, 3, &node_ids).await.unwrap();
        let reassembled = manager.reassemble_by_context_id("ctx_test").await.unwrap();
        
        assert_eq!(reassembled, tokens);
    }

    #[tokio::test]
    async fn test_get_shards_by_node() {
        let manager = ContextShardManager::new();
        let tokens: Vec<TokenId> = (0..1000).collect();
        let node_ids = vec!["node_1".to_string(), "node_2".to_string()];

        let shards = manager.slice_context("ctx_1", &tokens, 4, &node_ids).await.unwrap();

        let node1_shards = manager.get_shards_by_node("node_1").await;
        let node2_shards = manager.get_shards_by_node("node_2").await;

        assert_eq!(node1_shards.len(), 2);
        assert_eq!(node2_shards.len(), 2);

        // 验证分片 ID 正确
        for shard in node1_shards {
            assert!(shards.iter().any(|s| s.shard_id == shard.shard_id));
            assert_eq!(shard.node_id, "node_1");
        }
    }

    #[tokio::test]
    async fn test_delete_shard() {
        let manager = ContextShardManager::new();
        let tokens: Vec<TokenId> = (0..100).collect();
        let node_ids = vec!["node_1".to_string()];

        let shards = manager.slice_context("ctx_1", &tokens, 2, &node_ids).await.unwrap();
        let shard_id = shards[0].shard_id;

        // 删除分片
        manager.delete_shard(shard_id).await.unwrap();

        // 验证分片已删除
        assert!(manager.get_shard(shard_id).await.is_none());
        assert_eq!(manager.shard_count().await, 1);
    }

    #[tokio::test]
    async fn test_shard_integrity_verification() {
        let manager = ContextShardManager::new();
        let tokens: Vec<TokenId> = (0..100).collect();
        let node_ids = vec!["node_1".to_string()];

        let shards = manager.slice_context("ctx_1", &tokens, 2, &node_ids).await.unwrap();

        // 所有分片应该通过完整性验证
        for shard in &shards {
            assert!(shard.verify_integrity());
        }
    }

    #[tokio::test]
    async fn test_large_context_slicing() {
        let manager = ContextShardManager::new();
        let tokens: Vec<TokenId> = (0..100_000).collect(); // 100K tokens
        let node_ids = vec![
            "node_1".to_string(),
            "node_2".to_string(),
            "node_3".to_string(),
        ];

        let shards = manager.slice_context("large_ctx", &tokens, 100, &node_ids).await.unwrap();

        assert_eq!(shards.len(), 100);
        
        // 验证每个分片大约有 1000 个 tokens
        for shard in &shards {
            assert!(shard.token_count() <= 1000);
        }

        // 重新组装
        let shard_ids: Vec<u64> = shards.iter().map(|s| s.shard_id).collect();
        let reassembled = manager.reassemble_context(&shard_ids).await.unwrap();
        assert_eq!(reassembled, tokens);
    }

    #[tokio::test]
    async fn test_clear_all_shards() {
        let manager = ContextShardManager::new();
        let tokens: Vec<TokenId> = (0..100).collect();
        let node_ids = vec!["node_1".to_string()];

        manager.slice_context("ctx_1", &tokens, 2, &node_ids).await.unwrap();
        manager.slice_context("ctx_2", &tokens, 2, &node_ids).await.unwrap();

        assert_eq!(manager.shard_count().await, 4);

        // 清空所有分片
        manager.clear().await;

        assert_eq!(manager.shard_count().await, 0);
    }
}
