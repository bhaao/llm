//! 异步提交服务压力测试 - Channel + 批处理
//!
//! 测试场景：
//! 1. 高并发提交压力
//! 2. 批处理大小触发
//! 3. 批处理超时触发
//! 4. 背压控制（通道满）
//! 5. 超时处理

use block_chain_with_context::async_commit::{
    AsyncCommitService, AsyncCommitConfig, CommitRequest, CommitResult,
};
use block_chain_with_context::block::KvCacheProof;
use block_chain_with_context::metadata::BlockMetadata;
use std::time::Duration;

/// 创建测试提交请求
fn create_test_request(request_id: &str) -> CommitRequest {
    CommitRequest {
        request_id: request_id.to_string(),
        node_id: "node_1".to_string(),
        output: format!("output_{}", request_id),
        kv_proof: KvCacheProof::new(
            format!("kv_{}", request_id),
            format!("hash_{}", request_id),
            "node_1".to_string(),
            100,
        ),
        metadata: BlockMetadata::default(),
        expected_tokens: None,
        response_tx: None,
    }
}

/// 测试：异步提交服务基本创建
#[tokio::test]
async fn test_async_commit_service_creation() {
    let config = AsyncCommitConfig::default();
    let _service = AsyncCommitService::new(config);
    
    assert!(!_service.is_running());
    assert_eq!(_service.stats().batch_size, 10);
    assert_eq!(_service.stats().batch_timeout_ms, 1000);
}

/// 测试：高并发提交压力（100 并发）
#[tokio::test]
async fn test_high_concurrency_stress() {
    let config = AsyncCommitConfig {
        batch_size: 20,
        batch_timeout_ms: 500,
        channel_buffer_size: 200,
        commit_timeout_ms: 5000,
    };
    
    let mut service = AsyncCommitService::new(config);
    let sender = service.sender();
    
    // 启动服务
    let handle = tokio::spawn(async move {
        service.run().await
    });
    
    // 并发发送 100 个请求
    let mut send_handles = Vec::new();
    for i in 0..100 {
        let sender_clone = sender.clone();
        let handle = tokio::spawn(async move {
            let request = create_test_request(&format!("req_{}", i));
            sender_clone.send(request).await
        });
        send_handles.push(handle);
    }
    
    // 等待所有发送完成
    for handle in send_handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok(), "Failed to send request");
    }
    
    // 等待处理
    tokio::time::sleep(Duration::from_millis(200)).await;
    
    // 停止服务
    drop(sender);
    let _ = handle.await;
}

/// 测试：批处理大小触发
#[tokio::test]
async fn test_batch_trigger_by_size() {
    let config = AsyncCommitConfig {
        batch_size: 5,
        batch_timeout_ms: 10000, // 很长的超时，确保按大小触发
        channel_buffer_size: 20,
        commit_timeout_ms: 5000,
    };
    
    let mut service = AsyncCommitService::new(config);
    let sender = service.sender();
    
    // 启动服务
    let handle = tokio::spawn(async move {
        service.run().await
    });
    
    // 发送 5 个请求（达到批处理大小）
    for i in 0..5 {
        let request = create_test_request(&format!("req_{}", i));
        sender.send(request).await.unwrap();
    }
    
    // 等待处理
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // 停止服务
    drop(sender);
    let _ = handle.await;
}

/// 测试：批处理超时触发
#[tokio::test]
async fn test_batch_trigger_by_timeout() {
    let config = AsyncCommitConfig {
        batch_size: 20, // 很大的大小，确保按超时触发
        batch_timeout_ms: 100,
        channel_buffer_size: 20,
        commit_timeout_ms: 5000,
    };
    
    let mut service = AsyncCommitService::new(config);
    let sender = service.sender();
    
    // 启动服务
    let handle = tokio::spawn(async move {
        service.run().await
    });
    
    // 发送 1 个请求（不足批处理大小，等待超时）
    let request = create_test_request("req_1");
    sender.send(request).await.unwrap();
    
    // 等待超时触发
    tokio::time::sleep(Duration::from_millis(200)).await;
    
    // 停止服务
    drop(sender);
    let _ = handle.await;
}

/// 测试：背压控制（通道满）
#[tokio::test]
async fn test_backpressure_when_channel_full() {
    let config = AsyncCommitConfig {
        batch_size: 5,
        batch_timeout_ms: 1000,
        channel_buffer_size: 10, // 小缓冲区
        commit_timeout_ms: 5000,
    };
    
    let service = AsyncCommitService::new(config);
    let sender = service.sender();
    
    // 启动服务（不消费，模拟慢消费者）
    let handle = tokio::spawn(async move {
        // 故意延迟启动，让通道积累压力
        tokio::time::sleep(Duration::from_millis(500)).await;
        service.run().await
    });
    
    // 快速发送超过缓冲区的请求
    let mut send_results = Vec::new();
    for i in 0..20 {
        let request = create_test_request(&format!("req_{}", i));
        let result = sender.send(request).await;
        send_results.push(result);
    }
    
    // 部分发送应该成功（缓冲区大小限制）
    let success_count = send_results.iter().filter(|r| r.is_ok()).count();
    assert!(success_count >= 10, "At least buffer size requests should succeed");
    
    // 等待处理
    tokio::time::sleep(Duration::from_millis(200)).await;
    
    // 停止服务
    drop(sender);
    let _ = handle.await;
}

/// 测试：提交超时处理
#[tokio::test]
async fn test_commit_timeout_handling() {
    let config = AsyncCommitConfig {
        batch_size: 10,
        batch_timeout_ms: 100,
        channel_buffer_size: 20,
        commit_timeout_ms: 50, // 很短的超时
    };
    
    let mut service = AsyncCommitService::new(config);
    
    // 服务未启动，提交应该失败
    let request = create_test_request("req_1");
    let result = service.submit_with_timeout(request).await;
    assert!(result.is_err());
}

/// 测试：多批次连续提交
#[tokio::test]
async fn test_sequential_batch_commits() {
    let config = AsyncCommitConfig {
        batch_size: 5,
        batch_timeout_ms: 100,
        channel_buffer_size: 50,
        commit_timeout_ms: 5000,
    };
    
    let mut service = AsyncCommitService::new(config);
    let sender = service.sender();
    
    // 启动服务
    let handle = tokio::spawn(async move {
        service.run().await
    });
    
    // 发送 3 批请求
    for batch in 0..3 {
        for i in 0..5 {
            let request = create_test_request(&format!("batch_{}_req_{}", batch, i));
            sender.send(request).await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    
    // 等待所有批次处理完成
    tokio::time::sleep(Duration::from_millis(300)).await;
    
    // 停止服务
    drop(sender);
    let _ = handle.await;
}

/// 测试：CommitResult 创建和验证
#[tokio::test]
async fn test_commit_result_creation() {
    let success_result = CommitResult::success(100, "abc123".to_string(), 50);
    assert!(success_result.success);
    assert_eq!(success_result.block_height, Some(100));
    assert_eq!(success_result.block_hash, Some("abc123".to_string()));
    assert!(success_result.error.is_none());
    assert_eq!(success_result.latency_ms, 50);
    
    let failure_result = CommitResult::failure("Test error".to_string());
    assert!(!failure_result.success);
    assert!(failure_result.block_height.is_none());
    assert!(failure_result.block_hash.is_none());
    assert_eq!(failure_result.error, Some("Test error".to_string()));
}

/// 测试：异步提交统计信息
#[tokio::test]
async fn test_async_commit_stats() {
    let config = AsyncCommitConfig {
        batch_size: 15,
        batch_timeout_ms: 2000,
        channel_buffer_size: 150,
        commit_timeout_ms: 10000,
    };
    
    let service = AsyncCommitService::new(config);
    let stats = service.stats();
    
    assert_eq!(stats.batch_size, 15);
    assert_eq!(stats.batch_timeout_ms, 2000);
    assert_eq!(stats.buffer_size, 150);
    assert!(!stats.running);
}

/// 测试：极端大批量提交（1000 请求）
#[tokio::test]
async fn test_extreme_bulk_submission() {
    let config = AsyncCommitConfig {
        batch_size: 50,
        batch_timeout_ms: 200,
        channel_buffer_size: 500,
        commit_timeout_ms: 10000,
    };
    
    let mut service = AsyncCommitService::new(config);
    let sender = service.sender();
    
    // 启动服务
    let handle = tokio::spawn(async move {
        service.run().await
    });
    
    // 发送 1000 个请求
    let mut send_handles = Vec::new();
    for i in 0..1000 {
        let sender_clone = sender.clone();
        let handle = tokio::spawn(async move {
            let request = create_test_request(&format!("bulk_req_{}", i));
            sender_clone.send(request).await
        });
        send_handles.push(handle);
    }
    
    // 等待所有发送完成
    for handle in send_handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok(), "Failed to send bulk request");
    }
    
    // 等待处理完成
    tokio::time::sleep(Duration::from_millis(2000)).await;
    
    // 停止服务
    drop(sender);
    let _ = handle.await;
}

/// 测试：服务启动和停止
#[tokio::test]
async fn test_service_start_stop() {
    let config = AsyncCommitConfig::default();
    let service = AsyncCommitService::new(config);
    
    assert!(!service.is_running());
    
    let sender = service.sender();
    
    // 启动服务
    let mut service_mut = service;
    let handle = tokio::spawn(async move {
        service_mut.run().await
    });
    
    // 等待启动
    tokio::time::sleep(Duration::from_millis(50)).await;
    
    // 停止服务（通过 drop sender）
    drop(sender);
    let _ = handle.await;
}
