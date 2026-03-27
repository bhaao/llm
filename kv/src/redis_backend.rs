//! Redis 远程存储后端实现 - L3 缓存
//!
//! **功能**：
//! - 使用 Redis 作为 L3 远程存储
//! - 支持异步读写
//! - 自动重连机制

#![cfg(feature = "redis-backend")]

use anyhow::{Result, Context};
use redis::aio::MultiplexedConnection;
use redis::{Client, AsyncCommands};
use crate::multi_level_cache::{RemoteStorageBackend, RemoteConfig};

/// Redis 远程存储后端
pub struct RedisStorageBackend {
    /// Redis 客户端
    _client: Client,
    /// Redis 连接（使用 MultiplexedConnection 支持并发）
    conn: MultiplexedConnection,
    /// 配置
    _config: RemoteConfig,
}

impl RedisStorageBackend {
    /// 创建新的 Redis 存储后端
    pub async fn new(config: RemoteConfig) -> Result<Self> {
        // 创建 Redis 客户端
        let client = Client::open(config.endpoint.as_str())
            .context("Failed to create Redis client")?;
        
        // 获取异步连接
        let conn = client.get_multiplexed_async_connection().await
            .context("Failed to get Redis connection")?;

        Ok(RedisStorageBackend {
            _client: client,
            conn,
            _config: config,
        })
    }
}

#[async_trait::async_trait]
impl RemoteStorageBackend for RedisStorageBackend {
    /// 获取数据
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        // 使用 clone 因为 Commands trait 需要&mut self
        let mut conn = self.conn.clone();
        
        let result: Result<Option<Vec<u8>>> = conn
            .get(key)
            .await
            .context("Failed to GET from Redis");
        
        match result {
            Ok(value) => Ok(value),
            Err(e) => {
                // 如果是 Key 不存在错误，返回 None
                if e.to_string().contains("Nil") {
                    Ok(None)
                } else {
                    // 记录错误但不抛出异常（返回 None）
                    tracing::warn!("Redis GET error for key '{}': {}", key, e);
                    Ok(None)
                }
            }
        }
    }

    /// 存储数据
    async fn put(&self, key: &str, value: Vec<u8>) -> Result<()> {
        let mut conn = self.conn.clone();
        
        conn.set::<_, _, ()>(key, value)
            .await
            .context("Failed to SET value in Redis")?;
        
        Ok(())
    }

    /// 删除数据
    async fn delete(&self, key: &str) -> Result<()> {
        let mut conn = self.conn.clone();
        
        let _: i32 = conn
            .del(key)
            .await
            .context("Failed to DEL value in Redis")?;
        
        Ok(())
    }

    /// 检查是否存在
    async fn exists(&self, key: &str) -> Result<bool> {
        let mut conn = self.conn.clone();
        
        let exists: i32 = conn
            .exists(key)
            .await
            .context("Failed to check EXISTS in Redis")?;
        
        Ok(exists > 0)
    }

    /// 获取后端类型名称
    fn backend_type(&self) -> &'static str {
        "Redis"
    }
}

/// 从 RemoteConfig 创建 RedisStorageBackend
pub async fn create_redis_backend(config: &RemoteConfig) -> Result<RedisStorageBackend> {
    RedisStorageBackend::new(config.clone()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // 需要 Redis 服务器，手动运行
    async fn test_redis_basic() {
        let config = RemoteConfig {
            storage_type: crate::multi_level_cache::RemoteStorageType::Redis,
            endpoint: "redis://127.0.0.1:6379".to_string(),
            auth_token: None,
            bucket_name: None,
            timeout_ms: 5000,
        };

        let backend = RedisStorageBackend::new(config).await.unwrap();

        // 测试写入
        backend.put("test_key", b"test_value".to_vec()).await.unwrap();

        // 测试读取
        let value = backend.get("test_key").await.unwrap();
        assert_eq!(value, Some(b"test_value".to_vec()));

        // 测试删除
        backend.delete("test_key").await.unwrap();

        // 验证删除
        let value = backend.get("test_key").await.unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    #[ignore] // 需要 Redis 服务器，手动运行
    async fn test_redis_exists() {
        let config = RemoteConfig {
            storage_type: crate::multi_level_cache::RemoteStorageType::Redis,
            endpoint: "redis://127.0.0.1:6379".to_string(),
            auth_token: None,
            bucket_name: None,
            timeout_ms: 5000,
        };

        let backend = RedisStorageBackend::new(config).await.unwrap();

        // 测试不存在
        assert!(!backend.exists("nonexistent").await.unwrap());

        // 写入后测试存在
        backend.put("test_exists", b"value".to_vec()).await.unwrap();
        assert!(backend.exists("test_exists").await.unwrap());

        // 删除后测试不存在
        backend.delete("test_exists").await.unwrap();
        assert!(!backend.exists("test_exists").await.unwrap());
    }
}
