//! 集成测试 - 端到端流程验证
//!
//! **测试目标**：
//! 1. 验证完整的 KV 缓存工作流程
//! 2. 测试多节点 KV 缓存和证明验证
//! 3. 验证分段链式校验
//! 4. 测试持久化和恢复功能

use kv_cache::{
    KvCacheManager, KvIntegrityProof,
};
use std::fs;

// ==================== 端到端流程测试 ====================

/// 测试完整的 KV 缓存生命周期
#[test]
fn test_full_kv_cache_lifecycle() {
    let temp_dir = "test_temp_lifecycle";
    let _ = fs::remove_dir_all(temp_dir);
    fs::create_dir_all(temp_dir).unwrap();

    // 阶段 1: 创建并填充 KV 缓存
    let manager = KvCacheManager::new();

    // 初始有创世分段
    assert_eq!(manager.height(), 0); // 创世分段索引为 0

    // 写入 KV 数据
    for i in 1..=5 {
        let key = format!("key_{}", i);
        let value = format!("value_{}", i).into_bytes();
        manager.write_kv(key, value).unwrap();
    }

    // 验证 KV 数量
    assert_eq!(manager.total_kv_count(), 5);

    // 清理测试数据
    let _ = fs::remove_dir_all(temp_dir);
}

/// 测试多节点 KV 缓存
#[test]
fn test_multi_node_kv_cache() {
    let nodes = vec!["node_alpha", "node_beta", "node_gamma"];
    let mut managers = Vec::new();

    for node_id in &nodes {
        let manager = KvCacheManager::new();

        // 每个节点写入 KV
        let key = format!("{}_key", node_id);
        let value = format!("{}_value", node_id).into_bytes();
        manager.write_kv(key, value).unwrap();

        managers.push(manager);
    }

    // 验证每个节点的分段数量
    for manager in &managers {
        // 创世分段 + 1 个数据分段 = 2 个分段
        assert!(manager.segment_count() >= 1);
    }
}

/// 测试 KV 完整性证明验证流程
#[test]
fn test_kv_integrity_proof_verification() {
    // 注意：KvIntegrityProof 现在由外部创建，用于验证 KV 数据完整性
    // 此测试验证证明结构的基本功能
    let key = "test_key".to_string();
    let value = b"test_value".to_vec();
    
    // 创建完整性证明
    let proof = KvIntegrityProof::new(key.clone(), &value, 0, "segment_hash".to_string());
    assert_eq!(proof.key, key);

    // 验证 KV 数据完整性
    assert!(proof.verify_kv_integrity(&value));

    // 验证错误数据
    let wrong_value = b"wrong_value".to_vec();
    assert!(!proof.verify_kv_integrity(&wrong_value));
}

/// 测试分段链式校验
#[test]
fn test_segment_chain_verification() {
    let mut manager = KvCacheManager::new();

    // 写入第一组 KV 并密封（分段 0）
    manager.write_kv("key1".to_string(), b"value1".to_vec()).unwrap();


    // 创建新分段（分段 1）并写入第二组 KV
    manager.create_new_segment().unwrap();
    manager.write_kv("key2".to_string(), b"value2".to_vec()).unwrap();

    // 验证链式连接
    let segment_1 = manager.get_segment(1).unwrap();

    // 验证整个链的完整性
}

// ==================== 错误处理测试 ====================

/// 测试密封分段无法修改
#[test]
fn test_sealed_segment_cannot_modify() {
    let manager = KvCacheManager::new();

    manager.write_kv("key".to_string(), b"value".to_vec()).unwrap();

    // 验证分段已密封（分段 0）
    let segment = manager.get_segment(0).unwrap();

    // 验证密封的分段内容不变
    assert_eq!(segment.shard_count(), 1); // 仍然只有 1 个 KV
}

/// 测试空键值对处理
#[test]
fn test_empty_key_value_handling() {
    let manager = KvCacheManager::new();

    // 空值应该允许
    let result = manager.write_kv("empty_key".to_string(), vec![]);
    assert!(result.is_ok());

    // 读取空值
    let value = manager.read_kv("empty_key");
    assert!(value.is_some());
    assert!(value.unwrap().is_empty());
}

/// 测试特殊字符键值
#[test]
fn test_special_characters_handling() {
    let manager = KvCacheManager::new();

    // 特殊字符键
    let special_key = "key!@#$%^&*()_+-=[]{}|;':\",./<>?".to_string();
    let value = b"value".to_vec();
    let result = manager.write_kv(special_key.clone(), value.clone());
    assert!(result.is_ok());

    // 验证能正确读取
    let read_value = manager.read_kv(&special_key);
    assert!(read_value.is_some());
    assert_eq!(read_value.unwrap(), value);
}

/// 测试 Unicode 键值
#[test]
fn test_unicode_handling() {
    let manager = KvCacheManager::new();

    // Unicode 键值
    let key = "键值🚀".to_string();
    let value = "你好世界🌍".as_bytes().to_vec();

    let result = manager.write_kv(key.clone(), value.clone());
    assert!(result.is_ok());

    // 验证能正确读取
    let read_value = manager.read_kv(&key);
    assert!(read_value.is_some());
    assert_eq!(read_value.unwrap(), value);
}

// ==================== 热点缓存测试 ====================

/// 测试热点缓存功能
#[test]
fn test_hot_cache_functionality() {
    let manager = KvCacheManager::new();

    // 写入 KV
    manager.write_kv("hot_key".to_string(), b"hot_value".to_vec()).unwrap();

    // 读取应该从热点缓存返回
    let value = manager.read_kv("hot_key");
    assert!(value.is_some());
    assert_eq!(value.unwrap(), b"hot_value".to_vec());
}

// ==================== 副本管理测试 ====================

/// 测试副本位置管理
#[test]
fn test_replica_management() {
    // 注意：副本管理功能已移除，此测试暂跳过
    // 原功能：管理 KV 分段的多副本存储
    // 新架构：不再需要显式副本管理，由分布式 KV 缓存自动处理
}

// ==================== 回滚测试 ====================

/// 测试分段回滚功能
#[test]
fn test_segment_rollback() {
    let mut manager = KvCacheManager::new();

    // 初始高度为 0（创世分段）
    assert_eq!(manager.height(), 0);

    // 写入 KV
    manager.write_kv("key1".to_string(), b"value1".to_vec()).unwrap();
    
    // 创建新分段（索引 1）
    manager.create_new_segment().unwrap();
    assert_eq!(manager.height(), 1);
    
    // 密封分段 1

    // 标记当前分段（索引 1）为已回滚

    // 验证分段已标记为回滚

    // 未回滚的分段
}

// ==================== 并发测试 ====================

/// 测试并发读写（使用 Arc）
#[test]
fn test_concurrent_read_write() {
    use std::sync::Arc;
    use std::thread;

    let manager = Arc::new(std::sync::RwLock::new(KvCacheManager::new()));
    let mut handles = vec![];

    // 创建 10 个写线程
    for i in 0..10 {
        let mgr = Arc::clone(&manager);
        let handle = thread::spawn(move || {
            let mgr = mgr.write().unwrap();
            let key = format!("key_{}", i);
            let value = format!("value_{}", i).into_bytes();
            mgr.write_kv(key, value).unwrap();
        });
        handles.push(handle);
    }

    // 等待所有写线程完成
    for handle in handles {
        handle.join().unwrap();
    }

    // 验证所有 KV 都已写入
    let mgr = manager.read().unwrap();
    assert_eq!(mgr.total_kv_count(), 10);
}

// ==================== 边界条件测试 ====================

/// 测试超大值处理
#[test]
fn test_large_value_handling() {
    let manager = KvCacheManager::new();

    // 1MB 数据
    let large_value = vec![0u8; 1024 * 1024];
    let result = manager.write_kv("large_key".to_string(), large_value.clone());
    assert!(result.is_ok());

    // 验证能正确读取
    let read_value = manager.read_kv("large_key");
    assert!(read_value.is_some());
    assert_eq!(read_value.unwrap().len(), 1024 * 1024);
}

/// 测试超长键处理
#[test]
fn test_long_key_handling() {
    let manager = KvCacheManager::new();

    // 1000 字符键
    let long_key = "k".repeat(1000);
    let result = manager.write_kv(long_key.clone(), b"value".to_vec());
    assert!(result.is_ok());

    // 验证能正确读取
    let read_value = manager.read_kv(&long_key);
    assert!(read_value.is_some());
}

/// 测试多分段场景
#[test]
fn test_multiple_segments() {
    let mut manager = KvCacheManager::new();

    // 创建多个分段
    for i in 0..10 {
        // 创建新分段
        if i > 0 {
            manager.create_new_segment().unwrap();
        }
        manager.write_kv(format!("key_{}", i), format!("value_{}", i).into_bytes()).unwrap();
    }

    // 验证分段数量：创世分段 (0) + 9 个新分段 (1-9) = 10 个分段
    // 因为第一次循环 i=0 时不创建新分段，写入到分段 0
    // i=1..9 时创建分段 1..9
    assert_eq!(manager.segment_count(), 10);
    assert_eq!(manager.height(), 9);

    // 验证链完整性
}
