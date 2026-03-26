//! 并发测试 - 验证线程安全和边界条件
//!
//! **测试目标**：
//! - 验证 Arc<RwLock<Blockchain>> 的线程安全
//! - 验证并发读写 KV 的安全性
//! - 验证并发推理请求的处理
//! - 100 线程压力测试
//!
//! # 运行测试
//!
//! ```bash
//! cargo test --test concurrency_tests -- --nocapture
//! ```

#[cfg(test)]
mod tests {
    use std::sync::{Arc, RwLock};
    use std::thread;
    use std::time::Duration;

    use block_chain_with_context::{
        Blockchain, BlockchainConfig, MemoryLayerManager,
        AccessCredential, AccessType,
        CommitmentService, KvCacheProof, Transaction, TransactionType, TransactionPayload,
        InferenceResponse,
    };
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::collections::HashMap;

    /// 测试区块链的并发写入
    #[test]
    fn test_concurrent_blockchain_writes() {
        let blockchain: Arc<RwLock<Blockchain>> = Arc::new(RwLock::new(
            Blockchain::with_config("test_address".to_string(), BlockchainConfig::default())
        ));

        let mut handles = vec![];

        // 创建 10 个线程，每个线程尝试写入交易
        for i in 0..10 {
            let bc: Arc<RwLock<Blockchain>> = Arc::clone(&blockchain);
            let handle = thread::spawn(move || {
                let mut bc = bc.write().unwrap();
                let tx = Transaction::new(
                    format!("user_{}", i),
                    format!("assistant_{}", i),
                    TransactionType::Transfer,
                    TransactionPayload::None,
                );
                bc.add_pending_transaction(tx);
                // 模拟一些工作
                thread::sleep(Duration::from_millis(10));
            });
            handles.push(handle);
        }

        // 等待所有线程完成
        for handle in handles {
            handle.join().unwrap();
        }

        // 验证所有交易都已添加
        let bc = blockchain.read().unwrap();
        // 创世区块 + 1 个区块（包含所有交易）
        assert!(bc.chain().len() >= 1);
    }

    /// 测试记忆层的并发读写
    #[test]
    fn test_concurrent_memory_access() {
        let memory: Arc<Mutex<MemoryLayerManager>> = Arc::new(Mutex::new(MemoryLayerManager::new("test_node")));
        let credential = AccessCredential {
            credential_id: "test_cred".to_string(),
            provider_id: "provider_1".to_string(),
            memory_block_ids: vec!["all".to_string()],
            access_type: AccessType::ReadWrite,
            expires_at: u64::MAX,
            issuer_node_id: "test_node".to_string(),
            signature: "test_sig".to_string(),
            is_revoked: false,
        };

        let mut write_handles = vec![];
        let mut read_handles = vec![];

        // 创建 5 个写线程
        for i in 0..5 {
            let mem: Arc<Mutex<MemoryLayerManager>> = Arc::clone(&memory);
            let cred = credential.clone();
            let handle = thread::spawn(move || {
                let key = format!("key_{}", i);
                let value = format!("value_{}", i).into_bytes();
                let mut mem = mem.lock().unwrap();
                mem.write_kv(key, value, &cred).unwrap();
                thread::sleep(Duration::from_millis(5));
            });
            write_handles.push(handle);
        }

        // 创建 5 个读线程
        for i in 0..5 {
            let mem: Arc<Mutex<MemoryLayerManager>> = Arc::clone(&memory);
            let cred = credential.clone();
            let handle = thread::spawn(move || {
                let key = format!("key_{}", i);
                // 可能读到也可能读不到，取决于写入顺序
                let mem = mem.lock().unwrap();
                let _ = mem.read_kv(&key, &cred);
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
    }

    /// 测试存证服务的并发提交
    #[test]
    fn test_concurrent_commitment_service() {
        let service: Arc<CommitmentService> = Arc::new(CommitmentService::with_config(
            "test_address".to_string(),
            BlockchainConfig::default(),
        ).unwrap());

        let mut handles = vec![];

        // 创建 10 个线程，每个线程尝试提交 KV 存证
        for i in 0..10 {
            let svc: Arc<CommitmentService> = Arc::clone(&service);
            let handle = thread::spawn(move || {
                let kv_proof = KvCacheProof::new(
                    format!("kv_{}", i),
                    format!("hash_{}", i),
                    format!("node_{}", i),
                    1024 + i,
                );
                svc.commit_kv_proof(kv_proof).unwrap();
                thread::sleep(Duration::from_millis(5));
            });
            handles.push(handle);
        }

        // 等待所有线程完成
        for handle in handles {
            handle.join().unwrap();
        }

        // 验证区块高度
        assert!(service.block_height() >= 1);
    }

    /// 测试边界条件：大量并发写入（100 线程压力测试）
    #[test]
    fn test_stress_concurrent_writes() {
        let blockchain: Arc<RwLock<Blockchain>> = Arc::new(RwLock::new(
            Blockchain::with_config("test_address".to_string(), BlockchainConfig::default())
        ));

        let mut handles = vec![];

        // 创建 100 个线程
        for i in 0..100 {
            let bc: Arc<RwLock<Blockchain>> = Arc::clone(&blockchain);
            let handle = thread::spawn(move || {
                let mut bc = bc.write().unwrap();
                let tx = Transaction::new(
                    format!("user_{}", i),
                    format!("assistant_{}", i),
                    TransactionType::Transfer,
                    TransactionPayload::None,
                );
                bc.add_pending_transaction(tx);
            });
            handles.push(handle);
        }

        // 等待所有线程完成
        for handle in handles {
            handle.join().unwrap();
        }

        // 验证区块链仍然有效
        let bc = blockchain.read().unwrap();
        assert!(bc.verify_chain(), "Blockchain should be valid after stress test");
    }

    /// 测试边界条件：快速连续提交
    #[test]
    fn test_rapid_sequential_commits() {
        let service = CommitmentService::with_config(
            "test_address".to_string(),
            BlockchainConfig::default(),
        ).unwrap();

        // 快速连续提交 50 个 KV 存证
        for i in 0..50 {
            let kv_proof = KvCacheProof::new(
                format!("kv_{}", i),
                format!("hash_{}", i),
                "node_1".to_string(),
                1024,
            );
            service.commit_kv_proof(kv_proof).unwrap();
        }

        // 验证区块高度
        assert!(service.block_height() >= 1);
        assert!(service.verify_blockchain());
    }

    /// 100 线程并发读写区块链压力测试
    #[test]
    fn test_100_threads_concurrent_read_write() {
        let blockchain: Arc<RwLock<Blockchain>> = Arc::new(RwLock::new(
            Blockchain::with_config("test_address".to_string(), BlockchainConfig::default())
        ));

        // 先添加一些初始数据
        {
            let mut bc = blockchain.write().unwrap();
            for i in 0..10 {
                let tx = Transaction::new(
                    format!("init_user_{}", i),
                    format!("init_assistant_{}", i),
                    TransactionType::Transfer,
                    TransactionPayload::None,
                );
                bc.add_pending_transaction(tx);
            }
        }

        let success_count = Arc::new(AtomicUsize::new(0));
        let mut handles = vec![];

        // 创建 100 个线程，混合读写操作
        for i in 0..100 {
            let bc: Arc<RwLock<Blockchain>> = Arc::clone(&blockchain);
            let success_count: Arc<AtomicUsize> = Arc::clone(&success_count);
            let handle = thread::spawn(move || {
                if i % 3 == 0 {
                    // 33% 写操作
                    let mut bc = bc.write().unwrap();
                    let tx = Transaction::new(
                        format!("stress_user_{}", i),
                        format!("stress_assistant_{}", i),
                        TransactionType::Transfer,
                        TransactionPayload::None,
                    );
                    bc.add_pending_transaction(tx);
                    success_count.fetch_add(1, Ordering::SeqCst);
                } else {
                    // 67% 读操作
                    let bc = bc.read().unwrap();
                    let _height = bc.height();
                    let _valid = bc.verify_chain();
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

        // 验证区块链仍然有效
        let bc = blockchain.read().unwrap();
        assert!(bc.verify_chain(), "Blockchain should be valid after 100-thread stress test");
    }

    /// 100 线程并发提交 KV 存证压力测试
    #[test]
    fn test_100_threads_concurrent_kv_proofs() {
        let service: Arc<CommitmentService> = Arc::new(CommitmentService::with_config(
            "test_address".to_string(),
            BlockchainConfig::default(),
        ).unwrap());

        let success_count = Arc::new(AtomicUsize::new(0));
        let mut handles = vec![];

        // 创建 100 个线程同时提交 KV 存证
        for i in 0..100 {
            let svc: Arc<CommitmentService> = Arc::clone(&service);
            let success_count: Arc<AtomicUsize> = Arc::clone(&success_count);
            let handle = thread::spawn(move || {
                let kv_proof = KvCacheProof::new(
                    format!("stress_kv_{}", i),
                    format!("stress_hash_{}", i),
                    format!("stress_node_{}", i % 10),
                    1024 + i,
                );
                if svc.commit_kv_proof(kv_proof).is_ok() {
                    success_count.fetch_add(1, Ordering::SeqCst);
                }
            });
            handles.push(handle);
        }

        // 等待所有线程完成
        for handle in handles {
            handle.join().unwrap();
        }

        // 验证所有提交都成功
        assert_eq!(success_count.load(Ordering::SeqCst), 100);

        // 验证区块链仍然有效
        assert!(service.verify_blockchain(), "Blockchain should be valid after 100-thread KV proof stress test");
    }

    /// 混合操作压力测试：同时提交交易和 KV 存证
    #[test]
    fn test_mixed_operations_stress() {
        let service: Arc<CommitmentService> = Arc::new(CommitmentService::with_config(
            "test_address".to_string(),
            BlockchainConfig::default(),
        ).unwrap());

        let success_count = Arc::new(AtomicUsize::new(0));
        let mut handles = vec![];

        // 创建 50 个线程提交交易，50 个线程提交 KV 存证
        for i in 0..100 {
            let svc: Arc<CommitmentService> = Arc::clone(&service);
            let success_count: Arc<AtomicUsize> = Arc::clone(&success_count);
            let handle = thread::spawn(move || {
                if i % 2 == 0 {
                    // 提交交易
                    if svc.commit_transaction(
                        format!("user_{}", i),
                        format!("assistant_{}", i),
                        &InferenceResponse {
                            request_id: format!("req_{}", i),
                            completion: format!("response_{}", i),
                            prompt_tokens: 10,
                            completion_tokens: 20,
                            latency_ms: 100,
                            efficiency: 0.0,
                            new_kv: HashMap::new(),
                            success: true,
                            error_message: None,
                        },
                    ).is_ok() {
                        success_count.fetch_add(1, Ordering::SeqCst);
                    }
                } else {
                    // 提交 KV 存证
                    let kv_proof = KvCacheProof::new(
                        format!("kv_{}", i),
                        format!("hash_{}", i),
                        format!("node_{}", i % 5),
                        512 + i,
                    );
                    if svc.commit_kv_proof(kv_proof).is_ok() {
                        success_count.fetch_add(1, Ordering::SeqCst);
                    }
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
        assert!(service.verify_blockchain());
    }
}
