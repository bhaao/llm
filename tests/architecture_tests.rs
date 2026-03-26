//! 三层架构集成测试 - 端到端测试节点层、记忆层、推理提供商层的解耦架构
//!
//! 测试场景：
//! 1. 完整推理流程测试
//! 2. 提供商切换测试
//! 3. 访问凭证验证测试
//! 4. 记忆链哈希校验测试
//! 5. 多副本容灾测试
//! 6. 并发推理测试
//! 7. 错误处理测试
//! 8. 区块链存证验证测试
//!
//! **注意**: 此测试文件仍使用废弃的 ArchitectureCoordinator，
//! 未来应迁移到新的服务层 API (InferenceOrchestrator, CommitmentService, FailoverService)

#![allow(deprecated)]

use block_chain_with_context::deprecated::coordinator::ArchitectureCoordinator;
use block_chain_with_context::provider_layer::{InferenceEngineType, InferenceRequest};
use block_chain_with_context::node_layer::{AccessType, NodeIdentity, NodeRole, ProviderStatus};
use std::sync::{Arc, RwLock};
use std::thread;

/// 测试完整推理流程
#[test]
fn test_end_to_end_inference_flow() {
    // 创建协调器
    let mut coordinator = ArchitectureCoordinator::new("node_1".to_string());

    // 注册推理提供商
    coordinator.register_provider(
        "provider_1".to_string(),
        InferenceEngineType::Vllm,
        100,
    ).unwrap();

    // 创建推理请求
    let request = InferenceRequest::new(
        "req_1".to_string(),
        "Hello, AI! Please explain quantum computing.".to_string(),
        "llama-7b".to_string(),
        200,
    ).with_memory_blocks(vec![0]);

    // 执行推理
    let response = coordinator.execute_inference(request).unwrap();

    // 验证响应
    assert!(response.success);
    assert!(!response.completion.is_empty());
    assert!(response.completion_tokens > 0);
    assert!(response.prompt_tokens > 0);
    // latency_ms is always >= 0 due to type (u64)
    assert!(!response.new_kv.is_empty());

    // 验证链完整性
    assert!(coordinator.verify_memory_chain());
    assert!(coordinator.verify_blockchain());

    // 验证统计
    let stats = coordinator.get_inference_stats();
    assert_eq!(stats.total, 1);
    assert_eq!(stats.successful, 1);
    assert_eq!(stats.success_rate, 1.0);
}

/// 测试推理提供商动态切换
#[test]
fn test_provider_switching() {
    let mut coordinator = ArchitectureCoordinator::new("node_1".to_string());

    // 注册两个提供商
    coordinator.register_provider(
        "provider_1".to_string(),
        InferenceEngineType::Vllm,
        100,
    ).unwrap();

    coordinator.register_provider(
        "provider_2".to_string(),
        InferenceEngineType::Sglang,
        80,
    ).unwrap();

    // 执行第一次推理（使用 provider_1）
    let request1 = InferenceRequest::new(
        "req_1".to_string(),
        "First prompt".to_string(),
        "llama-7b".to_string(),
        100,
    );
    let response1 = coordinator.execute_inference(request1).unwrap();
    assert!(response1.success);

    // 切换到 provider_2
    coordinator.switch_provider("provider_2", "load balancing").unwrap();

    // 执行第二次推理（使用 provider_2）
    let request2 = InferenceRequest::new(
        "req_2".to_string(),
        "Second prompt".to_string(),
        "llama-7b".to_string(),
        100,
    );
    let response2 = coordinator.execute_inference(request2).unwrap();
    assert!(response2.success);

    // 验证统计
    let stats = coordinator.get_inference_stats();
    assert_eq!(stats.total, 2);
    assert_eq!(stats.successful, 2);
}

/// 测试访问凭证验证
#[test]
fn test_access_credential_verification() {
    let mut coordinator = ArchitectureCoordinator::new("node_1".to_string());

    coordinator.register_provider(
        "provider_1".to_string(),
        InferenceEngineType::Vllm,
        100,
    ).unwrap();

    // 签发只读凭证
    let read_credential = coordinator.node_layer.issue_credential(
        "provider_1".to_string(),
        vec!["0".to_string()],
        AccessType::ReadOnly,
        3600,
    ).unwrap();

    // 签发写入凭证
    let write_credential = coordinator.node_layer.issue_credential(
        "provider_1".to_string(),
        vec!["0".to_string()],
        AccessType::WriteOnly,
        3600,
    ).unwrap();

    // 验证凭证有效
    assert!(read_credential.is_valid());
    assert!(write_credential.is_valid());

    // 撤销凭证
    let credential_id = read_credential.credential_id.clone();
    coordinator.node_layer_mut().revoke_credential(&credential_id).unwrap();

    // 验证凭证已撤销（再次撤销应该返回错误）
    assert!(coordinator.node_layer_mut().revoke_credential(&credential_id).is_err());
}

/// 测试记忆层哈希校验
#[test]
fn test_memory_hash_verification() {
    let mut coordinator = ArchitectureCoordinator::new("node_1".to_string());

    // 写入 KV 数据
    let credential = coordinator.node_layer.issue_credential(
        "provider_1".to_string(),
        vec!["0".to_string()],
        AccessType::ReadWrite,
        3600,
    ).unwrap();

    let kv_data = b"test_kv_data_for_hash_verification";
    coordinator.memory_layer_mut().write_kv(
        "test_key".to_string(),
        kv_data.to_vec(),
        &credential,
    ).unwrap();

    // 获取区块哈希（写入后区块已更新）
    let block = coordinator.memory_layer.latest_block();
    let expected_hash = block.map(|b| b.header.hash.clone()).unwrap_or_default();

    // 验证哈希
    let latest_index = coordinator.memory_layer.latest_block_index();
    assert!(coordinator.memory_layer.verify_hash(latest_index, &expected_hash));
    assert!(!coordinator.memory_layer.verify_hash(latest_index, "invalid_hash"));

    // 验证 KV 完整性
    let shard = coordinator.memory_layer.get_block(latest_index).unwrap().get_shard("test_key").unwrap();
    assert!(shard.verify_integrity());
}

/// 测试多副本容灾
#[test]
fn test_multi_replica_fault_tolerance() {
    let mut coordinator = ArchitectureCoordinator::new("node_1".to_string());

    // 添加多个副本
    coordinator.memory_layer_mut().add_replica(0, "node_2".to_string()).unwrap();
    coordinator.memory_layer_mut().add_replica(0, "node_3".to_string()).unwrap();
    coordinator.memory_layer_mut().add_replica(0, "node_4".to_string()).unwrap();

    // 验证副本数量
    let replicas = coordinator.memory_layer.get_replicas(0).unwrap();
    assert_eq!(replicas.len(), 3);
    assert!(replicas.contains(&"node_2".to_string()));
    assert!(replicas.contains(&"node_3".to_string()));
    assert!(replicas.contains(&"node_4".to_string()));

    // 模拟单节点故障（移除一个副本）
    // 实际场景中应该从其他副本恢复数据
    assert!(coordinator.verify_memory_chain());
}

/// 测试并发推理
#[test]
fn test_concurrent_inference() {
    let coordinator = Arc::new(RwLock::new(
        ArchitectureCoordinator::new("node_1".to_string())
    ));

    // 注册提供商
    {
        let mut coord = coordinator.write().unwrap();
        coord.register_provider(
            "provider_1".to_string(),
            InferenceEngineType::Vllm,
            100,
        ).unwrap();
    }

    // 创建多个线程并发执行推理
    let mut handles = Vec::new();
    for i in 0..5 {
        let coord_clone = Arc::clone(&coordinator);
        let handle = thread::spawn(move || {
            let mut coord = coord_clone.write().unwrap();
            let request = InferenceRequest::new(
                format!("req_{}", i),
                format!("Prompt {}", i),
                "llama-7b".to_string(),
                100,
            );
            coord.execute_inference(request)
        });
        handles.push(handle);
    }

    // 等待所有线程完成
    let results: Vec<_> = handles.into_iter()
        .map(|h| h.join().unwrap())
        .collect();

    // 验证所有推理都成功
    for result in results {
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.success);
    }

    // 验证统计
    let coord = coordinator.read().unwrap();
    let stats = coord.get_inference_stats();
    assert_eq!(stats.total, 5);
    assert_eq!(stats.successful, 5);
}

/// 测试错误处理 - 无效提供商
#[test]
fn test_error_handling_invalid_provider() {
    let mut coordinator = ArchitectureCoordinator::new("node_1".to_string());

    // 尝试切换到不存在的提供商
    let result = coordinator.switch_provider("non_existent_provider", "test");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not found"));
}

/// 测试错误处理 - 无可用提供商
#[test]
fn test_error_handling_no_available_provider() {
    let mut coordinator = ArchitectureCoordinator::new("node_1".to_string());

    // 不注册任何提供商，直接执行推理
    let request = InferenceRequest::new(
        "req_1".to_string(),
        "Hello!".to_string(),
        "llama-7b".to_string(),
        100,
    );

    let result = coordinator.execute_inference(request);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("No available inference provider"));
}

/// 测试区块链存证验证
#[test]
fn test_blockchain_attestation_verification() {
    let mut coordinator = ArchitectureCoordinator::new("node_1".to_string());

    coordinator.register_provider(
        "provider_1".to_string(),
        InferenceEngineType::Vllm,
        100,
    ).unwrap();

    // 执行推理
    let request = InferenceRequest::new(
        "req_1".to_string(),
        "Test prompt for attestation".to_string(),
        "llama-7b".to_string(),
        100,
    );
    coordinator.execute_inference(request).unwrap();

    // 获取 KV 证明
    let kv_proofs = coordinator.get_kv_proofs();
    assert!(!kv_proofs.is_empty());

    // 验证区块链上有存证
    let blockchain = &coordinator.blockchain;
    let blockchain_read = blockchain.read().unwrap();
    let all_kv_proofs = blockchain_read.get_all_kv_proofs();
    assert!(!all_kv_proofs.is_empty());

    // 验证 KV 完整性
    for proof in all_kv_proofs.iter() {
        // 模拟验证 KV 数据
        let test_data = b"test";
        let is_valid = proof.verify_kv_integrity(test_data);
        // 由于测试数据不匹配，应该返回 false
        assert!(!is_valid);
    }
    // blockchain_read 在这里自动 drop
}

/// 测试调度策略切换
#[test]
fn test_scheduling_strategy_switching() {
    use block_chain_with_context::node_layer::SchedulingStrategy;

    let mut coordinator = ArchitectureCoordinator::new("node_1".to_string());

    // 注册多个提供商
    coordinator.register_provider(
        "efficient_provider".to_string(),
        InferenceEngineType::Vllm,
        150, // 高效率
    ).unwrap();

    coordinator.register_provider(
        "quality_provider".to_string(),
        InferenceEngineType::Sglang,
        80, // 低效率
    ).unwrap();

    // 设置质量优先策略
    coordinator.node_layer_mut()
        .set_scheduling_strategy(SchedulingStrategy::QualityFirst);

    // 更新提供商指标
    coordinator.node_layer_mut()
        .report_provider_metrics("efficient_provider", 150.0, true).unwrap();
    coordinator.node_layer_mut()
        .report_provider_metrics("quality_provider", 80.0, true).unwrap();

    // 模拟质量得分差异
    {
        let provider = coordinator.node_layer_mut()
            .get_provider_mut("quality_provider").unwrap();
        provider.quality_score = 0.99; // 高质量
    }
    {
        let provider = coordinator.node_layer_mut()
            .get_provider_mut("efficient_provider").unwrap();
        provider.quality_score = 0.85; // 低质量
    }

    // 选择最佳提供商（应该选择质量高的）
    let best = coordinator.node_layer.select_best_provider().unwrap();
    assert_eq!(best.provider_id, "quality_provider");
}

/// 测试记忆层版本控制
#[test]
fn test_memory_layer_versioning() {
    let mut coordinator = ArchitectureCoordinator::new("node_1".to_string());

    let credential = coordinator.node_layer.issue_credential(
        "provider_1".to_string(),
        vec!["0".to_string()],
        AccessType::ReadWrite,
        3600,
    ).unwrap();

    // 写入多个 KV，触发区块创建
    for i in 0..5 {
        coordinator.memory_layer_mut().write_kv(
            format!("key_{}", i),
            format!("value_{}", i).into_bytes(),
            &credential,
        ).unwrap();

        // 密封当前区块，强制创建新区块
        if i % 2 == 0 {
            coordinator.memory_layer_mut().seal_current_block();
        }
    }

    // 验证区块高度
    assert!(coordinator.memory_layer.height() > 1);

    // 验证链完整性
    assert!(coordinator.verify_memory_chain());
}

/// 测试节点角色权限
#[test]
fn test_node_role_permissions() {
    let mut coordinator = ArchitectureCoordinator::new("node_1".to_string());

    // 注册不同角色的节点
    let consensus_node = NodeIdentity::new(
        "consensus_node".to_string(),
        "address_consensus".to_string(),
        NodeRole::Consensus,
        "pubkey_consensus".to_string(),
        Some("Consensus node info".to_string()),
    );

    let regular_node = NodeIdentity::new(
        "regular_node".to_string(),
        "address_regular".to_string(),
        NodeRole::Regular,
        "pubkey_regular".to_string(),
        Some("Regular node info".to_string()),
    );

    let regulatory_node = NodeIdentity::new(
        "regulatory_node".to_string(),
        "address_regulatory".to_string(),
        NodeRole::Regulatory,
        "pubkey_regulatory".to_string(),
        Some("Regulatory node info".to_string()),
    );

    coordinator.node_layer_mut().register_node(consensus_node).unwrap();
    coordinator.node_layer_mut().register_node(regular_node).unwrap();
    coordinator.node_layer_mut().register_node(regulatory_node).unwrap();

    // 验证共识节点列表
    let consensus_nodes = coordinator.node_layer.get_consensus_nodes();
    assert_eq!(consensus_nodes.len(), 2); // 包括当前节点

    // 验证活跃节点列表
    let active_nodes = coordinator.node_layer.get_active_nodes();
    assert_eq!(active_nodes.len(), 4); // 包括当前节点
}

/// 测试提供商状态管理
#[test]
fn test_provider_status_management() {
    let mut coordinator = ArchitectureCoordinator::new("node_1".to_string());

    coordinator.register_provider(
        "provider_1".to_string(),
        InferenceEngineType::Vllm,
        100,
    ).unwrap();

    // 验证初始状态为待审核（Pending）
    let provider = coordinator.provider_layer.get_provider_record("provider_1").unwrap();
    assert_eq!(provider.status, ProviderStatus::Pending);

    // 激活提供商
    coordinator.provider_layer_mut()
        .update_provider_status("provider_1", ProviderStatus::Active).unwrap();

    // 验证已激活
    let provider = coordinator.provider_layer.get_provider_record("provider_1").unwrap();
    assert_eq!(provider.status, ProviderStatus::Active);

    // 暂停提供商
    coordinator.provider_layer_mut()
        .update_provider_status("provider_1", ProviderStatus::Suspended).unwrap();

    // 验证已暂停
    let provider = coordinator.provider_layer.get_provider_record("provider_1").unwrap();
    assert_eq!(provider.status, ProviderStatus::Suspended);

    // 验证活跃提供商列表不再包含
    let active_providers = coordinator.provider_layer.get_active_providers();
    assert_eq!(active_providers.len(), 0);
}
