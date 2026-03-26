//! 异步存储后端模块
//!
//! **核心功能**：
//! - 定义统一的异步存储接口
//! - 支持 CPU 内存、磁盘、远程存储等多种后端
//! - 批量异步 IO 操作
//!
//! # 架构设计
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │  TieredStorageManager                   │
//! │         ↓                               │
//! │  AsyncStorageBackend (trait)            │
//! │    ├─ CpuStorageBackend (内存)          │
//! │    ├─ DiskStorageBackend (磁盘)         │
//! │    └─ RemoteStorageBackend (远程)       │
//! └─────────────────────────────────────────┘
//! ```

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use async_trait::async_trait;

/// 异步存储后端 trait
///
/// 所有存储后端必须实现此 trait
#[async_trait]
pub trait AsyncStorageBackend: Send + Sync {
    /// 获取 chunk 数据
    ///
    /// # 参数
    ///
    /// * `chunk_id` - Chunk 唯一标识
    ///
    /// # 返回
    ///
    /// * `Option<Vec<u8>>` - Chunk 数据或 None
    async fn get(&self, chunk_id: &str) -> Option<Vec<u8>>;

    /// 存储 chunk 数据
    ///
    /// # 参数
    ///
    /// * `chunk_id` - Chunk 唯一标识
    /// * `data` - Chunk 数据
    ///
    /// # 返回
    ///
    /// * `Result<(), String>` - 成功或错误
    async fn put(&self, chunk_id: String, data: Vec<u8>) -> Result<(), String>;

    /// 删除 chunk 数据
    ///
    /// # 参数
    ///
    /// * `chunk_id` - Chunk 唯一标识
    ///
    /// # 返回
    ///
    /// * `Result<(), String>` - 成功或错误
    async fn delete(&self, chunk_id: &str) -> Result<(), String>;

    /// 批量获取 chunk 数据
    ///
    /// # 参数
    ///
    /// * `chunk_ids` - Chunk ID 列表
    ///
    /// # 返回
    ///
    /// * `Vec<Option<Vec<u8>>>` - 每个 chunk 的数据
    async fn batch_get(&self, chunk_ids: &[String]) -> Vec<Option<Vec<u8>>> {
        // 默认实现：串行获取
        let mut results = Vec::with_capacity(chunk_ids.len());
        for chunk_id in chunk_ids {
            results.push(self.get(chunk_id).await);
        }
        results
    }

    /// 批量存储 chunk 数据
    ///
    /// # 参数
    ///
    /// * `chunks` - (chunk_id, data) 列表
    ///
    /// # 返回
    ///
    /// * `Result<(), String>` - 成功或错误
    async fn batch_put(&self, chunks: Vec<(String, Vec<u8>)>) -> Result<(), String> {
        // 默认实现：串行存储
        for (chunk_id, data) in chunks {
            self.put(chunk_id, data).await?;
        }
        Ok(())
    }

    /// 获取存储后端类型名称
    fn backend_type(&self) -> &'static str;

    /// 获取存储使用量 (字节)
    async fn storage_usage(&self) -> u64;
}

/// CPU 内存存储后端
///
/// 特点：
/// - 最快访问 (< 1ms)
/// - 容量有限
/// - 不支持持久化
pub struct CpuStorageBackend {
    cache: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl CpuStorageBackend {
    /// 创建新的 CPU 存储后端
    pub fn new() -> Self {
        CpuStorageBackend {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 创建带初始容量的 CPU 存储后端
    pub fn with_capacity(capacity: usize) -> Self {
        CpuStorageBackend {
            cache: Arc::new(RwLock::new(HashMap::with_capacity(capacity))),
        }
    }

    /// 清空缓存
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    /// 获取缓存中的 chunk 数量
    pub async fn len(&self) -> usize {
        let cache = self.cache.read().await;
        cache.len()
    }
}

impl Default for CpuStorageBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AsyncStorageBackend for CpuStorageBackend {
    async fn get(&self, chunk_id: &str) -> Option<Vec<u8>> {
        let cache = self.cache.read().await;
        cache.get(chunk_id).cloned()
    }

    async fn put(&self, chunk_id: String, data: Vec<u8>) -> Result<(), String> {
        let mut cache = self.cache.write().await;
        cache.insert(chunk_id, data);
        Ok(())
    }

    async fn delete(&self, chunk_id: &str) -> Result<(), String> {
        let mut cache = self.cache.write().await;
        cache.remove(chunk_id);
        Ok(())
    }

    async fn batch_get(&self, chunk_ids: &[String]) -> Vec<Option<Vec<u8>>> {
        let cache = self.cache.read().await;
        chunk_ids.iter().map(|id| cache.get(id).cloned()).collect()
    }

    async fn batch_put(&self, chunks: Vec<(String, Vec<u8>)>) -> Result<(), String> {
        let mut cache = self.cache.write().await;
        for (chunk_id, data) in chunks {
            cache.insert(chunk_id, data);
        }
        Ok(())
    }

    fn backend_type(&self) -> &'static str {
        "cpu_memory"
    }

    async fn storage_usage(&self) -> u64 {
        let cache = self.cache.read().await;
        cache.values().map(|data| data.len() as u64).sum()
    }
}

/// 磁盘存储后端
///
/// 特点：
/// - 中等速度 (10-50ms)
/// - 容量大
/// - 支持持久化
pub struct DiskStorageBackend {
    storage_path: PathBuf,
    write_buffer: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl DiskStorageBackend {
    /// 创建新的磁盘存储后端
    ///
    /// # 参数
    ///
    /// * `storage_path` - 存储目录路径
    ///
    /// # 返回
    ///
    /// * `Result<Self, String>` - 成功或错误
    pub fn new<P: AsRef<Path>>(storage_path: P) -> Result<Self, String> {
        let path = storage_path.as_ref().to_path_buf();

        // 创建目录
        std::fs::create_dir_all(&path)
            .map_err(|e| format!("Failed to create storage directory: {}", e))?;

        Ok(DiskStorageBackend {
            storage_path: path,
            write_buffer: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// 获取 chunk 文件路径
    fn chunk_file_path(&self, chunk_id: &str) -> PathBuf {
        // 使用 chunk_id 作为文件名 (SHA256 哈希，安全)
        self.storage_path.join(format!("{}.chunk", chunk_id))
    }

    /// 刷新写缓冲区到磁盘
    pub async fn flush(&self) -> Result<(), String> {
        let buffer = self.write_buffer.write().await;
        for (chunk_id, data) in buffer.iter() {
            let path = self.chunk_file_path(chunk_id);
            tokio::fs::write(&path, data)
                .await
                .map_err(|e| format!("Failed to write to disk: {}", e))?;
        }
        Ok(())
    }

    /// 从磁盘加载所有 chunks 到缓冲区 (启动时调用)
    pub async fn load_all(&self) -> Result<usize, String> {
        let mut count = 0;
        let mut buffer = self.write_buffer.write().await;

        if let Ok(mut entries) = tokio::fs::read_dir(&self.storage_path).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "chunk") {
                    if let Ok(data) = tokio::fs::read(&path).await {
                        if let Some(chunk_id) = path.file_stem()
                            .and_then(|s| s.to_str())
                        {
                            buffer.insert(chunk_id.to_string(), data);
                            count += 1;
                        }
                    }
                }
            }
        }

        Ok(count)
    }

    /// 清空磁盘存储
    pub async fn clear(&self) -> Result<(), String> {
        // 清空缓冲区
        {
            let mut buffer = self.write_buffer.write().await;
            buffer.clear();
        }

        // 删除所有文件
        if let Ok(mut entries) = tokio::fs::read_dir(&self.storage_path).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let _ = tokio::fs::remove_file(entry.path()).await;
            }
        }

        Ok(())
    }
}

#[async_trait]
impl AsyncStorageBackend for DiskStorageBackend {
    async fn get(&self, chunk_id: &str) -> Option<Vec<u8>> {
        // 先查写缓冲区
        {
            let buffer = self.write_buffer.read().await;
            if let Some(data) = buffer.get(chunk_id) {
                return Some(data.clone());
            }
        }

        // 再查磁盘
        let path = self.chunk_file_path(chunk_id);
        if path.exists() {
            if let Ok(data) = tokio::fs::read(&path).await {
                return Some(data);
            }
        }

        None
    }

    async fn put(&self, chunk_id: String, data: Vec<u8>) -> Result<(), String> {
        // 先写入缓冲区
        {
            let mut buffer = self.write_buffer.write().await;
            buffer.insert(chunk_id.clone(), data.clone());
        }

        // 异步写入磁盘
        let path = self.chunk_file_path(&chunk_id);
        tokio::fs::write(&path, &data)
            .await
            .map_err(|e| format!("Failed to write to disk: {}", e))?;

        Ok(())
    }

    async fn delete(&self, chunk_id: &str) -> Result<(), String> {
        // 从缓冲区移除
        {
            let mut buffer = self.write_buffer.write().await;
            buffer.remove(chunk_id);
        }

        // 删除磁盘文件
        let path = self.chunk_file_path(chunk_id);
        if path.exists() {
            tokio::fs::remove_file(&path)
                .await
                .map_err(|e| format!("Failed to delete from disk: {}", e))?;
        }

        Ok(())
    }

    async fn batch_get(&self, chunk_ids: &[String]) -> Vec<Option<Vec<u8>>> {
        // 并发读取
        let mut futures = Vec::new();
        for chunk_id in chunk_ids {
            futures.push(self.get(chunk_id));
        }

        // 顺序执行 (简化实现)
        let mut results = Vec::with_capacity(chunk_ids.len());
        for chunk_id in chunk_ids {
            results.push(self.get(chunk_id).await);
        }
        results
    }

    async fn batch_put(&self, chunks: Vec<(String, Vec<u8>)>) -> Result<(), String> {
        // 先写入缓冲区
        {
            let mut buffer = self.write_buffer.write().await;
            for (chunk_id, data) in &chunks {
                buffer.insert(chunk_id.clone(), data.clone());
            }
        }

        // 并发写入磁盘
        let mut write_futures = Vec::new();
        for (chunk_id, data) in chunks {
            let path = self.chunk_file_path(&chunk_id);
            write_futures.push(tokio::spawn(async move {
                tokio::fs::write(path, data)
                    .await
                    .map_err(|e| format!("Failed to write: {}", e))
            }));
        }

        // 等待所有写入完成
        for future in write_futures {
            future.await.map_err(|e| format!("Write task failed: {}", e))??;
        }

        Ok(())
    }

    fn backend_type(&self) -> &'static str {
        "disk"
    }

    async fn storage_usage(&self) -> u64 {
        let mut total = 0u64;

        // 缓冲区大小
        {
            let buffer = self.write_buffer.read().await;
            total += buffer.values().map(|data| data.len() as u64).sum::<u64>();
        }

        // 磁盘文件大小
        if let Ok(mut entries) = tokio::fs::read_dir(&self.storage_path).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Ok(metadata) = entry.metadata().await {
                    total += metadata.len();
                }
            }
        }

        total
    }
}

/// 存储后端枚举 (用于动态选择)
pub enum StorageBackendType {
    Cpu(CpuStorageBackend),
    Disk(DiskStorageBackend),
}

#[async_trait]
impl AsyncStorageBackend for StorageBackendType {
    async fn get(&self, chunk_id: &str) -> Option<Vec<u8>> {
        match self {
            StorageBackendType::Cpu(backend) => backend.get(chunk_id).await,
            StorageBackendType::Disk(backend) => backend.get(chunk_id).await,
        }
    }

    async fn put(&self, chunk_id: String, data: Vec<u8>) -> Result<(), String> {
        match self {
            StorageBackendType::Cpu(backend) => backend.put(chunk_id, data).await,
            StorageBackendType::Disk(backend) => backend.put(chunk_id, data).await,
        }
    }

    async fn delete(&self, chunk_id: &str) -> Result<(), String> {
        match self {
            StorageBackendType::Cpu(backend) => backend.delete(chunk_id).await,
            StorageBackendType::Disk(backend) => backend.delete(chunk_id).await,
        }
    }

    fn backend_type(&self) -> &'static str {
        match self {
            StorageBackendType::Cpu(_) => "cpu_memory",
            StorageBackendType::Disk(_) => "disk",
        }
    }

    async fn storage_usage(&self) -> u64 {
        match self {
            StorageBackendType::Cpu(backend) => backend.storage_usage().await,
            StorageBackendType::Disk(backend) => backend.storage_usage().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_cpu_storage_backend() {
        let backend = CpuStorageBackend::new();

        // 测试 put/get
        backend.put("chunk_1".to_string(), vec![1, 2, 3]).await.unwrap();
        let data = backend.get("chunk_1").await;
        assert_eq!(data, Some(vec![1, 2, 3]));

        // 测试 delete
        backend.delete("chunk_1").await.unwrap();
        let data = backend.get("chunk_1").await;
        assert_eq!(data, None);

        // 测试 batch_put/batch_get
        let chunks = vec![
            ("chunk_a".to_string(), vec![10, 20]),
            ("chunk_b".to_string(), vec![30, 40]),
        ];
        backend.batch_put(chunks).await.unwrap();

        let results = backend.batch_get(&["chunk_a".to_string(), "chunk_b".to_string()]).await;
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], Some(vec![10, 20]));
        assert_eq!(results[1], Some(vec![30, 40]));

        // 测试 storage_usage
        let usage = backend.storage_usage().await;
        assert_eq!(usage, 4); // 2 + 2 bytes

        // 测试 backend_type
        assert_eq!(backend.backend_type(), "cpu_memory");
    }

    #[tokio::test]
    async fn test_disk_storage_backend() {
        let temp_dir = TempDir::new().unwrap();
        let backend = DiskStorageBackend::new(temp_dir.path()).unwrap();

        // 测试 put/get
        backend.put("chunk_1".to_string(), vec![1, 2, 3]).await.unwrap();
        let data = backend.get("chunk_1").await;
        assert_eq!(data, Some(vec![1, 2, 3]));

        // 测试 flush
        backend.flush().await.unwrap();

        // 测试 delete
        backend.delete("chunk_1").await.unwrap();
        let data = backend.get("chunk_1").await;
        assert_eq!(data, None);

        // 测试 batch_put/batch_get
        let chunks = vec![
            ("chunk_a".to_string(), vec![10, 20]),
            ("chunk_b".to_string(), vec![30, 40]),
        ];
        backend.batch_put(chunks).await.unwrap();

        let results = backend.batch_get(&["chunk_a".to_string(), "chunk_b".to_string()]).await;
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], Some(vec![10, 20]));
        assert_eq!(results[1], Some(vec![30, 40]));

        // 测试 backend_type
        assert_eq!(backend.backend_type(), "disk");
    }

    #[tokio::test]
    async fn test_disk_storage_persistence() {
        let temp_dir = TempDir::new().unwrap();

        // 写入数据
        {
            let backend = DiskStorageBackend::new(temp_dir.path()).unwrap();
            backend.put("persist_chunk".to_string(), vec![100, 200]).await.unwrap();
            backend.flush().await.unwrap();
        }

        // 重新创建 backend，验证数据持久化
        let backend2 = DiskStorageBackend::new(temp_dir.path()).unwrap();
        let data = backend2.get("persist_chunk").await;
        assert_eq!(data, Some(vec![100, 200]));
    }

    #[tokio::test]
    async fn test_disk_storage_clear() {
        let temp_dir = TempDir::new().unwrap();
        let backend = DiskStorageBackend::new(temp_dir.path()).unwrap();

        backend.put("chunk_1".to_string(), vec![1, 2]).await.unwrap();
        backend.put("chunk_2".to_string(), vec![3, 4]).await.unwrap();

        backend.clear().await.unwrap();

        assert_eq!(backend.get("chunk_1").await, None);
        assert_eq!(backend.get("chunk_2").await, None);
    }

    #[tokio::test]
    async fn test_concurrent_cpu_storage() {
        let backend = Arc::new(CpuStorageBackend::new());
        let mut handles = Vec::new();

        // 并发写入
        for i in 0..100 {
            let backend_clone = Arc::clone(&backend);
            let handle = tokio::spawn(async move {
                backend_clone
                    .put(format!("chunk_{}", i), vec![i as u8])
                    .await
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap().unwrap();
        }

        // 验证所有数据
        assert_eq!(backend.len().await, 100);

        for i in 0..100 {
            let data = backend.get(&format!("chunk_{}", i)).await;
            assert_eq!(data, Some(vec![i as u8]));
        }
    }
}
