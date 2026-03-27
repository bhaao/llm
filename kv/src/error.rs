//! 统一错误类型定义
//!
//! 精简后的错误类型，只保留 KV 缓存核心错误

use thiserror::Error;

/// 应用级错误类型
#[derive(Error, Debug)]
pub enum AppError {
    // ========== KV 缓存错误 ==========
    #[error("KV 缓存错误：{reason}")]
    KvCache { reason: String },

    #[error("KV 存储错误：{reason}")]
    KvStorage { reason: String },

    #[error("KV 未找到：键 {key}")]
    KvNotFound { key: String },

    #[error("KV 分片验证失败：索引 {index}, 原因：{reason}")]
    KvShardValidation { index: u64, reason: String },

    #[error("KV 索引错误：{reason}")]
    KvIndex { reason: String },

    #[error("KV 压缩错误：{reason}")]
    KvCompression { reason: String },

    #[error("KV 预取错误：{reason}")]
    KvPrefetch { reason: String },

    #[error("访问权限不足：{reason}")]
    AccessDenied { reason: String },

    // ========== 配置错误 ==========
    #[error("配置错误：{reason}")]
    Config { reason: String },

    #[error("配置验证失败：字段 {field}, 原因：{reason}")]
    ConfigValidation { field: String, reason: String },

    // ========== 并发/锁错误 ==========
    #[error("锁错误：{reason}")]
    Lock { reason: String },

    #[error("锁超时：操作 {operation}, 超时 {timeout_ms}ms")]
    LockTimeout { operation: String, timeout_ms: u64 },

    #[error("死锁检测到：{reason}")]
    Deadlock { reason: String },

    // ========== 网络/通信错误 ==========
    #[error("网络错误：{reason}")]
    Network { reason: String },

    #[error("RPC 错误：{reason}")]
    Rpc { reason: String },

    #[error("Redis 错误：{reason}")]
    Redis { reason: String },

    // ========== 推理服务错误 ==========
    #[error("推理错误：{reason}")]
    Inference { reason: String },

    #[error("推理超时：{timeout_ms}ms")]
    InferenceTimeout { timeout_ms: u64 },

    // ========== 序列化/反序列化错误 ==========
    #[error("序列化错误：{reason}")]
    Serialization { reason: String },

    // ========== IO 错误 ==========
    #[error("IO 错误：{reason}")]
    Io { reason: String },

    // ========== 其他错误 ==========
    #[error("未实现：{feature}")]
    Unimplemented { feature: String },

    #[error("内部错误：{reason}")]
    Internal { reason: String },
}

// ========== 便捷构造方法 ==========

impl AppError {
    // KV 缓存错误
    pub fn kv_cache(reason: impl Into<String>) -> Self {
        AppError::KvCache {
            reason: reason.into(),
        }
    }

    pub fn kv_storage(reason: impl Into<String>) -> Self {
        AppError::KvStorage {
            reason: reason.into(),
        }
    }

    pub fn kv_not_found(key: impl Into<String>) -> Self {
        AppError::KvNotFound { key: key.into() }
    }

    pub fn kv_shard_validation(index: u64, reason: impl Into<String>) -> Self {
        AppError::KvShardValidation {
            index,
            reason: reason.into(),
        }
    }

    pub fn kv_index(reason: impl Into<String>) -> Self {
        AppError::KvIndex {
            reason: reason.into(),
        }
    }

    pub fn kv_compression(reason: impl Into<String>) -> Self {
        AppError::KvCompression {
            reason: reason.into(),
        }
    }

    pub fn kv_prefetch(reason: impl Into<String>) -> Self {
        AppError::KvPrefetch {
            reason: reason.into(),
        }
    }

    pub fn access_denied(reason: impl Into<String>) -> Self {
        AppError::AccessDenied {
            reason: reason.into(),
        }
    }

    // 配置错误
    pub fn config(reason: impl Into<String>) -> Self {
        AppError::Config {
            reason: reason.into(),
        }
    }

    pub fn config_validation(field: impl Into<String>, reason: impl Into<String>) -> Self {
        AppError::ConfigValidation {
            field: field.into(),
            reason: reason.into(),
        }
    }

    // 锁错误
    pub fn lock(reason: impl Into<String>) -> Self {
        AppError::Lock {
            reason: reason.into(),
        }
    }

    pub fn lock_timeout(operation: impl Into<String>, timeout_ms: u64) -> Self {
        AppError::LockTimeout {
            operation: operation.into(),
            timeout_ms,
        }
    }

    pub fn deadlock(reason: impl Into<String>) -> Self {
        AppError::Deadlock {
            reason: reason.into(),
        }
    }

    // 网络错误
    pub fn network(reason: impl Into<String>) -> Self {
        AppError::Network {
            reason: reason.into(),
        }
    }

    pub fn rpc(reason: impl Into<String>) -> Self {
        AppError::Rpc {
            reason: reason.into(),
        }
    }

    pub fn redis(reason: impl Into<String>) -> Self {
        AppError::Redis {
            reason: reason.into(),
        }
    }

    // 推理错误
    pub fn inference(reason: impl Into<String>) -> Self {
        AppError::Inference {
            reason: reason.into(),
        }
    }

    pub fn inference_timeout(timeout_ms: u64) -> Self {
        AppError::InferenceTimeout { timeout_ms }
    }

    // 序列化错误
    pub fn serialization(reason: impl Into<String>) -> Self {
        AppError::Serialization {
            reason: reason.into(),
        }
    }

    // IO 错误
    pub fn io(reason: impl Into<String>) -> Self {
        AppError::Io {
            reason: reason.into(),
        }
    }

    // 其他错误
    pub fn unimplemented(feature: impl Into<String>) -> Self {
        AppError::Unimplemented {
            feature: feature.into(),
        }
    }

    pub fn internal(reason: impl Into<String>) -> Self {
        AppError::Internal {
            reason: reason.into(),
        }
    }
}

// ========== From 转换实现 ==========

impl From<std::sync::PoisonError<std::sync::MutexGuard<'_, ()>>> for AppError {
    fn from(err: std::sync::PoisonError<std::sync::MutexGuard<'_, ()>>) -> Self {
        AppError::lock(format!("Mutex poisoned: {}", err))
    }
}

impl<T> From<std::sync::TryLockError<T>> for AppError {
    fn from(err: std::sync::TryLockError<T>) -> Self {
        match err {
            std::sync::TryLockError::Poisoned(_) => AppError::lock("Lock poisoned"),
            std::sync::TryLockError::WouldBlock => AppError::lock("Lock would block"),
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::serialization(format!("JSON serialization failed: {}", err))
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::io(format!("IO operation failed: {}", err))
    }
}

impl From<tokio::time::error::Elapsed> for AppError {
    fn from(_err: tokio::time::error::Elapsed) -> Self {
        AppError::lock_timeout("async operation", 0)
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::internal(format!("Internal error: {}", err))
    }
}

// ========== Result 类型别名 ==========

/// 应用级 Result 类型
pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        // 测试 KV 未找到错误
        let err = AppError::kv_not_found("test_key");
        assert!(matches!(err, AppError::KvNotFound { .. }));

        // 测试配置验证错误
        let err = AppError::config_validation("max_memory", "must be positive");
        assert!(matches!(err, AppError::ConfigValidation { .. }));

        // 测试锁超时错误
        let err = AppError::lock_timeout("write", 5000);
        assert!(matches!(err, AppError::LockTimeout { .. }));

        // 测试 KV 存储错误
        let err = AppError::kv_storage("disk full");
        assert!(matches!(err, AppError::KvStorage { .. }));
    }

    #[test]
    fn test_error_display() {
        let err = AppError::kv_not_found("test_key");
        assert_eq!(err.to_string(), "KV 未找到：键 test_key");

        let err = AppError::inference_timeout(30000);
        assert_eq!(err.to_string(), "推理超时：30000ms");

        let err = AppError::kv_storage("disk full");
        assert_eq!(err.to_string(), "KV 存储错误：disk full");
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let app_err: AppError = io_err.into();

        assert!(matches!(app_err, AppError::Io { .. }));
        assert!(app_err.to_string().contains("IO operation failed"));
    }
}
