//! 并发压力测试
//!
//! **测试目标**：
//! 1. 多线程并发读写区块链
//! 2. 竞态条件检测
//! 3. 死锁检测
//! 4. 高并发场景下的数据一致性

use block_chain_with_context::{
    blockchain::Blockchain,
    transaction::{Transaction, TransactionType, TransactionPayload},
    metadata::BlockMetadata,
};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

/// 创建测试交易
fn create_tx_for_thread(thread_id: u32, tx_id: u32) -> Transaction {
    let mut tx = Transaction::new_internal(
        format!("user_thread_{}_tx_{}", thread_id, tx_id),
        format!("assistant_thread_{}", thread_id),
        TransactionType::Internal,
        TransactionPayload::None,
    );
    tx.gas_used = 10;
    tx
}

// ==================== 并发读写测试 ====================

/// 基础并发测试：10 个线程同时写入
#[test]
fn test_concurrent_writes_10_threads() {
    let blockchain = Arc::new(RwLock::new(Blockchain::new("concurrent_user".to_string())));
    let mut handles = vec![];
    
    for i in 0..10 {
        let bc = Arc::clone(&blockchain);
        let handle = thread::spawn(move || {
            let mut bc = bc.write().unwrap();
            let tx = create_tx_for_thread(i, 1);
            bc.add_pending_transaction(tx);
            i
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    let bc = blockchain.read().unwrap();
    assert_eq!(bc.pending_transactions().len(), 10);
}

/// 高并发压力测试：100 个线程同时写入
#[test]
fn test_concurrent_writes_100_threads() {
    let blockchain = Arc::new(RwLock::new(Blockchain::new("stress_user".to_string())));
    let mut handles = vec![];
    
    for i in 0..100 {
        let bc = Arc::clone(&blockchain);
        let handle = thread::spawn(move || {
            let mut bc = bc.write().unwrap();
            let tx = create_tx_for_thread(i, 1);
            bc.add_pending_transaction(tx);
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    let bc = blockchain.read().unwrap();
    assert_eq!(bc.pending_transactions().len(), 100);
}

/// 并发读写混合测试
#[test]
fn test_concurrent_read_write_mixed() {
    let blockchain = Arc::new(RwLock::new(Blockchain::new("mixed_user".to_string())));
    let mut handles = vec![];
    
    for i in 0..50 {
        let bc = Arc::clone(&blockchain);
        let handle = thread::spawn(move || {
            let mut bc = bc.write().unwrap();
            let tx = create_tx_for_thread(i, 1);
            bc.add_pending_transaction(tx);
        });
        handles.push(handle);
    }
    
    for _ in 0..50 {
        let bc = Arc::clone(&blockchain);
        let handle = thread::spawn(move || {
            let bc = bc.read().unwrap();
            let _count = bc.pending_transactions().len();
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    let bc = blockchain.read().unwrap();
    assert_eq!(bc.pending_transactions().len(), 50);
}

// ==================== 区块提交并发测试 ====================

/// 并发提交多个区块
#[test]
fn test_concurrent_block_commits() {
    let blockchain = Arc::new(RwLock::new(Blockchain::new("commit_user".to_string())));
    let mut handles = vec![];
    
    for i in 0..5 {
        let bc = Arc::clone(&blockchain);
        let handle = thread::spawn(move || {
            let mut bc = bc.write().unwrap();
            let tx = create_tx_for_thread(i, 1);
            bc.add_pending_transaction(tx);
            let metadata = BlockMetadata::default();
            let result = bc.commit_inference(metadata, format!("node_{}", i));
            result.is_ok()
        });
        handles.push(handle);
    }
    
    let mut results = vec![];
    for handle in handles {
        results.push(handle.join().unwrap());
    }
    
    let bc = blockchain.read().unwrap();
    assert!(bc.chain().len() >= 1);
    assert!(bc.verify_chain());
}

/// 竞态条件测试：同时提交两个块
#[test]
fn test_race_condition_double_commit() {
    let blockchain = Arc::new(RwLock::new(Blockchain::new("race_user".to_string())));
    
    {
        let mut bc = blockchain.write().unwrap();
        for i in 0..5 {
            let tx = create_tx_for_thread(i, 1);
            bc.add_pending_transaction(tx);
        }
    }
    
    let bc1 = Arc::clone(&blockchain);
    let handle1 = thread::spawn(move || {
        let mut bc = bc1.write().unwrap();
        let metadata = BlockMetadata::default();
        bc.commit_inference(metadata, "node_a".to_string()).is_ok()
    });

    let bc2 = Arc::clone(&blockchain);
    let handle2 = thread::spawn(move || {
        let mut bc = bc2.write().unwrap();
        let metadata = BlockMetadata::default();
        bc.commit_inference(metadata, "node_b".to_string()).is_ok()
    });

    let result1 = handle1.join().unwrap();
    let result2 = handle2.join().unwrap();

    assert!(result1 || result2);
    
    let bc = blockchain.read().unwrap();
    assert!(bc.chain().len() >= 1);
}

// ==================== 死锁检测测试 ====================

/// 死锁检测：快速连续读写
#[test]
fn test_deadlock_detection_rapid_access() {
    let blockchain = Arc::new(RwLock::new(Blockchain::new("deadlock_user".to_string())));
    let mut handles = vec![];
    
    for i in 0..20 {
        let bc = Arc::clone(&blockchain);
        let handle = thread::spawn(move || {
            for j in 0..10 {
                if j % 2 == 0 {
                    let mut bc = bc.write().unwrap();
                    let tx = create_tx_for_thread(i, j);
                    bc.add_pending_transaction(tx);
                } else {
                    let bc = bc.read().unwrap();
                    let _len = bc.pending_transactions().len();
                }
                thread::sleep(Duration::from_millis(1));
            }
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    let bc = blockchain.read().unwrap();
    assert!(bc.pending_transactions().len() > 0);
}

// ==================== 异步并发测试 ====================

#[cfg(test)]
mod async_tests {
    use super::*;
    use tokio::sync::Mutex;
    use tokio::task;
    
    #[tokio::test]
    async fn test_async_concurrent_writes() {
        let blockchain = Arc::new(Mutex::new(Blockchain::new("async_user".to_string())));
        let mut handles = vec![];
        
        for i in 0..20 {
            let bc = Arc::clone(&blockchain);
            let handle = task::spawn(async move {
                let mut bc = bc.lock().await;
                let tx = create_tx_for_thread(i, 1);
                bc.add_pending_transaction(tx);
            });
            handles.push(handle);
        }
        
        for handle in handles {
            handle.await.unwrap();
        }
        
        let bc = blockchain.lock().await;
        assert_eq!(bc.pending_transactions().len(), 20);
    }
    
    #[tokio::test]
    async fn test_async_stress_100_tasks() {
        let blockchain = Arc::new(Mutex::new(Blockchain::new("async_stress_user".to_string())));
        let mut handles = vec![];
        
        for i in 0..100 {
            let bc = Arc::clone(&blockchain);
            let handle = task::spawn(async move {
                let mut bc = bc.lock().await;
                let tx = create_tx_for_thread(i, 1);
                bc.add_pending_transaction(tx);
            });
            handles.push(handle);
        }
        
        for handle in handles {
            handle.await.unwrap();
        }
        
        let bc = blockchain.lock().await;
        assert_eq!(bc.pending_transactions().len(), 100);
    }
}
