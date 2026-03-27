//! KV-Cache 集成测试
//!
//! 这些测试验证主项目与 kv-cache crate 的集成

use block_chain_with_context::memory_layer::{MemoryLayerManager, KvProof};
use block_chain_with_context::node_layer::{AccessCredential, AccessType};

fn create_test_credential() -> AccessCredential {
    AccessCredential {
        credential_id: "test_cred".to_string(),
        provider_id: "provider_1".to_string(),
        memory_block_ids: vec!["all".to_string()],
        access_type: AccessType::ReadWrite,
        expires_at: u64::MAX,
        issuer_node_id: "node_1".to_string(),
        signature: "test_signature".to_string(),
        is_revoked: false,
    }
}

#[test]
fn test_kv_cache_integration() {
    // 创建记忆层管理器（内部使用 kv-cache）
    let mut manager = MemoryLayerManager::new("node_1");
    let credential = create_test_credential();

    // 写入 KV 数据（使用 kv-cache 存储）
    manager
        .write_kv("test_key".to_string(), b"test_value".to_vec(), &credential)
        .unwrap();

    // 读取 KV 数据（从 kv-cache 读取）
    let value = manager.read_kv("test_key", &credential).unwrap();
    assert_eq!(value, b"test_value");
}

#[test]
fn test_kv_cache_with_compression() {
    // 测试 kv-cache 的压缩功能
    let mut manager = MemoryLayerManager::new("node_1");
    let credential = create_test_credential();

    // 写入大数据（应该被压缩）
    let large_data = vec![42u8; 10000];
    manager
        .write_kv("large_key".to_string(), large_data.clone(), &credential)
        .unwrap();

    // 读取并验证
    let value = manager.read_kv("large_key", &credential).unwrap();
    assert_eq!(value, large_data);
}

#[test]
fn test_kv_cache_permission_denied() {
    // 测试权限控制
    let mut manager = MemoryLayerManager::new("node_1");
    
    // 创建只读凭证
    let read_only_cred = AccessCredential {
        credential_id: "read_cred".to_string(),
        provider_id: "provider_1".to_string(),
        memory_block_ids: vec!["all".to_string()],
        access_type: AccessType::ReadOnly,
        expires_at: u64::MAX,
        issuer_node_id: "node_1".to_string(),
        signature: "test_signature".to_string(),
        is_revoked: false,
    };

    // 用只读凭证尝试写入应该失败
    let result = manager.write_kv("key".to_string(), b"value".to_vec(), &read_only_cred);
    assert!(result.is_err());
}

#[test]
fn test_kv_cache_hot_cache() {
    // 测试热点缓存功能（kv-cache 内置）
    let mut manager = MemoryLayerManager::new("node_1");
    let credential = create_test_credential();

    // 写入数据
    manager
        .write_kv("hot_key".to_string(), b"hot_value".to_vec(), &credential)
        .unwrap();

    // 多次读取（应该进入热点缓存）
    for _ in 0..15 {
        let value = manager.read_kv("hot_key", &credential);
        assert_eq!(value, Some(b"hot_value".to_vec()));
    }
}

#[test]
fn test_kv_cache_batch_write() {
    // 测试批量写入
    let mut manager = MemoryLayerManager::new("node_1");
    let credential = create_test_credential();

    // 批量写入 100 个 KV
    for i in 0..100 {
        manager
            .write_kv(format!("key_{}", i), format!("value_{}", i).into_bytes(), &credential)
            .unwrap();
    }

    // 随机读取验证
    for i in 0..100 {
        let value = manager.read_kv(&format!("key_{}", i), &credential).unwrap();
        assert_eq!(value, format!("value_{}", i).into_bytes());
    }
}

#[test]
fn test_memory_block_with_kv_cache() {
    // 测试记忆区块与 kv-cache 集成
    let mut manager = MemoryLayerManager::new("node_1");
    let credential = create_test_credential();

    // 写入 KV 到记忆区块
    manager
        .write_kv("block_key".to_string(), b"block_value".to_vec(), &credential)
        .unwrap();

    // 密封区块
    manager.seal_current_block();

    // 验证区块已密封
    let block = manager.get_block(manager.latest_block_index()).unwrap();
    assert!(block.is_sealed());

    // KV 数据仍然可以从 kv-cache 读取
    let value = manager.read_kv("block_key", &credential).unwrap();
    assert_eq!(value, b"block_value");
}

#[test]
fn test_kv_cache_chain_verification() {
    // 测试链验证与 kv-cache 集成
    let mut manager = MemoryLayerManager::new("node_1");
    let credential = create_test_credential();

    // 写入多个区块
    for i in 0..5 {
        manager
            .write_kv(
                format!("key_{}", i),
                format!("value_{}", i).into_bytes(),
                &credential,
            )
            .unwrap();

        // 密封当前区块，强制创建新区块
        if i % 2 == 0 {
            manager.seal_current_block();
        }
    }

    // 验证链完整性
    assert!(manager.verify_chain());
}

#[test]
fn test_kv_cache_replica_management() {
    // 测试副本管理与 kv-cache 集成
    let mut manager = MemoryLayerManager::new("node_1");
    let credential = create_test_credential();

    // 写入 KV
    manager
        .write_kv("replica_key".to_string(), b"replica_value".to_vec(), &credential)
        .unwrap();

    // 添加副本位置
    manager.add_replica(0, "node_2".to_string()).unwrap();
    manager.add_replica(0, "node_3".to_string()).unwrap();

    // 验证副本位置
    let replicas = manager.get_replicas(0).unwrap();
    assert_eq!(replicas.len(), 2);
    assert!(replicas.contains(&"node_2".to_string()));
    assert!(replicas.contains(&"node_3".to_string()));

    // KV 数据仍然可以读取
    let value = manager.read_kv("replica_key", &credential).unwrap();
    assert_eq!(value, b"replica_value");
}

#[test]
fn test_kv_cache_rollback() {
    // 测试回滚与 kv-cache 集成
    let mut manager = MemoryLayerManager::new("node_1");
    let credential = create_test_credential();

    // 写入 KV
    manager
        .write_kv("rollback_key".to_string(), b"rollback_value".to_vec(), &credential)
        .unwrap();

    // 标记回滚
    manager.mark_current_block_as_rolled_back().unwrap();

    // 注意：kv-cache 的数据不会被回滚清空
    // 因为 kv-cache 是独立于记忆区块的存储层
    // 回滚只影响记忆区块的 shards 列表
    let value = manager.read_kv("rollback_key", &credential);
    assert_eq!(value, Some(b"rollback_value".to_vec()));
}

#[test]
fn test_kv_cache_proof_generation() {
    // 测试 KV 证明生成
    let mut manager = MemoryLayerManager::new("node_1");
    let credential = create_test_credential();

    // 写入 KV
    manager
        .write_kv("proof_key".to_string(), b"proof_value".to_vec(), &credential)
        .unwrap();

    // 获取 KV 证明
    let proofs = manager.get_all_kv_proofs();
    assert!(!proofs.is_empty());

    // 验证证明
    for proof in &proofs {
        assert!(!proof.kv_hash.is_empty());
        assert!(!proof.node_id.is_empty());
    }
}

#[tokio::test]
async fn test_async_kv_cache_integration() {
    // 测试异步集成
    use block_chain_with_context::memory_layer::AsyncMemoryLayerManager;

    let manager = AsyncMemoryLayerManager::new("node_1");
    let credential = create_test_credential();

    // 异步写入
    manager
        .write_kv("async_key".to_string(), b"async_value".to_vec(), &credential)
        .await
        .unwrap();

    // 异步读取
    let value = manager.read_kv("async_key", &credential).await.unwrap();
    assert_eq!(value, b"async_value");
}
