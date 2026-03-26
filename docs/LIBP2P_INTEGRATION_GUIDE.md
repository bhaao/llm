# libp2p 集成实现指南

**版本**: v0.5.0  
**状态**: 原型  
**最后更新**: 2026-03-05

---

## 📋 概述

本项目提供两种 P2P 网络实现：

| 实现 | 状态 | 适用场景 | 说明 |
|------|------|---------|------|
| **gRPC (p2p_network.rs)** | ✅ 生产就绪 | 企业部署 | 基于 gRPC 的点对点通信，性能稳定 |
| **libp2p (简化版)** | ⚠️ 原型 | 技术验证 | 基于 libp2p 的 P2P 网络，去中心化 |
| **libp2p (完整版)** | 📅 计划中 | 去中心化部署 | GossipSub + mDNS 完整集成 |

---

## 🎯 当前实现状态

### gRPC 网络（生产就绪）✅

**文件**: `src/network/p2p_network.rs`

**已实现功能**:
- ✅ PBFT 共识消息广播
- ✅ Gossip 数据同步
- ✅ 节点发现和管理
- ✅ gRPC 服务端和客户端
- ✅ 完整的 Protobuf 消息定义

**使用示例**:

```rust
use block_chain_with_context::network::{
    NetworkConfig, NodeInfo, GrpcNetwork,
    ConsensusNetwork, GossipNetwork,
};

// 创建网络配置
let config = NetworkConfig {
    node_id: "node_1".to_string(),
    listen_addr: "http://127.0.0.1:50051".to_string(),
    initial_nodes: vec![],
    connect_timeout_ms: 5000,
    request_timeout_ms: 10000,
};

// 创建 gRPC 网络
let network = GrpcNetwork::new(config).await?;

// 作为 PBFT 共识网络使用
network.broadcast(&signed_pbft_message).await?;

// 作为 Gossip 网络使用
network.push_gossip(&gossip_message).await?;
```

**运行多节点测试**:

```bash
# 终端 1：启动节点 1
cargo run --example pbft_node -- --id node1 --port 50051

# 终端 2：启动节点 2
cargo run --example pbft_node -- --id node2 --port 50052 --peer http://127.0.0.1:50051

# 终端 3：启动节点 3
cargo run --example pbft_node -- --id node3 --port 50053 --peer http://127.0.0.1:50051
```

---

### libp2p 网络（简化版原型）⚠️

**文件**: `src/network/libp2p_network.rs`

**已实现功能**:
- ✅ libp2p 配置和 PeerId 生成
- ✅ mDNS 节点发现（stub）
- ✅ GossipSub 发布/订阅接口（stub）
- ✅ 与现有 gossip.rs 和 pbft.rs 集成

**当前限制**:
- ❌ Swarm 事件循环未启动（仅初始化配置）
- ❌ 消息发布/订阅是 stub 实现
- ❌ 无真实网络连接

**使用示例**:

```rust
use block_chain_with_context::network::{
    Libp2pConfig, Libp2pNetwork,
    Libp2pGossipNetwork, Libp2pConsensusNetwork,
};

// 创建配置
let config = Libp2pConfig::default();

// 创建网络（简化版，不启动真实 Swarm）
let network = Libp2pNetwork::new(config).await?;

// 包装为 Gossip 网络
let gossip = Libp2pGossipNetwork::new(Arc::new(network));

// 发布消息（当前是 stub）
gossip.gossip(&gossip_message).await?;
```

---

### libp2p 完整实现（计划中）📅

**目标**: 实现完整的 GossipSub + mDNS 集成

**需要实现的功能**:
1. Swarm 事件循环
2. GossipSub 消息发布/订阅
3. mDNS 节点发现和连接
4. 与现有 gossip.rs 和 pbft.rs 深度集成

**参考实现**:
- [libp2p gossipsub 示例](https://github.com/libp2p/rust-libp2p/tree/master/examples/gossipsub)
- [libp2p mdns 示例](https://github.com/libp2p/rust-libp2p/tree/master/examples/mdns)

---

## 🧪 多节点集成测试

### PBFT 集成测试（gRPC）

**文件**: `tests/pbft_integration_tests.rs`

**运行方式**:

```bash
cargo test --test pbft_integration_tests -- --nocapture
```

**测试场景**:
- 3 节点 PBFT 共识
- Leader 故障和视图切换
- 2f+1 签名收集

### Gossip 集成测试（gRPC）

**文件**: `tests/gossip_integration_tests.rs`

**运行方式**:

```bash
cargo test --test gossip_integration_tests -- --nocapture
```

**测试场景**:
- KV 分片跨节点同步
- Vector Clock 冲突解决
- Merkle Tree 完整性验证

---

## 📊 性能对比

| 指标 | gRPC | libp2p (简化) | libp2p (完整) |
|------|------|--------------|--------------|
| 延迟 | ~1ms | N/A | ~5-10ms |
| 吞吐量 | 高 | N/A | 中 |
| 去中心化 | ❌ | ✅ | ✅ |
| 生产就绪 | ✅ | ❌ | ❌ |
| 适用场景 | 企业 | 验证 | 去中心化 |

---

## 🔧 开发指南

### 添加新的网络实现

1. 实现 `ConsensusNetwork` trait
2. 实现 `GossipNetwork` trait
3. 在 `mod.rs` 中注册
4. 添加集成测试

### 完善 libp2p 集成

**步骤 1**: 启动 Swarm 事件循环

```rust
use libp2p::{Swarm, SwarmEvent};
use futures::stream::StreamExt;

let mut swarm = SwarmBuilder::with_existing_identity(keypair)
    .with_tokio()
    .with_tcp(
        tcp::Config::default(),
        |i| noise::Config::new(i),
        yamux::Config::default,
    )?
    .with_behaviour(|_| behaviour)?
    .build();

swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

loop {
    match swarm.select_next_some().await {
        SwarmEvent::NewListenAddr { address, .. } => {
            info!("Listening on {}", address);
        }
        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
            info!("Connected to {}", peer_id);
        }
        SwarmEvent::Behaviour(Libp2pBehaviourEvent::Gossipsub(GossipsubEvent::Message { .. })) => {
            // 处理 GossipSub 消息
        }
        _ => {}
    }
}
```

**步骤 2**: 实现 GossipSub 发布/订阅

```rust
use libp2p::gossipsub::{Topic, MessageId, GossipsubMessage};

// 订阅主题
let topic = Topic::new(StreamProtocol::new("pbft_consensus"));
swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

// 发布消息
let data = vec![1, 2, 3];
swarm.behaviour_mut().gossipsub.publish(topic, data)?;
```

**步骤 3**: 集成到现有系统

```rust
// 包装为 ConsensusNetwork
pub struct Libp2pConsensusNetwork {
    swarm: Arc<Mutex<Swarm<Libp2pBehaviour>>>,
}

#[tonic::async_trait]
impl ConsensusNetwork for Libp2pConsensusNetwork {
    async fn broadcast(&self, message: &SignedPbftMessage) -> Result<u32, NetworkError> {
        let mut swarm = self.swarm.lock().await;
        let topic = Topic::new(StreamProtocol::new("pbft_consensus"));
        swarm.behaviour_mut().gossipsub.publish(topic, message.encode_to_vec())?;
        Ok(1)
    }
    // ... 其他方法
}
```

---

## 📚 参考资料

1. **libp2p 官方文档**: https://docs.libp2p.io/
2. **rust-libp2p 示例**: https://github.com/libp2p/rust-libp2p/tree/master/examples
3. **GossipSub 规范**: https://github.com/libp2p/specs/blob/master/pubsub/gossipsub/README.md
4. **gRPC 官方文档**: https://grpc.io/docs/

---

## 🎯 下一步行动

### 短期（v0.5.0）

- [ ] 完善 libp2p Swarm 事件循环
- [ ] 实现 GossipSub 发布/订阅
- [ ] 添加 3 节点 libp2p 集成测试

### 中期（v0.6.0）

- [ ] 添加 Kademlia DHT 节点发现
- [ ] 实现节点身份验证
- [ ] 添加连接速率限制

### 长期（v1.0.0）

- [ ] 生产环境部署指南
- [ ] 性能优化和基准测试
- [ ] 安全加固（防 DoS、抗 Sybil）

---

**文档状态**: ✅ 完成  
**维护者**: Block Chain with Context Team
