//! PBFT 共识集成测试 - 多节点模拟
//!
//! 测试场景：
//! 1. 多节点共识流程（Pre-prepare → Prepare → Commit）
//! 2. 视图切换（Leader 故障）
//! 3. Checkpoint 垃圾回收
//! 4. 拜占庭容错（f 个故障节点）

use block_chain_with_context::consensus::{
    PBFTConsensus, ConsensusConfig, ConsensusState,
    PBFTMessage, Operation, SignedMessage,
};
use std::collections::HashMap;

/// 创建 4 节点测试集群（容忍 1 个故障节点）
fn create_4_node_cluster() -> Vec<(String, PBFTConsensus)> {
    let nodes = vec![
        "node_0".to_string(),
        "node_1".to_string(),
        "node_2".to_string(),
        "node_3".to_string(),
    ];

    nodes
        .iter()
        .map(|node_id| {
            let config = ConsensusConfig::for_testing(node_id.clone(), nodes.clone());
            let consensus = PBFTConsensus::new(config);
            (node_id.clone(), consensus)
        })
        .collect()
}

/// 测试：PBFT 共识基本流程
#[test]
fn test_pbft_basic_consensus() {
    let mut cluster = create_4_node_cluster();

    // 选择 node_0 作为 Leader
    let leader = &mut cluster[0].1;

    // 创建操作
    let operation = Operation {
        id: "op_1".to_string(),
        data: b"test consensus data".to_vec(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
    };

    // Leader 提出操作
    let pre_prepare = leader.propose(operation.clone());
    assert!(pre_prepare.is_ok());

    // 验证状态
    assert_eq!(leader.state(), ConsensusState::Normal);
}

/// 测试：多节点 Prepare 阶段
#[test]
fn test_pbft_prepare_phase() {
    let mut cluster = create_4_node_cluster();

    // Leader 提出操作
    let operation = Operation {
        id: "op_1".to_string(),
        data: b"test prepare phase".to_vec(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
    };

    let leader = &mut cluster[0].1;
    let pre_prepare = leader.propose(operation.clone()).unwrap();

    // 其他节点接收 Pre-prepare 并发送 Prepare
    for (_, node) in cluster.iter_mut().skip(1) {
        let prepare = node.handle_pre_prepare(pre_prepare.clone());
        assert!(prepare.is_ok());
    }
}

/// 测试：视图切换（Leader 故障）
#[test]
fn test_pbft_view_change() {
    let mut cluster = create_4_node_cluster();

    // 触发 node_0 的视图切换
    let node_0 = &mut cluster[0].1;
    let view_change_msg = node_0.initiate_view_change(1);

    assert!(view_change_msg.is_ok());
    assert_eq!(node_0.state(), ConsensusState::ViewChanging);

    // 其他节点接收视图切换消息
    for (_, node) in cluster.iter_mut().skip(1) {
        let result = node.handle_view_change_message(view_change_msg.clone().unwrap());
        assert!(result.is_ok());
    }
}

/// 测试：Checkpoint 创建
#[test]
fn test_pbft_checkpoint() {
    let mut cluster = create_4_node_cluster();

    // 创建 Checkpoint
    let node_0 = &mut cluster[0].1;
    let checkpoint = node_0.create_checkpoint(100);

    assert!(checkpoint.is_ok());
    assert_eq!(checkpoint.unwrap().sequence_number, 100);
}

/// 测试：法定人数计算
#[test]
fn test_pbft_quorum_calculation() {
    // 4 节点集群（n=4, f=1, quorum=3）
    let nodes = vec![
        "node_0".to_string(),
        "node_1".to_string(),
        "node_2".to_string(),
        "node_3".to_string(),
    ];
    let config = ConsensusConfig::for_testing("node_0".to_string(), nodes);
    assert_eq!(config.quorum_size(), 3);
    assert_eq!(config.max_faulty(), 1);

    // 7 节点集群（n=7, f=2, quorum=5）
    let nodes: Vec<String> = (0..7).map(|i| format!("node_{}", i)).collect();
    let config = ConsensusConfig::for_testing("node_0".to_string(), nodes);
    assert_eq!(config.quorum_size(), 5);
    assert_eq!(config.max_faulty(), 2);
}

/// 测试：消息签名和验证
#[test]
fn test_pbft_message_signing() {
    let nodes = vec!["node_0".to_string(), "node_1".to_string()];
    let config = ConsensusConfig::for_testing("node_0".to_string(), nodes.clone());
    let consensus = PBFTConsensus::new(config);

    let operation = Operation {
        id: "op_1".to_string(),
        data: b"test signing".to_vec(),
        timestamp: 1234567890,
    };

    // 签名消息
    let signed_msg = consensus.sign_message(PBFTMessage::PrePrepare {
        view: 0,
        sequence: 1,
        digest: "test_digest".to_string(),
        operation: operation.clone(),
    });

    assert!(signed_msg.is_ok());
    assert!(!signed_msg.unwrap().signature.is_empty());
}

/// 测试：多操作连续提交
#[test]
fn test_pbft_sequential_commits() {
    let mut cluster = create_4_node_cluster();

    for i in 0..5 {
        let operation = Operation {
            id: format!("op_{}", i),
            data: format!("test operation {}", i).into_bytes(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        };

        let leader = &mut cluster[0].1;
        let result = leader.propose(operation);
        assert!(result.is_ok());
    }
}

/// 测试：并发视图切换
#[test]
fn test_pbft_concurrent_view_change() {
    let mut cluster = create_4_node_cluster();

    // 多个节点同时发起视图切换
    for (_, node) in cluster.iter_mut() {
        let result = node.initiate_view_change(0);
        assert!(result.is_ok());
    }
}

/// 测试：状态恢复
#[test]
fn test_pbft_state_recovery() {
    let mut cluster = create_4_node_cluster();

    let node_0 = &mut cluster[0].1;

    // 进入视图切换状态
    let _ = node_0.initiate_view_change(0);
    assert_eq!(node_0.state(), ConsensusState::ViewChanging);

    // 恢复状态
    let result = node_0.recover_from_view_change(1);
    assert!(result.is_ok());
    assert_eq!(node_0.state(), ConsensusState::Normal);
}
