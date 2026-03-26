//! Gossip 同步协议集成测试 - 多节点数据同步
//!
//! 测试场景：
//! 1. Vector Clock 冲突解决
//! 2. Merkle Tree 完整性验证
//! 3. 多节点 Gossip 同步
//! 4. 网络分区模拟

use block_chain_with_context::gossip::{
    GossipProtocol, GossipConfig, KVShard, VectorClock,
    ReplicaInfo, SyncState, MergeResult,
};
use std::collections::HashMap;
use std::time::Duration;

/// 创建测试 Gossip 节点
fn create_gossip_node(node_id: &str, address: &str) -> GossipProtocol {
    let config = GossipConfig {
        node_id: node_id.to_string(),
        address: address.to_string(),
        gossip_interval_ms: 100,
        fanout: 2,
        timeout_secs: 30,
    };
    GossipProtocol::new(config)
}

/// 测试：Vector Clock 基本操作
#[test]
fn test_vector_clock_basic_operations() {
    let mut vc = VectorClock::new();
    
    // 递增测试
    vc.increment("node_1");
    vc.increment("node_1");
    vc.increment("node_2");
    
    assert_eq!(vc.get("node_1"), 2);
    assert_eq!(vc.get("node_2"), 1);
    assert_eq!(vc.get("node_3"), 0);
}

/// 测试：Vector Clock 合并
#[test]
fn test_vector_clock_merge() {
    let mut vc1 = VectorClock::new();
    vc1.increment("node_1");
    vc1.increment("node_2");
    
    let mut vc2 = VectorClock::new();
    vc2.increment("node_2");
    vc2.increment("node_2");
    vc2.increment("node_3");
    
    vc1.merge(&vc2);
    
    assert_eq!(vc1.get("node_1"), 1);
    assert_eq!(vc1.get("node_2"), 2);
    assert_eq!(vc1.get("node_3"), 1);
}

/// 测试：Vector Clock 冲突检测
#[test]
fn test_vector_clock_conflict_detection() {
    let mut vc1 = VectorClock::new();
    vc1.increment("node_1");
    
    let mut vc2 = VectorClock::new();
    vc2.increment("node_2");
    
    // 并发冲突
    let ordering = vc1.compare(&vc2);
    assert_eq!(ordering, block_chain_with_context::gossip::ClockOrdering::Concurrent);
    
    // 因果关系
    let mut vc3 = VectorClock::new();
    vc3.increment("node_1");
    vc3.increment("node_2");
    
    let ordering = vc3.compare(&vc1);
    assert_eq!(ordering, block_chain_with_context::gossip::ClockOrdering::Greater);
}

/// 测试：KV 分片创建和更新
#[test]
fn test_kv_shard_creation_and_update() {
    let mut shard = KVShard::new("shard_1".to_string(), "node_1".to_string());
    
    // 初始状态
    assert_eq!(shard.shard_id, "shard_1");
    assert!(shard.data.is_empty());
    assert_eq!(shard.version.get("node_1"), 1);
    
    // 写入数据
    shard.set("key_1".to_string(), b"value_1".to_vec(), "node_1");
    assert_eq!(shard.get("key_1"), Some(&b"value_1".to_vec()));
    assert_eq!(shard.version.get("node_1"), 2);
}

/// 测试：KV 分片合并（覆盖场景）
#[test]
fn test_kv_shard_merge_overwrite() {
    let mut shard1 = KVShard::new("shard_1".to_string(), "node_1".to_string());
    shard1.set("key_1".to_string(), b"value_1".to_vec(), "node_1");
    
    let mut shard2 = KVShard::new("shard_1".to_string(), "node_2".to_string());
    shard2.set("key_2".to_string(), b"value_2".to_vec(), "node_2");
    
    // 让 shard2 的版本明确大于 shard1
    shard2.version = shard1.version.clone();
    shard2.version.increment("node_2");
    
    let result = shard1.merge(&shard2);
    assert_eq!(result, MergeResult::Overwritten);
    assert_eq!(shard1.get("key_2"), Some(&b"value_2".to_vec()));
}

/// 测试：Merkle Tree 完整性验证
#[test]
fn test_merkle_tree_integrity() {
    let mut shard = KVShard::new("shard_1".to_string(), "node_1".to_string());
    
    // 初始验证
    assert!(shard.verify_integrity());
    
    // 添加数据后验证
    shard.set("key_1".to_string(), b"value_1".to_vec(), "node_1");
    shard.set("key_2".to_string(), b"value_2".to_vec(), "node_1");
    assert!(shard.verify_integrity());
    
    // 篡改数据后验证失败
    shard.data.insert("key_3".to_string(), b"value_3".to_vec());
    assert!(!shard.verify_integrity());
}

/// 测试：Gossip 协议创建
#[test]
fn test_gossip_protocol_creation() {
    let config = GossipConfig::default();
    let gossip = GossipProtocol::new(config);
    
    let stats = gossip.stats();
    assert_eq!(stats.total_peers, 0);
    assert_eq!(stats.total_shards, 0);
}

/// 测试：Gossip Peer 选择
#[test]
fn test_gossip_peer_selection() {
    let config = GossipConfig {
        fanout: 2,
        ..Default::default()
    };
    let mut gossip = GossipProtocol::new(config);
    
    // 添加 5 个 peer
    for i in 0..5 {
        gossip.add_peer(ReplicaInfo::new(
            format!("node_{}", i),
            format!("localhost:{}", 8080 + i),
        ));
    }
    
    let peers = gossip.select_random_peers();
    assert!(peers.len() <= 2);
}

/// 测试：多节点 Gossip 同步
#[tokio::test]
async fn test_multi_node_gossip_sync() {
    // 创建 3 个节点
    let mut node1 = create_gossip_node("node_1", "localhost:8081");
    let mut node2 = create_gossip_node("node_2", "localhost:8082");
    let mut node3 = create_gossip_node("node_3", "localhost:8083");
    
    // 添加 KV 分片到 node1
    let mut shard = KVShard::new("shard_1".to_string(), "node_1".to_string());
    shard.set("key_1".to_string(), b"value_1".to_vec(), "node_1");
    node1.add_shard(shard.clone());
    
    // 添加 peer 关系
    node1.add_peer(ReplicaInfo::new("node_2".to_string(), "localhost:8082".to_string()));
    node1.add_peer(ReplicaInfo::new("node_3".to_string(), "localhost:8083".to_string()));
    
    // 同步分片
    let result = node1.sync_shard("shard_1").await;
    assert!(result.is_ok());
    
    // 验证同步结果（由于是内存模拟，主要验证流程）
    let stats = node1.stats();
    assert_eq!(stats.total_peers, 2);
    assert_eq!(stats.total_shards, 1);
}

/// 测试：Gossip 同步状态跟踪
#[test]
fn test_gossip_sync_state_tracking() {
    let mut sync_state = SyncState::new();
    
    // 添加副本
    sync_state.add_replica(ReplicaInfo::new(
        "node_2".to_string(),
        "localhost:8082".to_string(),
    ));
    
    // 标记待同步
    sync_state.mark_pending_sync("node_2".to_string());
    assert!(sync_state.needs_sync());
    
    // 标记同步完成
    sync_state.mark_sync_complete("node_2");
    assert!(!sync_state.needs_sync());
    assert!(sync_state.last_sync_time.is_some());
}

/// 测试：Gossip 同步失败处理
#[test]
fn test_gossip_sync_failure_handling() {
    let mut sync_state = SyncState::new();
    
    // 模拟多次同步失败
    for i in 1..=3 {
        sync_state.mark_sync_failure();
        assert_eq!(sync_state.sync_failures, i);
    }
}

/// 测试：副本超时检测
#[test]
fn test_replica_timeout_detection() {
    let mut replica = ReplicaInfo::new("node_1".to_string(), "localhost:8081".to_string());
    
    // 初始在线
    assert!(replica.is_online);
    assert!(!replica.is_timeout(30));
    
    // 模拟超时（手动设置最后心跳时间）
    replica.last_heartbeat = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() - 60;
    
    assert!(replica.is_timeout(30));
}

/// 测试：Gossip 统计信息
#[test]
fn test_gossip_stats() {
    let config = GossipConfig {
        node_id: "node_1".to_string(),
        address: "localhost:8081".to_string(),
        gossip_interval_ms: 100,
        fanout: 3,
        timeout_secs: 30,
    };
    
    let mut gossip = GossipProtocol::new(config);
    
    // 添加分片
    let shard = KVShard::new("shard_1".to_string(), "node_1".to_string());
    gossip.add_shard(shard);
    
    // 添加 peer
    gossip.add_peer(ReplicaInfo::new("node_2".to_string(), "localhost:8082".to_string()));
    gossip.add_peer(ReplicaInfo::new("node_3".to_string(), "localhost:8083".to_string()));
    
    let stats = gossip.stats();
    assert_eq!(stats.node_id, "node_1");
    assert_eq!(stats.total_shards, 1);
    assert_eq!(stats.total_peers, 2);
}

/// 测试：并发 Vector Clock 比较
#[test]
fn test_concurrent_vector_clocks() {
    let mut vc1 = VectorClock::new();
    vc1.increment("node_1");
    vc1.increment("node_2");
    
    let mut vc2 = VectorClock::new();
    vc2.increment("node_2");
    vc2.increment("node_3");
    
    // 这两个 Vector Clock 是并发的
    let ordering1 = vc1.compare(&vc2);
    let ordering2 = vc2.compare(&vc1);
    
    assert_eq!(ordering1, block_chain_with_context::gossip::ClockOrdering::Concurrent);
    assert_eq!(ordering2, block_chain_with_context::gossip::ClockOrdering::Concurrent);
}

/// 测试：KV 分片数据完整性
#[tokio::test]
async fn test_kv_shard_data_integrity() {
    let mut shard = KVShard::new("shard_1".to_string(), "node_1".to_string());
    
    // 添加多个键值对
    for i in 0..10 {
        shard.set(
            format!("key_{}", i),
            format!("value_{}", i).into_bytes(),
            "node_1",
        );
    }
    
    // 验证完整性
    assert!(shard.verify_integrity());
    
    // 验证数据
    for i in 0..10 {
        let expected = format!("value_{}", i).into_bytes();
        assert_eq!(shard.get(&format!("key_{}", i)), Some(&expected));
    }
}
