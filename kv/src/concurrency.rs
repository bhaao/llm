//! 并发工具模块
//!
//! 提供安全的并发原语，避免死锁和锁超时问题
//!
//! # 锁使用规范
//!
//! 1. **锁顺序规范**: 永远按 L1 → L2 → L3 顺序加锁
//! 2. **锁超时**: 所有锁操作必须有超时机制
//! 3. **避免嵌套锁**: 尽量不在持有锁时获取其他锁
//! 4. **优先使用 Mutex**: 除非读操作远多于写，否则使用 Mutex 而非 RwLock

use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tokio::time::timeout;
use crate::error::{AppError, AppResult};

/// 锁超时默认时间（毫秒）
pub const DEFAULT_LOCK_TIMEOUT_MS: u64 = 5000;

/// 带超时的 Mutex 获取
///
/// # 参数
///
/// * `mutex` - Mutex 引用
/// * `timeout_ms` - 超时时间（毫秒）
/// * `operation` - 操作描述（用于错误消息）
///
/// # 返回
///
/// - `Ok(MutexGuard)` - 成功获取锁
/// - `Err(AppError::LockTimeout)` - 超时
/// - `Err(AppError::Lock)` - 其他错误
pub async fn acquire_mutex_timeout<'a, T>(
    mutex: &'a Mutex<T>,
    timeout_ms: u64,
    operation: &'a str,
) -> AppResult<tokio::sync::MutexGuard<'a, T>> {
    match timeout(Duration::from_millis(timeout_ms), mutex.lock()).await {
        Ok(guard) => Ok(guard),
        Err(_) => Err(AppError::lock_timeout(operation, timeout_ms)),
    }
}

/// 带超时的 RwLock 读锁获取
///
/// # 参数
///
/// * `rwlock` - RwLock 引用
/// * `timeout_ms` - 超时时间（毫秒）
/// * `operation` - 操作描述（用于错误消息）
///
/// # 返回
///
/// - `Ok(RwLockReadGuard)` - 成功获取读锁
/// - `Err(AppError::LockTimeout)` - 超时
/// - `Err(AppError::Lock)` - 其他错误
pub async fn acquire_rwlock_read_timeout<'a, T>(
    rwlock: &'a RwLock<T>,
    timeout_ms: u64,
    operation: &'a str,
) -> AppResult<tokio::sync::RwLockReadGuard<'a, T>> {
    match timeout(Duration::from_millis(timeout_ms), rwlock.read()).await {
        Ok(guard) => Ok(guard),
        Err(_) => Err(AppError::lock_timeout(operation, timeout_ms)),
    }
}

/// 带超时的 RwLock 写锁获取
///
/// # 参数
///
/// * `rwlock` - RwLock 引用
/// * `timeout_ms` - 超时时间（毫秒）
/// * `operation` - 操作描述（用于错误消息）
///
/// # 返回
///
/// - `Ok(RwLockWriteGuard)` - 成功获取写锁
/// - `Err(AppError::LockTimeout)` - 超时
/// - `Err(AppError::Lock)` - 其他错误
pub async fn acquire_rwlock_write_timeout<'a, T>(
    rwlock: &'a RwLock<T>,
    timeout_ms: u64,
    operation: &'a str,
) -> AppResult<tokio::sync::RwLockWriteGuard<'a, T>> {
    match timeout(Duration::from_millis(timeout_ms), rwlock.write()).await {
        Ok(guard) => Ok(guard),
        Err(_) => Err(AppError::lock_timeout(operation, timeout_ms)),
    }
}

/// 安全的同步 Mutex 包装器
///
/// 提供带超时的同步锁获取方法，适用于同步代码
pub struct SafeMutex<T> {
    inner: std::sync::Mutex<T>,
    default_timeout_ms: u64,
}

impl<T> SafeMutex<T> {
    /// 创建新的 SafeMutex
    pub fn new(data: T) -> Self {
        SafeMutex {
            inner: std::sync::Mutex::new(data),
            default_timeout_ms: DEFAULT_LOCK_TIMEOUT_MS,
        }
    }

    /// 创建带自定义超时的 SafeMutex
    pub fn with_timeout(data: T, timeout_ms: u64) -> Self {
        SafeMutex {
            inner: std::sync::Mutex::new(data),
            default_timeout_ms: timeout_ms,
        }
    }

    /// 获取锁（带默认超时）
    pub fn lock(&self) -> AppResult<std::sync::MutexGuard<'_, T>> {
        self.lock_timeout(self.default_timeout_ms)
    }

    /// 获取锁（带自定义超时）
    pub fn lock_timeout(&self, timeout_ms: u64) -> AppResult<std::sync::MutexGuard<'_, T>> {
        // 使用 try_lock 循环检测超时
        let start = std::time::Instant::now();
        loop {
            match self.inner.try_lock() {
                Ok(guard) => return Ok(guard),
                Err(std::sync::TryLockError::Poisoned(poisoned)) => {
                    // 锁中毒但恢复 - 使用 into_inner 获取锁
                    return Ok(std::sync::PoisonError::into_inner(poisoned));
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    if start.elapsed().as_millis() >= timeout_ms as u128 {
                        return Err(AppError::lock_timeout("mutex lock", timeout_ms));
                    }
                    // 短暂休眠后重试
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }

    /// 获取内部数据可变引用（需要可变自引用）
    pub fn get_mut(&mut self) -> &mut T {
        self.inner.get_mut().unwrap()
    }

    /// 消耗 Self 并返回内部数据
    pub fn into_inner(self) -> T {
        self.inner.into_inner().unwrap_or_else(|e| e.into_inner())
    }
}

/// 安全的同步 RwLock 包装器
///
/// 提供带超时的同步锁获取方法，适用于同步代码
pub struct SafeRwLock<T> {
    inner: std::sync::RwLock<T>,
    default_timeout_ms: u64,
}

impl<T> SafeRwLock<T> {
    /// 创建新的 SafeRwLock
    pub fn new(data: T) -> Self {
        SafeRwLock {
            inner: std::sync::RwLock::new(data),
            default_timeout_ms: DEFAULT_LOCK_TIMEOUT_MS,
        }
    }

    /// 创建带自定义超时的 SafeRwLock
    pub fn with_timeout(data: T, timeout_ms: u64) -> Self {
        SafeRwLock {
            inner: std::sync::RwLock::new(data),
            default_timeout_ms: timeout_ms,
        }
    }

    /// 获取读锁（带默认超时）
    pub fn read(&self) -> AppResult<std::sync::RwLockReadGuard<'_, T>> {
        self.read_timeout(self.default_timeout_ms)
    }

    /// 获取读锁（带自定义超时）
    pub fn read_timeout(&self, timeout_ms: u64) -> AppResult<std::sync::RwLockReadGuard<'_, T>> {
        let start = std::time::Instant::now();
        loop {
            match self.inner.try_read() {
                Ok(guard) => return Ok(guard),
                Err(std::sync::TryLockError::WouldBlock) => {
                    if start.elapsed().as_millis() >= timeout_ms as u128 {
                        return Err(AppError::lock_timeout("rwlock read", timeout_ms));
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    return Err(AppError::lock("RwLock poisoned"));
                }
            }
        }
    }

    /// 获取写锁（带默认超时）
    pub fn write(&self) -> AppResult<std::sync::RwLockWriteGuard<'_, T>> {
        self.write_timeout(self.default_timeout_ms)
    }

    /// 获取写锁（带自定义超时）
    pub fn write_timeout(&self, timeout_ms: u64) -> AppResult<std::sync::RwLockWriteGuard<'_, T>> {
        let start = std::time::Instant::now();
        loop {
            match self.inner.try_write() {
                Ok(guard) => return Ok(guard),
                Err(std::sync::TryLockError::WouldBlock) => {
                    if start.elapsed().as_millis() >= timeout_ms as u128 {
                        return Err(AppError::lock_timeout("rwlock write", timeout_ms));
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    return Err(AppError::lock("RwLock poisoned"));
                }
            }
        }
    }

    /// 获取内部数据可变引用
    pub fn get_mut(&mut self) -> &mut T {
        self.inner.get_mut().unwrap()
    }

    /// 消耗 Self 并返回内部数据
    pub fn into_inner(self) -> T {
        self.inner.into_inner().unwrap_or_else(|e| e.into_inner())
    }
}

/// 锁顺序守卫 - 确保按正确顺序获取多个锁
///
/// # 使用示例
///
/// ```ignore
/// use crate::concurrency::{LockOrder, LockOrderGuard};
///
/// // 定义锁顺序：L1 < L2 < L3
/// const LOCK_ORDER: LockOrder = LockOrder::L1;
///
/// // 使用守卫确保按顺序获取锁
/// let guard1 = LockOrderGuard::new(LOCK_ORDER);
/// let l1 = l1_mutex.lock().await?;
///
/// let guard2 = LockOrderGuard::new(LockOrder::L2);
/// let l2 = l2_mutex.lock().await?;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LockOrder {
    /// L1 缓存锁（最高优先级）
    L1 = 0,
    /// L2 磁盘锁
    L2 = 1,
    /// L3 远程存储锁
    L3 = 2,
    /// 其他锁
    Other = 3,
}

impl LockOrder {
    /// 检查锁顺序是否有效
    ///
    /// # 参数
    ///
    /// * `current` - 当前持有的锁
    /// * `next` - 要获取的下一个锁
    ///
    /// # 返回
    ///
    /// - `true` - 顺序有效（next > current）
    /// - `false` - 顺序无效，可能导致死锁
    pub fn is_valid_order(current: LockOrder, next: LockOrder) -> bool {
        current < next
    }

    /// 验证并记录锁获取
    ///
    /// # Panics
    ///
    /// 如果检测到无效锁顺序，在 debug 模式下会 panic
    pub fn acquire(self, current_lock: Option<LockOrder>) {
        if let Some(current) = current_lock {
            if !Self::is_valid_order(current, self) {
                let msg = format!(
                    "检测到无效锁顺序：当前持有 {:?}，尝试获取 {:?}，可能导致死锁",
                    current, self
                );
                #[cfg(debug_assertions)]
                panic!("{}", msg);
                #[cfg(not(debug_assertions))]
                eprintln!("{}", msg);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mutex_timeout() {
        let mutex = Mutex::new(42);
        
        // 正常获取锁
        let guard = acquire_mutex_timeout(&mutex, 1000, "test").await.unwrap();
        assert_eq!(*guard, 42);
        drop(guard);

        // 测试超时
        let _held = mutex.lock().await;
        let result = acquire_mutex_timeout(&mutex, 10, "test_timeout").await;
        assert!(matches!(result, Err(AppError::LockTimeout { .. })));
    }

    #[tokio::test]
    async fn test_rwlock_timeout() {
        let rwlock = RwLock::new(42);
        
        // 正常获取读锁
        let guard = acquire_rwlock_read_timeout(&rwlock, 1000, "test_read").await.unwrap();
        assert_eq!(*guard, 42);
        drop(guard);

        // 正常获取写锁
        let mut guard = acquire_rwlock_write_timeout(&rwlock, 1000, "test_write").await.unwrap();
        *guard = 100;
        drop(guard);

        let guard = acquire_rwlock_read_timeout(&rwlock, 1000, "test_read2").await.unwrap();
        assert_eq!(*guard, 100);
    }

    #[test]
    fn test_safe_mutex() {
        let mutex = SafeMutex::new(42);
        
        let guard = mutex.lock().unwrap();
        assert_eq!(*guard, 42);
        drop(guard);

        let mut guard = mutex.lock().unwrap();
        *guard = 200;
        drop(guard);

        let guard = mutex.lock().unwrap();
        assert_eq!(*guard, 200);
    }

    #[test]
    fn test_safe_rwlock() {
        let rwlock = SafeRwLock::new(42);
        
        let guard = rwlock.read().unwrap();
        assert_eq!(*guard, 42);
        drop(guard);

        let mut guard = rwlock.write().unwrap();
        *guard = 300;
        drop(guard);

        let guard = rwlock.read().unwrap();
        assert_eq!(*guard, 300);
    }

    #[test]
    fn test_lock_order() {
        // 有效顺序
        assert!(LockOrder::is_valid_order(LockOrder::L1, LockOrder::L2));
        assert!(LockOrder::is_valid_order(LockOrder::L2, LockOrder::L3));
        assert!(LockOrder::is_valid_order(LockOrder::L1, LockOrder::L3));

        // 无效顺序
        assert!(!LockOrder::is_valid_order(LockOrder::L2, LockOrder::L1));
        assert!(!LockOrder::is_valid_order(LockOrder::L3, LockOrder::L2));
        assert!(!LockOrder::is_valid_order(LockOrder::L3, LockOrder::L1));
    }
}
