//! 属性测试 - 使用 proptest 随机生成测试数据
//!
//! **测试目标**：
//! - 随机生成区块和交易数据
//! - 验证区块链属性的不变性
//! - 边界条件测试（超大输入、空数据等）
//!
//! # 运行测试
//!
//! ```bash
//! cargo test --test property_tests -- --nocapture
//! ```

use proptest::prelude::*;
use block_chain_with_context::{
    Blockchain, BlockchainConfig, Transaction, TransactionType, 
    TransactionPayload, BlockMetadata, KvCacheProof, Verifiable,
};

/// 生成随机交易
fn arbitrary_transaction() -> impl Strategy<Value = Transaction> {
    (
        any::<String>(),
        any::<String>(),
        prop_oneof![
            Just(TransactionType::Transfer),
            Just(TransactionType::Internal),
            Just(TransactionType::InferenceResponse),
        ],
    ).prop_map(|(from, to, tx_type)| {
        Transaction::new(from, to, tx_type, TransactionPayload::None)
    })
}

/// 生成随机字符串（限制长度）
fn arbitrary_string(max_len: usize) -> impl Strategy<Value = String> {
    prop::string::string_regex(&format!(".{{0,{}}}", max_len)).unwrap()
}

/// 生成随机 KV 存证
fn arbitrary_kv_proof() -> impl Strategy<Value = KvCacheProof> {
    (
        arbitrary_string(50),
        arbitrary_string(64),  // 哈希通常是 64 字符
        arbitrary_string(30),
        0..10000u64,
    ).prop_map(|(kv_id, hash, node_id, size)| {
        KvCacheProof::new(kv_id, hash, node_id, size)
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// 测试：区块链创建后总是有效的
    #[test]
    fn prop_blockchain_creation(owner in "[a-zA-Z0-9_]{1,30}") {
        let blockchain = Blockchain::new(owner);
        
        // 新区块链应该总是有效
        assert!(blockchain.verify_chain());
        assert_eq!(blockchain.height(), 1); // 创世区块
    }

    /// 测试：添加交易后提交区块，区块链仍然有效
    #[test]
    fn prop_blockchain_after_commit(
        owner in "[a-zA-Z0-9_]{1,30}",
        node_id in "[a-zA-Z0-9_]{1,30}",
        tx_count in 1..10usize,
    ) {
        let mut blockchain = Blockchain::with_config(
            owner.to_string(),
            BlockchainConfig::default(),
        );
        
        let _ = blockchain.register_node(node_id.to_string());
        
        // 添加随机数量的交易
        for i in 0..tx_count {
            let tx = Transaction::new(
                format!("user_{}", i),
                format!("assistant_{}", i),
                TransactionType::Transfer,
                TransactionPayload::None,
            );
            blockchain.add_pending_transaction(tx);
        }
        
        // 提交区块
        let metadata = BlockMetadata::default();
        let _ = blockchain.commit_inference(metadata, node_id.to_string());
        
        // 区块链应该仍然有效
        assert!(blockchain.verify_chain());
    }

    /// 测试：KV 存证添加后区块链仍然有效
    #[test]
    fn prop_blockchain_with_kv_proof(
        owner in "[a-zA-Z0-9_]{1,30}",
        node_id in "[a-zA-Z0-9_]{1,30}",
        kv_count in 1..20usize,
    ) {
        let mut blockchain = Blockchain::with_config(
            owner.to_string(),
            BlockchainConfig::default(),
        );
        
        let _ = blockchain.register_node(node_id.to_string());
        
        // 添加 KV 存证
        for i in 0..kv_count {
            let kv_proof = KvCacheProof::new(
                format!("kv_{}", i),
                format!("hash_{}", i),
                node_id.to_string(),
                1024 + i as u64,
            );
            blockchain.add_kv_proof(kv_proof);
        }
        
        // 提交区块
        let metadata = BlockMetadata::default();
        let _ = blockchain.commit_inference(metadata, node_id.to_string());
        
        // 区块链应该仍然有效
        assert!(blockchain.verify_chain());
    }

    /// 测试：多次提交区块后链仍然有效
    #[test]
    fn prop_blockchain_multiple_commits(
        owner in "[a-zA-Z0-9_]{1,30}",
        node_id in "[a-zA-Z0-9_]{1,30}",
        commit_count in 1..10usize,
    ) {
        let mut blockchain = Blockchain::with_config(
            owner.to_string(),
            BlockchainConfig::default(),
        );
        
        let _ = blockchain.register_node(node_id.to_string());
        
        // 多次提交区块
        for c in 0..commit_count {
            let tx = Transaction::new(
                format!("user_{}", c),
                format!("assistant_{}", c),
                TransactionType::Transfer,
                TransactionPayload::None,
            );
            blockchain.add_pending_transaction(tx);
            
            let metadata = BlockMetadata::default();
            let _ = blockchain.commit_inference(metadata, node_id.to_string());
        }
        
        // 区块链应该仍然有效
        assert!(blockchain.verify_chain());
    }

    /// 测试：空交易列表提交后区块链仍然有效
    #[test]
    fn prop_blockchain_empty_commit(
        owner in "[a-zA-Z0-9_]{1,30}",
        node_id in "[a-zA-Z0-9_]{1,30}",
    ) {
        let mut blockchain = Blockchain::with_config(
            owner.to_string(),
            BlockchainConfig::default(),
        );
        
        let _ = blockchain.register_node(node_id.to_string());
        
        // 不添加任何交易直接提交
        let metadata = BlockMetadata::default();
        let _ = blockchain.commit_inference(metadata, node_id.to_string());
        
        // 区块链应该仍然有效
        assert!(blockchain.verify_chain());
    }

    /// 测试：大量交易提交后区块链仍然有效
    #[test]
    fn prop_blockchain_many_transactions(
        owner in "[a-zA-Z0-9_]{1,30}",
        node_id in "[a-zA-Z0-9_]{1,30}",
    ) {
        let mut blockchain = Blockchain::with_config(
            owner.to_string(),
            BlockchainConfig::default(),
        );
        
        let _ = blockchain.register_node(node_id.to_string());
        
        // 添加 100 笔交易
        for i in 0..100 {
            let tx = Transaction::new(
                format!("user_{}", i),
                format!("assistant_{}", i),
                TransactionType::Transfer,
                TransactionPayload::None,
            );
            blockchain.add_pending_transaction(tx);
        }
        
        // 提交区块
        let metadata = BlockMetadata::default();
        let _ = blockchain.commit_inference(metadata, node_id.to_string());
        
        // 区块链应该仍然有效
        assert!(blockchain.verify_chain());
    }

    /// 测试：哈希计算的一致性
    #[test]
    fn prop_hash_consistency(
        data in "[a-zA-Z0-9_]{1,100}",
    ) {
        use block_chain_with_context::traits::Hashable;
        
        let tx1 = Transaction::new(
            data.clone(),
            "recipient".to_string(),
            TransactionType::Transfer,
            TransactionPayload::None,
        );
        
        let tx2 = Transaction::new(
            data.clone(),
            "recipient".to_string(),
            TransactionType::Transfer,
            TransactionPayload::None,
        );
        
        // 相同数据的哈希应该相同
        assert_eq!(tx1.hash(), tx2.hash());
    }

    /// 测试：不同数据的哈希不同
    #[test]
    fn prop_hash_uniqueness(
        data1 in "[a-zA-Z0-9_]{1,100}",
        data2 in "[a-zA-Z0-9_]{1,100}",
    ) {
        use block_chain_with_context::traits::Hashable;
        
        // 跳过相同数据的情况
        prop_assume!(data1 != data2);
        
        let tx1 = Transaction::new(
            data1,
            "recipient".to_string(),
            TransactionType::Transfer,
            TransactionPayload::None,
        );
        
        let tx2 = Transaction::new(
            data2,
            "recipient".to_string(),
            TransactionType::Transfer,
            TransactionPayload::None,
        );
        
        // 不同数据的哈希应该不同
        assert_ne!(tx1.hash(), tx2.hash());
    }
}

/// 模糊测试：超大输入
#[test]
fn fuzz_large_transaction() {
    let mut blockchain = Blockchain::with_config(
        "test_owner".to_string(),
        BlockchainConfig::default(),
    );
    
    let _ = blockchain.register_node("test_node".to_string());
    
    // 创建超大交易（10KB 数据）
    let large_data = "x".repeat(10_000);
    let tx = Transaction::new(
        large_data.clone(),
        large_data.clone(),
        TransactionType::Transfer,
        TransactionPayload::None,
    );
    
    blockchain.add_pending_transaction(tx);
    
    // 提交区块
    let metadata = BlockMetadata::default();
    let _ = blockchain.commit_inference(metadata, "test_node".to_string());
    
    // 区块链应该仍然有效
    assert!(blockchain.verify_chain());
}

/// 模糊测试：大量 KV 存证
#[test]
fn fuzz_many_kv_proofs() {
    let mut blockchain = Blockchain::with_config(
        "test_owner".to_string(),
        BlockchainConfig::default(),
    );
    
    let _ = blockchain.register_node("test_node".to_string());
    
    // 添加 1000 个 KV 存证
    for i in 0..1000 {
        let kv_proof = KvCacheProof::new(
            format!("kv_{}", i),
            format!("hash_{:064}", i),  // 模拟 64 字符哈希
            "test_node".to_string(),
            1024,
        );
        blockchain.add_kv_proof(kv_proof);
    }
    
    // 提交区块
    let metadata = BlockMetadata::default();
    let _ = blockchain.commit_inference(metadata, "test_node".to_string());
    
    // 区块链应该仍然有效
    assert!(blockchain.verify_chain());
}

/// 模糊测试：快速连续提交
#[test]
fn fuzz_rapid_commits() {
    let mut blockchain = Blockchain::with_config(
        "test_owner".to_string(),
        BlockchainConfig::default(),
    );
    
    let _ = blockchain.register_node("test_node".to_string());
    
    // 快速连续提交 100 个区块
    for i in 0..100 {
        let tx = Transaction::new(
            format!("user_{}", i),
            format!("assistant_{}", i),
            TransactionType::Transfer,
            TransactionPayload::None,
        );
        blockchain.add_pending_transaction(tx);
        
        let metadata = BlockMetadata::default();
        let _ = blockchain.commit_inference(metadata, "test_node".to_string());
    }
    
    // 区块链应该仍然有效
    assert!(blockchain.verify_chain());
}
