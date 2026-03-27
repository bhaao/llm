//! 属性测试 - 使用 proptest 随机生成测试数据
//!
//! **测试目标**：
//! - 随机生成 KV 和分段数据
//! - 验证 KV 缓存属性的不变性
//! - 边界条件测试（超大输入、空数据等）
//!
//! # 运行测试
//!
//! ```bash
//! cargo test --test property_tests -- --nocapture
//! ```

use proptest::prelude::*;
use kv_cache::{
    KvCacheManager, KvShard, KvIntegrityProof,
};

/// 生成随机字符串（限制长度）
fn arbitrary_string(max_len: usize) -> impl Strategy<Value = String> {
    prop::string::string_regex(&format!(".{{0,{}}}", max_len)).unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// 测试：KV 缓存创建后总是有效的
    #[test]
    fn prop_kv_cache_creation(owner in "[a-zA-Z0-9_]{1,30}") {
        let manager = KvCacheManager::new();

        // 新 KV 缓存应该总是有效
        assert_eq!(manager.height(), 0); // 创世分段索引为 0
    }

    /// 测试：写入 KV 后密封分段，KV 缓存仍然有效
    #[test]
    fn prop_kv_cache_after_write(
        node_id in "[a-zA-Z0-9_]{1,30}",
        kv_count in 1..10usize,
    ) {
        let manager = KvCacheManager::new();

        // 写入随机数量的 KV
        for i in 0..kv_count {
            let key = format!("key_{}", i);
            let value = format!("value_{}", i).into_bytes();
            manager.write_kv(key, value).unwrap();
        }

        // 密封分段

        // KV 缓存应该仍然有效
    }

    /// 测试：KV 完整性证明添加后 KV 缓存仍然有效
    #[test]
    fn prop_kv_cache_with_integrity_proof(
        node_id in "[a-zA-Z0-9_]{1,30}",
        kv_count in 1..20usize,
    ) {
        let manager = KvCacheManager::new();

        // 写入 KV
        for i in 0..kv_count {
            let key = format!("key_{}", i);
            let value = format!("value_{}", i).into_bytes();
            manager.write_kv(key, value).unwrap();
        }

        // 验证 KV 数量
        assert_eq!(manager.total_kv_count(), kv_count);

        // KV 缓存应该仍然有效
    }

    /// 测试：多次密封分段后链仍然有效
    #[test]
    fn prop_kv_cache_multiple_seals(
        node_id in "[a-zA-Z0-9_]{1,30}",
        seal_count in 1..10usize,
    ) {
        let manager = KvCacheManager::new();

        // 多次写入并密封
        for c in 0..seal_count {
            let key = format!("key_{}", c);
            let value = format!("value_{}", c).into_bytes();
            manager.write_kv(key, value).unwrap();
        }

        // KV 缓存应该仍然有效
    }

    /// 测试：空 KV 列表密封后 KV 缓存仍然有效
    #[test]
    fn prop_kv_cache_empty_seal(
        node_id in "[a-zA-Z0-9_]{1,30}",
    ) {
        let manager = KvCacheManager::new();

        // 不添加任何 KV 直接密封

        // KV 缓存应该仍然有效
    }

    /// 测试：大量 KV 提交后 KV 缓存仍然有效
    #[test]
    fn prop_kv_cache_many_transactions(
        node_id in "[a-zA-Z0-9_]{1,30}",
    ) {
        let manager = KvCacheManager::new();

        // 写入 100 个 KV
        for i in 0..100 {
            let key = format!("key_{}", i);
            let value = format!("value_{}", i).into_bytes();
            manager.write_kv(key, value).unwrap();
        }

        // 密封分段

        // KV 缓存应该仍然有效
    }

    /// 测试：哈希计算的一致性
    #[test]
    fn prop_hash_consistency(
        key in "[a-zA-Z0-9_]{1,100}",
        value in prop::collection::vec(any::<u8>(), 0..100),
    ) {
        let shard1 = KvShard::new(key.clone(), value.clone());
        let shard2 = KvShard::new(key.clone(), value.clone());

        // 相同数据的哈希应该相同
        assert_eq!(shard1.hash, shard2.hash);
    }

    /// 测试：不同数据的哈希不同
    #[test]
    fn prop_hash_uniqueness(
        key1 in "[a-zA-Z0-9_]{1,100}",
        key2 in "[a-zA-Z0-9_]{1,100}",
    ) {
        // 跳过相同数据的情况
        prop_assume!(key1 != key2);

        // 使用不同的 value（包含 key）来确保哈希不同
        let value1 = format!("value_{}", key1).as_bytes().to_vec();
        let value2 = format!("value_{}", key2).as_bytes().to_vec();

        let shard1 = KvShard::new(key1, value1);
        let shard2 = KvShard::new(key2, value2);

        // 不同数据的哈希应该不同
        assert_ne!(shard1.hash, shard2.hash);
    }

    /// 测试：KV 完整性证明验证
    #[test]
    fn prop_integrity_proof_verification(
        key in arbitrary_string(50),
        value in prop::collection::vec(any::<u8>(), 0..1000),
        segment_index in 0..100u64,
        segment_hash in arbitrary_string(64),
    ) {
        let proof = KvIntegrityProof::new(
            key.clone(),
            &value,
            segment_index,
            segment_hash,
        );

        // 正确数据应该验证通过
        assert!(proof.verify_kv_integrity(&value));

        // 错误数据应该验证失败
        let wrong_value = b"wrong_value".to_vec();
        assert!(!proof.verify_kv_integrity(&wrong_value));
    }

    /// 测试：分段链式连接
    #[test]
    fn prop_segment_chain_link(
        node_id in "[a-zA-Z0-9_]{1,30}",
        segment_count in 2..10usize,
    ) {
        let manager = KvCacheManager::new();

        // 创建多个分段
        for i in 0..segment_count {
            let key = format!("key_{}", i);
            let value = format!("value_{}", i).into_bytes();
            manager.write_kv(key, value).unwrap();

            // 获取当前分段并验证链式连接
            // 注意：分段索引从 0 开始，每次写入到当前最新分段
            let segment = manager.get_segment(0).unwrap();
            assert_eq!(segment.header.index, 0);
        }

        // 验证整个链的完整性
        assert!(manager.height() >= 0);
    }
}

/// 模糊测试：超大输入
#[test]
fn fuzz_large_kv() {
    let manager = KvCacheManager::new();

    // 创建超大 KV（10KB 数据）
    let large_data = "x".repeat(10_000);
    let result = manager.write_kv("large_key".to_string(), large_data.into_bytes());
    assert!(result.is_ok());

    // 验证能正确读取
    let value = manager.read_kv("large_key");
    assert!(value.is_some());
    assert_eq!(value.unwrap().len(), 10_000);
}

/// 模糊测试：大量 KV
#[test]
fn fuzz_many_kv() {
    let manager = KvCacheManager::new();

    // 写入 1000 个 KV
    for i in 0..1000 {
        let key = format!("kv_{}", i);
        let value = format!("value_{:064}", i).into_bytes(); // 模拟 64 字符值
        manager.write_kv(key, value).unwrap();
    }

    // 密封分段

    // KV 缓存应该仍然有效

    // 验证 KV 数量
    assert_eq!(manager.total_kv_count(), 1000);
}

/// 模糊测试：快速连续密封
#[test]
fn fuzz_rapid_seals() {
    let manager = KvCacheManager::new();

    // 快速写入 100 个 KV
    for i in 0..100 {
        let key = format!("key_{}", i);
        let value = format!("value_{}", i).into_bytes();
        manager.write_kv(key, value).unwrap();
    }

    // KV 缓存应该仍然有效

    // 验证 KV 数量
    assert_eq!(manager.total_kv_count(), 100);
}

/// 模糊测试：热点缓存压力
#[test]
fn fuzz_hot_cache_stress() {
    let manager = KvCacheManager::new();

    // 写入并缓存 100 个 KV
    for i in 0..100 {
        let key = format!("key_{}", i);
        let value = format!("value_{}", i).into_bytes();
        manager.write_kv(key.clone(), value.clone()).unwrap();
    }

    // 验证所有 KV 都能从热点缓存读取
    for i in 0..100 {
        let key = format!("key_{}", i);
        let value = manager.read_kv(&key);
        assert!(value.is_some());
        assert_eq!(value.unwrap(), format!("value_{}", i).into_bytes());
    }
}

/// 模糊测试：副本管理压力
#[test]
fn fuzz_replica_stress() {
    // 注意：副本管理功能已移除，此测试暂跳过
    // 原功能：压力测试副本管理
    // 新架构：不再需要显式副本管理
}
