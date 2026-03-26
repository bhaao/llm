//! KV Chunk 索引模块 - Bloom Filter 加速查找
//!
//! **核心功能**：
//! - Bloom Filter 快速判断 chunk 是否存在
//! - 精确索引：chunk_id -> (block_index, shard_index)
//! - 批量查询优化
//!
//! # 性能优势
//!
//! | 操作 | 传统 HashMap | Bloom Filter + HashMap |
//! |-----|-------------|------------------------|
//! | 存在性判断 | O(1) | O(1) 更快 |
//! | 批量查询 | O(n) | O(n) 但可过滤 90%+ 不存在项 |
//! | 内存占用 | 高 | 低 |

use bloomfilter::Bloom;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// KV Chunk 索引
///
/// 结合 Bloom Filter 和 HashMap 的优势：
/// - Bloom Filter: O(1) 快速判断不存在
/// - HashMap: 精确查找位置
pub struct KvChunkIndex {
    /// Bloom Filter 快速判断存在性
    bloom: Bloom<String>,
    /// 精确索引：chunk_id -> (block_index, shard_index)
    exact_index: HashMap<String, (u64, usize)>,
    /// 期望的元素数量
    expected_items: usize,
}

impl KvChunkIndex {
    /// 创建新的索引
    ///
    /// # 参数
    ///
    /// * `expected_items` - 期望的 chunk 数量
    /// * `false_positive_rate` - 期望的假阳性率 (0.0-1.0)
    ///
    /// # 返回
    ///
    /// * `Self` - 新的索引实例
    pub fn new(expected_items: usize, false_positive_rate: f64) -> Self {
        // 计算 Bloom Filter 参数
        let estimated_items = expected_items.max(100);
        let fpr = false_positive_rate.clamp(0.001, 0.1);

        KvChunkIndex {
            bloom: Bloom::new_for_fp_rate(estimated_items, fpr),
            exact_index: HashMap::with_capacity(expected_items),
            expected_items,
        }
    }

    /// 创建默认索引 (期望 10000 个 chunks, 假阳性率 1%)
    pub fn default() -> Self {
        Self::new(10000, 0.01)
    }

    /// 添加 chunk 到索引
    ///
    /// # 参数
    ///
    /// * `chunk_id` - Chunk 唯一标识
    /// * `block_index` - Block 索引
    /// * `shard_index` - Shard 索引
    pub fn insert(&mut self, chunk_id: String, block_index: u64, shard_index: usize) {
        self.bloom.set(&chunk_id);
        self.exact_index.insert(chunk_id, (block_index, shard_index));
    }

    /// 快速判断 chunk 是否存在 (可能有假阳性)
    ///
    /// # 参数
    ///
    /// * `chunk_id` - Chunk 唯一标识
    ///
    /// # 返回
    ///
    /// * `bool` - 可能存在 (true) 或 一定不存在 (false)
    pub fn might_contain(&self, chunk_id: &str) -> bool {
        self.bloom.check(&chunk_id.to_string())
    }

    /// 获取精确位置
    ///
    /// # 参数
    ///
    /// * `chunk_id` - Chunk 唯一标识
    ///
    /// # 返回
    ///
    /// * `Option<(u64, usize)>` - (block_index, shard_index) 或 None
    pub fn get_location(&self, chunk_id: &str) -> Option<(u64, usize)> {
        self.exact_index.get(chunk_id).copied()
    }

    /// 批量查询 (Bloom Filter 优势场景)
    ///
    /// # 参数
    ///
    /// * `chunk_ids` - Chunk ID 列表
    ///
    /// # 返回
    ///
    /// * `Vec<bool>` - 每个 chunk 是否可能存在
    pub fn batch_might_contain(&self, chunk_ids: &[String]) -> Vec<bool> {
        chunk_ids.iter().map(|id| self.bloom.check(&id)).collect()
    }

    /// 批量获取精确位置
    ///
    /// # 参数
    ///
    /// * `chunk_ids` - Chunk ID 列表
    ///
    /// # 返回
    ///
    /// * `Vec<Option<(u64, usize)>>` - 每个 chunk 的位置
    pub fn batch_get_location(&self, chunk_ids: &[String]) -> Vec<Option<(u64, usize)>> {
        chunk_ids.iter().map(|id| self.get_location(id)).collect()
    }

    /// 从索引中移除 chunk
    ///
    /// # 参数
    ///
    /// * `chunk_id` - Chunk 唯一标识
    ///
    /// # 返回
    ///
    /// * `bool` - 是否成功移除
    ///
    /// 注意：Bloom Filter 不支持删除，只从 exact_index 移除
    pub fn remove(&mut self, chunk_id: &str) -> bool {
        self.exact_index.remove(chunk_id).is_some()
    }

    /// 获取索引大小
    pub fn len(&self) -> usize {
        self.exact_index.len()
    }

    /// 判断索引是否为空
    pub fn is_empty(&self) -> bool {
        self.exact_index.is_empty()
    }

    /// 清空索引
    pub fn clear(&mut self) {
        self.exact_index.clear();
        // 重新创建 Bloom Filter
        self.bloom = Bloom::new_for_fp_rate(self.expected_items.max(100), 0.01);
    }

    /// 获取所有 chunk IDs
    pub fn all_chunk_ids(&self) -> Vec<String> {
        self.exact_index.keys().cloned().collect()
    }

    /// 获取索引统计信息
    pub fn stats(&self) -> IndexStats {
        IndexStats {
            total_chunks: self.exact_index.len(),
            bloom_filter_size: self.exact_index.len() * 8, // 估算值
            expected_items: self.expected_items,
        }
    }
}

/// 索引统计信息
#[derive(Debug, Clone)]
pub struct IndexStats {
    /// 总 chunk 数量
    pub total_chunks: usize,
    /// Bloom Filter 大小 (位)
    pub bloom_filter_size: usize,
    /// 期望的 chunk 数量
    pub expected_items: usize,
}

/// 线程安全的 Chunk 索引包装器
pub struct ConcurrentKvChunkIndex {
    inner: Arc<RwLock<KvChunkIndex>>,
}

impl ConcurrentKvChunkIndex {
    /// 创建新的并发索引
    pub fn new(expected_items: usize, false_positive_rate: f64) -> Self {
        ConcurrentKvChunkIndex {
            inner: Arc::new(RwLock::new(KvChunkIndex::new(expected_items, false_positive_rate))),
        }
    }

    /// 创建默认并发索引
    pub fn default() -> Self {
        Self::new(10000, 0.01)
    }

    /// 插入 chunk (异步)
    pub async fn insert(&self, chunk_id: String, block_index: u64, shard_index: usize) {
        let mut index = self.inner.write().await;
        index.insert(chunk_id, block_index, shard_index);
    }

    /// 快速判断是否存在 (异步，只读)
    pub async fn might_contain(&self, chunk_id: &str) -> bool {
        let index = self.inner.read().await;
        index.might_contain(chunk_id)
    }

    /// 获取位置 (异步，只读)
    pub async fn get_location(&self, chunk_id: &str) -> Option<(u64, usize)> {
        let index = self.inner.read().await;
        index.get_location(chunk_id)
    }

    /// 批量查询 (异步，只读)
    pub async fn batch_might_contain(&self, chunk_ids: &[String]) -> Vec<bool> {
        let index = self.inner.read().await;
        index.batch_might_contain(chunk_ids)
    }

    /// 批量获取位置 (异步，只读)
    pub async fn batch_get_location(&self, chunk_ids: &[String]) -> Vec<Option<(u64, usize)>> {
        let index = self.inner.read().await;
        index.batch_get_location(chunk_ids)
    }

    /// 移除 chunk (异步)
    pub async fn remove(&self, chunk_id: &str) -> bool {
        let mut index = self.inner.write().await;
        index.remove(chunk_id)
    }

    /// 获取大小 (异步)
    pub async fn len(&self) -> usize {
        let index = self.inner.read().await;
        index.len()
    }

    /// 清空索引 (异步)
    pub async fn clear(&self) {
        let mut index = self.inner.write().await;
        index.clear();
    }
}

impl Clone for ConcurrentKvChunkIndex {
    fn clone(&self) -> Self {
        ConcurrentKvChunkIndex {
            inner: Arc::clone(&self.inner),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_insert_and_get() {
        let mut index = KvChunkIndex::default();

        index.insert("chunk_1".to_string(), 0, 0);
        index.insert("chunk_2".to_string(), 0, 1);

        assert_eq!(index.len(), 2);
        assert_eq!(index.get_location("chunk_1"), Some((0, 0)));
        assert_eq!(index.get_location("chunk_2"), Some((0, 1)));
        assert_eq!(index.get_location("chunk_3"), None);
    }

    #[test]
    fn test_bloom_filter_might_contain() {
        let mut index = KvChunkIndex::new(100, 0.01);

        index.insert("existing_chunk".to_string(), 0, 0);

        // 存在的 chunk 一定返回 true
        assert!(index.might_contain("existing_chunk"));

        // 不存在的 chunk 可能返回 false (大多数情况) 或 true (假阳性)
        // 由于 Bloom Filter 的特性，我们不能确定一定是 false
        // 但假阳性率应该很低 (1%)
    }

    #[test]
    fn test_batch_query() {
        let mut index = KvChunkIndex::new(100, 0.01);

        index.insert("chunk_1".to_string(), 0, 0);
        index.insert("chunk_2".to_string(), 0, 1);
        index.insert("chunk_3".to_string(), 0, 2);

        let query_ids = vec![
            "chunk_1".to_string(),
            "chunk_2".to_string(),
            "nonexistent".to_string(),
        ];

        let results = index.batch_might_contain(&query_ids);
        assert_eq!(results.len(), 3);
        assert!(results[0]); // chunk_1 存在
        assert!(results[1]); // chunk_2 存在
        // results[2] 可能为 true 或 false (假阳性)

        let locations = index.batch_get_location(&query_ids);
        assert_eq!(locations[0], Some((0, 0)));
        assert_eq!(locations[1], Some((0, 1)));
        assert_eq!(locations[2], None);
    }

    #[test]
    fn test_index_remove() {
        let mut index = KvChunkIndex::default();

        index.insert("chunk_1".to_string(), 0, 0);
        assert!(index.remove("chunk_1"));
        assert!(!index.remove("chunk_1")); // 已经移除

        assert_eq!(index.get_location("chunk_1"), None);
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_index_clear() {
        let mut index = KvChunkIndex::default();

        index.insert("chunk_1".to_string(), 0, 0);
        index.insert("chunk_2".to_string(), 0, 1);

        index.clear();

        assert_eq!(index.len(), 0);
        assert!(index.is_empty());
    }

    #[test]
    fn test_index_stats() {
        let mut index = KvChunkIndex::new(1000, 0.01);

        for i in 0..100 {
            index.insert(format!("chunk_{}", i), 0, i);
        }

        let stats = index.stats();
        assert_eq!(stats.total_chunks, 100);
        assert!(stats.bloom_filter_size > 0);
        assert_eq!(stats.expected_items, 1000);
    }

    #[tokio::test]
    async fn test_concurrent_index() {
        let index = ConcurrentKvChunkIndex::default();

        // 并发插入
        let mut handles = Vec::new();
        for i in 0..100 {
            let idx = index.clone();
            let handle = tokio::spawn(async move {
                idx.insert(format!("chunk_{}", i), 0, i).await;
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }

        assert_eq!(index.len().await, 100);

        // 并发查询
        let exists = index.might_contain("chunk_50").await;
        assert!(exists);

        let location = index.get_location("chunk_50").await;
        assert_eq!(location, Some((0, 50)));
    }

    #[tokio::test]
    async fn test_concurrent_batch_query() {
        let index = ConcurrentKvChunkIndex::default();

        // 插入一些数据
        for i in 0..10 {
            index.insert(format!("chunk_{}", i), 0, i).await;
        }

        let query_ids: Vec<String> = (0..10).map(|i| format!("chunk_{}", i)).collect();
        let locations = index.batch_get_location(&query_ids).await;

        assert_eq!(locations.len(), 10);
        for (i, loc) in locations.iter().enumerate() {
            assert_eq!(*loc, Some((0, i)));
        }
    }
}
