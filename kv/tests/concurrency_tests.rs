//! 并发测试 - 验证线程安全和边界条件
//!
//! **测试目标**：
//! - 验证 Arc<KvCacheManager> 的线程安全（DashMap 内部处理并发）
//! - 验证并发读写 KV 的安全性
//! - 100 线程压力测试
//!
//! # 运行测试
//!
//! ```bash
//! cargo test --test concurrency_tests -- --nocapture
//! ```

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use kv_cache::{
        KvCacheManager,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// 测试 KV 缓存的并发写入
    #[test]
    fn test_concurrent_kv_writes() {
        // 使用 Arc<KvCacheManager>，DashMap 内部处理并发，无需额外 RwLock
        let manager: Arc<KvCacheManager> = Arc::new(KvCacheManager::new());

        let mut handles = vec![];

        // 创建 10 个线程，每个线程尝试写入 KV
        for i in 0..10 {
            let mgr: Arc<KvCacheManager> = Arc::clone(&manager);
            let handle = thread::spawn(move || {
                let key = format!("key_{}", i);
                let value = format!("value_{}", i).into_bytes();
                mgr.write_kv(key, value).unwrap();
                // 模拟一些工作
                thread::sleep(Duration::from_millis(10));
            });
            handles.push(handle);
        }

        // 等待所有线程完成
        for handle in handles {
            handle.join().unwrap();
        }

        // 验证所有 KV 都已添加
        assert_eq!(manager.total_kv_count(), 10);
    }

    /// 测试 KV 缓存的并发读写
    #[test]
    fn test_concurrent_kv_read_write() {
        let manager: Arc<KvCacheManager> = Arc::new(KvCacheManager::new());

        // 先写入一些初始数据
        for i in 0..5 {
            let key = format!("key_{}", i);
            let value = format!("value_{}", i).into_bytes();
            manager.write_kv(key, value).unwrap();
        }

        let mut write_handles = vec![];
        let mut read_handles = vec![];

        // 创建 5 个写线程
        for i in 0..5 {
            let mgr: Arc<KvCacheManager> = Arc::clone(&manager);
            let handle = thread::spawn(move || {
                let key = format!("write_key_{}", i);
                let value = format!("write_value_{}", i).into_bytes();
                mgr.write_kv(key, value).unwrap();
                thread::sleep(Duration::from_millis(5));
            });
            write_handles.push(handle);
        }

        // 创建 5 个读线程
        for i in 0..5 {
            let mgr: Arc<KvCacheManager> = Arc::clone(&manager);
            let handle = thread::spawn(move || {
                let key = format!("key_{}", i);
                let value = mgr.read_kv(&key);
                // 可能读到也可能读不到，取决于写入顺序
                assert!(value.is_some());
            });
            read_handles.push(handle);
        }

        // 等待所有线程完成
        for handle in write_handles {
            handle.join().unwrap();
        }
        for handle in read_handles {
            handle.join().unwrap();
        }

        // 验证最终数据
        assert_eq!(manager.total_kv_count(), 10); // 5 个初始 + 5 个新写入
    }

    /// 测试边界条件：大量并发写入（100 线程压力测试）
    #[test]
    fn test_stress_concurrent_writes() {
        let manager: Arc<KvCacheManager> = Arc::new(KvCacheManager::new());

        let mut handles = vec![];

        // 创建 100 个线程
        for i in 0..100 {
            let mgr: Arc<KvCacheManager> = Arc::clone(&manager);
            let handle = thread::spawn(move || {
                let key = format!("key_{}", i);
                let value = format!("value_{}", i).into_bytes();
                mgr.write_kv(key, value).unwrap();
            });
            handles.push(handle);
        }

        // 等待所有线程完成
        for handle in handles {
            handle.join().unwrap();
        }

        // 验证 KV 缓存仍然有效
        assert_eq!(manager.total_kv_count(), 100);
    }

    /// 测试边界条件：快速连续提交
    #[test]
    fn test_rapid_sequential_writes() {
        let manager = KvCacheManager::new();

        // 快速连续写入 50 个 KV
        for i in 0..50 {
            let key = format!("key_{}", i);
            let value = format!("value_{}", i).into_bytes();
            manager.write_kv(key, value).unwrap();
        }

        // 验证 KV 数量
        assert_eq!(manager.total_kv_count(), 50);
    }

    /// 100 线程并发读写 KV 缓存压力测试
    #[test]
    fn test_100_threads_concurrent_read_write() {
        let manager: Arc<KvCacheManager> = Arc::new(KvCacheManager::new());

        // 先添加一些初始数据
        for i in 0..10 {
            let key = format!("init_key_{}", i);
            let value = format!("init_value_{}", i).into_bytes();
            manager.write_kv(key, value).unwrap();
        }

        let success_count = Arc::new(AtomicUsize::new(0));
        let mut handles = vec![];

        // 创建 100 个线程，混合读写操作
        for i in 0..100 {
            let mgr: Arc<KvCacheManager> = Arc::clone(&manager);
            let success_count: Arc<AtomicUsize> = Arc::clone(&success_count);
            let handle = thread::spawn(move || {
                if i % 3 == 0 {
                    // 33% 写操作
                    let key = format!("stress_key_{}", i);
                    let value = format!("stress_value_{}", i).into_bytes();
                    mgr.write_kv(key, value).unwrap();
                    success_count.fetch_add(1, Ordering::SeqCst);
                } else {
                    // 67% 读操作
                    let _count = mgr.total_kv_count();
                    success_count.fetch_add(1, Ordering::SeqCst);
                }
            });
            handles.push(handle);
        }

        // 等待所有线程完成
        for handle in handles {
            handle.join().unwrap();
        }

        // 验证所有操作都成功
        assert_eq!(success_count.load(Ordering::SeqCst), 100);
    }

    /// 测试 KV 完整性验证并发
    #[test]
    fn test_concurrent_kv_verification() {
        let manager: Arc<KvCacheManager> = Arc::new(KvCacheManager::new());

        // 先写入一些数据
        for i in 0..10 {
            let key = format!("key_{}", i);
            let value = format!("value_{}", i).into_bytes();
            manager.write_kv(key, value).unwrap();
        }

        let mut handles = vec![];

        // 创建 10 个线程同时验证 KV 完整性
        for _i in 0..10 {
            let mgr: Arc<KvCacheManager> = Arc::clone(&manager);
            let handle = thread::spawn(move || {
                if let Ok(segment) = mgr.latest_segment() {
                    for shard in segment.shards.values() {
                        assert!(shard.verify_integrity());
                    }
                }
                thread::sleep(Duration::from_millis(5));
            });
            handles.push(handle);
        }

        // 等待所有线程完成
        for handle in handles {
            handle.join().unwrap();
        }
    }

    /// 混合操作压力测试：并发读写
    #[test]
    fn test_mixed_operations_stress() {
        let manager: Arc<KvCacheManager> = Arc::new(KvCacheManager::new());

        let success_count = Arc::new(AtomicUsize::new(0));
        let mut handles = vec![];

        // 创建 100 个线程，50% 写入 50% 读取
        for i in 0..100 {
            let mgr: Arc<KvCacheManager> = Arc::clone(&manager);
            let success_count: Arc<AtomicUsize> = Arc::clone(&success_count);
            let handle = thread::spawn(move || {
                if i % 2 == 0 {
                    // 写入 KV
                    let key = format!("stress_key_{}", i);
                    let value = format!("stress_value_{}", i).into_bytes();
                    if mgr.write_kv(key, value).is_ok() {
                        success_count.fetch_add(1, Ordering::SeqCst);
                    }
                } else {
                    // 读取 KV
                    let _value = mgr.read_kv("some_key");
                    success_count.fetch_add(1, Ordering::SeqCst);
                }
            });
            handles.push(handle);
        }

        // 等待所有线程完成
        for handle in handles {
            handle.join().unwrap();
        }

        // 验证所有操作都成功
        assert_eq!(success_count.load(Ordering::SeqCst), 100);
    }

    /// 测试并发分段访问
    #[test]
    fn test_concurrent_segment_access() {
        let manager: Arc<KvCacheManager> = Arc::new(KvCacheManager::new());

        let mut handles = vec![];

        // 创建 10 个线程，每个线程写入并访问分段
        for i in 0..10 {
            let mgr: Arc<KvCacheManager> = Arc::clone(&manager);
            let handle = thread::spawn(move || {
                let key = format!("key_{}", i);
                let value = format!("value_{}", i).into_bytes();
                mgr.write_kv(key, value).unwrap();
            });
            handles.push(handle);
        }

        // 等待所有线程完成
        for handle in handles {
            handle.join().unwrap();
        }

        // 验证分段数量
        // 由于并发写入，分段数量可能少于 10（多个写入可能合并到同一段）
        assert!(manager.segment_count() >= 1);
    }
}
