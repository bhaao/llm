//! KV Chunk 模块 - Chunk-level 存储
//!
//! **核心功能**：
//! - 将 KV 数据按 chunk_size (默认 256 tokens) 切分
//! - 每个 chunk 有独立哈希和索引
//! - 支持 LRU 淘汰和访问统计
//!
//! # 架构设计
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │  KV 分片 (Block)                        │
//! │  ┌───────────┬───────────┬───────────┐  │
//! │  │  Chunk 0  │  Chunk 1  │  Chunk 2  │  │
//! │  │  256 tok  │  256 tok  │  256 tok  │  │
//! │  │  hash_0   │  hash_1   │  hash_2   │  │
//! │  └───────────┴───────────┴───────────┘  │
//! └─────────────────────────────────────────┘
//! ```

use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// KV Chunk 结构 - 基本存储单元
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvChunk {
    /// Chunk 唯一标识 (hash)
    pub chunk_id: String,
    /// KV 数据 (可能是压缩后的)
    pub kv_data: Vec<u8>,
    /// Token 范围 [start, end)
    pub token_range: (usize, usize),
    /// Chunk 大小 (字节，原始大小)
    pub size_bytes: usize,
    /// 访问计数 (用于 LRU)
    pub access_count: u64,
    /// 最后访问时间戳 (秒)
    pub last_accessed: u64,
    /// 创建时间戳 (秒)
    pub created_at: u64,
    /// 关联的 block_index (可选)
    pub block_index: Option<u64>,
    /// 关联的 shard_index (可选)
    pub shard_index: Option<usize>,
}

impl KvChunk {
    /// 默认 chunk 大小 (tokens)
    pub const DEFAULT_CHUNK_SIZE: usize = 256;

    /// 创建新的 KV Chunk
    ///
    /// # 参数
    ///
    /// * `kv_data` - KV 数据 (字节数组)
    /// * `start_token` - 起始 token 索引
    /// * `end_token` - 结束 token 索引
    ///
    /// # 返回
    ///
    /// * `Self` - 新的 Chunk 实例
    pub fn new(kv_data: Vec<u8>, start_token: usize, end_token: usize) -> Self {
        let size_bytes = kv_data.len();
        let chunk_id = Self::compute_hash(&kv_data);
        let timestamp = Self::current_timestamp();

        KvChunk {
            chunk_id,
            kv_data,
            token_range: (start_token, end_token),
            size_bytes,
            access_count: 0,
            last_accessed: timestamp,
            created_at: timestamp,
            block_index: None,
            shard_index: None,
        }
    }

    /// 创建带 block/shard 引用的 KV Chunk
    ///
    /// # 参数
    ///
    /// * `kv_data` - KV 数据
    /// * `start_token` - 起始 token 索引
    /// * `end_token` - 结束 token 索引
    /// * `block_index` - 关联的 block 索引
    /// * `shard_index` - 关联的 shard 索引
    ///
    /// # 返回
    ///
    /// * `Self` - 新的 Chunk 实例
    pub fn with_location(
        kv_data: Vec<u8>,
        start_token: usize,
        end_token: usize,
        block_index: u64,
        shard_index: usize,
    ) -> Self {
        let mut chunk = Self::new(kv_data, start_token, end_token);
        chunk.block_index = Some(block_index);
        chunk.shard_index = Some(shard_index);
        chunk
    }

    /// 计算数据的 SHA256 哈希
    ///
    /// # 参数
    ///
    /// * `data` - 输入数据
    ///
    /// # 返回
    ///
    /// * `String` - 十六进制哈希字符串
    pub fn compute_hash(data: &[u8]) -> String {
        let hash = Sha256::digest(data);
        format!("{:x}", hash)
    }

    /// 重新计算 chunk_id (当数据被修改时)
    pub fn refresh_chunk_id(&mut self) {
        self.chunk_id = Self::compute_hash(&self.kv_data);
    }

    /// 记录一次访问
    pub fn record_access(&mut self) {
        self.access_count += 1;
        self.last_accessed = Self::current_timestamp();
    }

    /// 获取 token 数量
    pub fn token_count(&self) -> usize {
        self.token_range.1 - self.token_range.0
    }

    /// 判断是否为冷数据
    ///
    /// # 参数
    ///
    /// * `cold_threshold_secs` - 冷数据时间阈值 (秒)
    /// * `cold_threshold_accesses` - 冷数据访问次数阈值
    ///
    /// # 返回
    ///
    /// * `bool` - 是否为冷数据
    pub fn is_cold(&self, cold_threshold_secs: u64, cold_threshold_accesses: u64) -> bool {
        let elapsed = Self::current_timestamp().saturating_sub(self.last_accessed);
        elapsed > cold_threshold_secs && self.access_count < cold_threshold_accesses
    }

    /// 判断是否为热点数据
    ///
    /// # 参数
    ///
    /// * `hot_threshold_accesses` - 热点数据访问次数阈值
    ///
    /// # 返回
    ///
    /// * `bool` - 是否为热点数据
    pub fn is_hot(&self, hot_threshold_accesses: u64) -> bool {
        self.access_count > hot_threshold_accesses
    }

    /// 获取当前时间戳 (秒)
    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    /// 获取压缩后的数据大小
    pub fn compressed_size(&self) -> usize {
        self.kv_data.len()
    }

    /// 获取压缩率 (如果已压缩)
    ///
    /// # 返回
    ///
    /// * `f64` - 压缩率 (压缩后大小/原始大小)
    pub fn compression_ratio(&self) -> f64 {
        if self.size_bytes == 0 {
            1.0
        } else {
            self.compressed_size() as f64 / self.size_bytes as f64
        }
    }
}

/// KV Chunk 分割器 - 将大块 KV 数据切分成 chunks
pub struct KvChunkSplitter {
    /// 每个 chunk 的 token 数量
    chunk_size_tokens: usize,
}

impl KvChunkSplitter {
    /// 创建新的分割器
    ///
    /// # 参数
    ///
    /// * `chunk_size_tokens` - 每个 chunk 的 token 数量
    ///
    /// # 返回
    ///
    /// * `Self` - 新的分割器实例
    pub fn new(chunk_size_tokens: usize) -> Self {
        KvChunkSplitter {
            chunk_size_tokens,
        }
    }

    /// 创建默认分割器 (256 tokens/chunk)
    pub fn default() -> Self {
        KvChunkSplitter::new(KvChunk::DEFAULT_CHUNK_SIZE)
    }

    /// 将 KV 数据分割成 chunks
    ///
    /// # 参数
    ///
    /// * `kv_data` - 完整的 KV 数据
    /// * `total_tokens` - 总 token 数量
    ///
    /// # 返回
    ///
    /// * `Vec<KvChunk>` - Chunk 列表
    pub fn split(&self, kv_data: &[u8], total_tokens: usize) -> Vec<KvChunk> {
        if kv_data.is_empty() || total_tokens == 0 {
            return Vec::new();
        }

        let bytes_per_token = kv_data.len() / total_tokens.max(1);
        let mut chunks = Vec::new();
        let mut offset = 0;
        let mut token_offset = 0;

        while token_offset < total_tokens {
            let chunk_tokens = (total_tokens - token_offset).min(self.chunk_size_tokens);
            let chunk_bytes = if bytes_per_token > 0 {
                chunk_tokens * bytes_per_token
            } else {
                kv_data.len() - offset
            };

            let chunk_end = (offset + chunk_bytes).min(kv_data.len());
            let chunk_data = kv_data[offset..chunk_end].to_vec();

            let chunk = KvChunk::new(
                chunk_data,
                token_offset,
                token_offset + chunk_tokens,
            );

            chunks.push(chunk);

            offset = chunk_end;
            token_offset += chunk_tokens;
        }

        chunks
    }

    /// 合并多个 chunks 为完整的 KV 数据
    ///
    /// # 参数
    ///
    /// * `chunks` - Chunk 列表
    ///
    /// # 返回
    ///
    /// * `Vec<u8>` - 合并后的 KV 数据
    pub fn merge(&self, chunks: &[KvChunk]) -> Vec<u8> {
        if chunks.is_empty() {
            return Vec::new();
        }

        let mut merged = Vec::with_capacity(chunks.iter().map(|c| c.kv_data.len()).sum());
        for chunk in chunks {
            merged.extend_from_slice(&chunk.kv_data);
        }
        merged
    }
}

/// Chunk 访问统计
#[derive(Debug, Clone, Default)]
pub struct ChunkAccessStats {
    /// 总访问次数
    pub total_accesses: u64,
    /// 命中次数
    pub hits: u64,
    /// 未命中次数
    pub misses: u64,
    /// 预取次数
    pub prefetches: u64,
}

impl ChunkAccessStats {
    /// 创建新的统计
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一次命中
    pub fn record_hit(&mut self) {
        self.total_accesses += 1;
        self.hits += 1;
    }

    /// 记录一次未命中
    pub fn record_miss(&mut self) {
        self.total_accesses += 1;
        self.misses += 1;
    }

    /// 记录一次预取
    pub fn record_prefetch(&mut self) {
        self.prefetches += 1;
    }

    /// 获取命中率
    pub fn hit_rate(&self) -> f64 {
        if self.total_accesses == 0 {
            0.0
        } else {
            self.hits as f64 / self.total_accesses as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kv_chunk_creation() {
        let data = vec![1u8, 2, 3, 4, 5];
        let chunk = KvChunk::new(data.clone(), 0, 256);

        assert!(!chunk.chunk_id.is_empty());
        assert_eq!(chunk.kv_data, data);
        assert_eq!(chunk.token_range, (0, 256));
        assert_eq!(chunk.size_bytes, data.len());
        assert_eq!(chunk.access_count, 0);
    }

    #[test]
    fn test_kv_chunk_with_location() {
        let data = vec![1u8, 2, 3];
        let chunk = KvChunk::with_location(data, 0, 100, 42, 0);

        assert_eq!(chunk.block_index, Some(42));
        assert_eq!(chunk.shard_index, Some(0));
        assert_eq!(chunk.token_range, (0, 100));
    }

    #[test]
    fn test_chunk_hash_consistency() {
        let data = vec![1u8, 2, 3, 4, 5];
        let chunk1 = KvChunk::new(data.clone(), 0, 256);
        let chunk2 = KvChunk::new(data.clone(), 0, 256);

        // 相同数据应该有相同哈希
        assert_eq!(chunk1.chunk_id, chunk2.chunk_id);
    }

    #[test]
    fn test_chunk_access_recording() {
        let mut chunk = KvChunk::new(vec![1u8, 2, 3], 0, 100);

        let initial_access = chunk.access_count;
        chunk.record_access();

        assert_eq!(chunk.access_count, initial_access + 1);
    }

    #[test]
    fn test_chunk_temperature判断() {
        let mut chunk = KvChunk::new(vec![1u8, 2, 3], 0, 100);

        // 初始状态
        assert!(!chunk.is_hot(10));
        assert!(!chunk.is_cold(300, 2));

        // 多次访问后变热
        for _ in 0..15 {
            chunk.record_access();
        }
        assert!(chunk.is_hot(10));
    }

    #[test]
    fn test_chunk_splitter() {
        let splitter = KvChunkSplitter::new(256);

        // 创建 1000 tokens 的数据
        let total_tokens = 1000;
        let data = vec![1u8; total_tokens * 4]; // 假设每个 token 4 字节

        let chunks = splitter.split(&data, total_tokens);

        // 应该分成 4 个 chunks (256 + 256 + 256 + 232)
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0].token_count(), 256);
        assert_eq!(chunks[1].token_count(), 256);
        assert_eq!(chunks[2].token_count(), 256);
        assert_eq!(chunks[3].token_count(), 232);
    }

    #[test]
    fn test_chunk_splitter_merge() {
        let splitter = KvChunkSplitter::new(256);
        let total_tokens = 512;
        let data = vec![1u8; total_tokens * 4];

        let chunks = splitter.split(&data, total_tokens);
        let merged = splitter.merge(&chunks);

        assert_eq!(merged.len(), data.len());
        assert_eq!(merged, data);
    }

    #[test]
    fn test_chunk_splitter_empty() {
        let splitter = KvChunkSplitter::default();

        let empty_chunks = splitter.split(&[], 0);
        assert!(empty_chunks.is_empty());

        let merged_empty = splitter.merge(&[]);
        assert!(merged_empty.is_empty());
    }

    #[test]
    fn test_access_stats() {
        let mut stats = ChunkAccessStats::new();

        assert_eq!(stats.hit_rate(), 0.0);

        stats.record_hit();
        stats.record_hit();
        stats.record_miss();

        assert_eq!(stats.total_accesses, 3);
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate() - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_compression_ratio() {
        let mut chunk = KvChunk::new(vec![1u8, 2, 3, 4, 5], 0, 100);
        chunk.size_bytes = 100; // 原始 100 字节

        // 假设压缩后 50 字节
        chunk.kv_data = vec![1u8; 50];

        assert!((chunk.compression_ratio() - 0.5).abs() < 0.01);
    }
}
