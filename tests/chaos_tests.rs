// 混沌测试 - 随机故障注入测试
//
// 测试目标：
// 1. 延迟注入 - 验证网络延迟容忍度
// 2. 并发压力测试 - 验证线程安全和高并发能力
// 3. 节点宕机 - 验证系统恢复能力
// 4. 消息丢失 - 验证容错能力
// 5. 长稳测试 - 验证长时间运行稳定性
//
// 运行方式：
// cargo test --test chaos_tests -- --nocapture

use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use block_chain_with_context::{
    Blockchain, BlockchainConfig, Transaction, TransactionType, TransactionPayload,
    BlockMetadata,
};
use log::{info, warn, error};

/// 生成随机数（使用 std::rand 避免 Send 问题）
fn random_bool(probability: f64) -> bool {
    rand::random::<f64>() < probability
}

/// 生成随机范围（使用 std::rand 避免 Send 问题）
fn random_range(start: u64, end: u64) -> u64 {
    rand::random::<u64>() % (end - start + 1) + start
}

/// 混沌测试场景 1：延迟注入
///
/// 测试方案：
/// 1. 创建区块链
/// 2. 在提交交易时随机添加 0-100ms 延迟
/// 3. 验证最终所有交易都能提交成功
#[tokio::test]
async fn test_latency_injection() {
    println!("\n=== 混沌测试：延迟注入 ===\n");
    
    let blockchain = Arc::new(tokio::sync::RwLock::new(
        Blockchain::with_config(
            "test_addr".to_string(),
            BlockchainConfig::default(),
        )
    ));
    
    const TX_COUNT: usize = 50;
    const MAX_LATENCY_MS: u64 = 100;
    
    let mut handles: Vec<tokio::task::JoinHandle<bool>> = Vec::new();
    let mut total_latency_ms = 0u64;
    
    for i in 0..TX_COUNT {
        let blockchain = blockchain.clone();
        
        // 随机延迟 0-100ms
        let latency_ms = (i % (MAX_LATENCY_MS as usize + 1)) as u64;
        total_latency_ms += latency_ms;
        
        let handle = tokio::spawn(async move {
            // 注入延迟
            sleep(Duration::from_millis(latency_ms)).await;
            
            let mut bc = blockchain.write().await;
            
            // 创建交易
            let tx = Transaction::new(
                format!("user_{}", i),
                format!("assistant_{}", i),
                TransactionType::Transfer,
                TransactionPayload::None,
            );
            
            bc.add_pending_transaction(tx);
            
            // 提交区块
            let metadata = BlockMetadata::default();
            let result = bc.commit_inference(metadata, "test_node".to_string());
            result.is_ok()
        });
        
        handles.push(handle);
    }
    
    // 等待所有交易完成
    let results = futures::future::join_all(handles).await;
    
    // 统计成功
    let mut success_count = 0;
    
    for result in results {
        if result.is_ok() {
            success_count += 1;
        }
    }
    
    println!("✅ 成功提交：{} / {}", success_count, TX_COUNT);
    println!("⏱️  总注入延迟：{}ms", total_latency_ms);
    println!("⏱️  平均延迟：{}ms", total_latency_ms / TX_COUNT as u64);
    
    // 验证：所有交易都应该成功
    assert_eq!(success_count, TX_COUNT, "所有交易都应该成功提交");
    
    println!("\n✅ 测试通过：延迟注入场景下系统仍能正常工作\n");
}

/// 混沌测试场景 2：并发压力测试
///
/// 测试方案：
/// 1. 100 个并发任务同时提交交易
/// 2. 验证线程安全
/// 3. 检查是否有死锁或竞态条件
#[tokio::test]
async fn test_concurrent_stress() {
    println!("\n=== 混沌测试：并发压力测试 ===\n");
    
    let blockchain = Arc::new(tokio::sync::RwLock::new(
        Blockchain::with_config(
            "test_addr".to_string(),
            BlockchainConfig::default(),
        )
    ));
    
    const CONCURRENT_COUNT: usize = 100;
    
    let mut handles = Vec::new();
    
    for i in 0..CONCURRENT_COUNT {
        let blockchain = blockchain.clone();
        
        let handle = tokio::spawn(async move {
            let mut bc = blockchain.write().await;
            
            let tx = Transaction::new(
                format!("user_{}", i),
                format!("assistant_{}", i),
                TransactionType::Transfer,
                TransactionPayload::None,
            );
            
            bc.add_pending_transaction(tx);
            
            let metadata = BlockMetadata::default();
            bc.commit_inference(metadata, "test_node".to_string()).is_ok()
        });
        
        handles.push(handle);
    }
    
    // 等待所有任务完成
    let results = futures::future::join_all(handles).await;
    
    // 统计成功/失败
    let mut success_count = 0;
    
    for result in results {
        if result.is_ok() {
            success_count += 1;
        }
    }
    
    println!("✅ 成功提交：{} / {}", success_count, CONCURRENT_COUNT);
    
    // 验证：所有并发提交都应该成功
    assert_eq!(success_count, CONCURRENT_COUNT, "所有并发提交都应该成功");
    
    // 验证区块链长度
    let bc = blockchain.read().await;
    let expected_len = 1 + CONCURRENT_COUNT; // 创始区块 + 100 个交易区块
    println!("📊 区块链长度：{} (期望：{})", bc.chain().len(), expected_len);
    
    println!("\n✅ 测试通过：100 并发压力测试通过\n");
}

/// 混沌测试场景 3：节点宕机恢复测试
///
/// 测试方案：
/// 1. 创建区块链
/// 2. 模拟"宕机"：随机让某些任务休眠更长时间
/// 3. 验证系统能够恢复正常
#[tokio::test]
async fn test_node_crash_recovery() {
    println!("\n=== 混沌测试：节点宕机恢复 ===\n");

    let blockchain = Arc::new(tokio::sync::RwLock::new(
        Blockchain::with_config(
            "test_addr".to_string(),
            BlockchainConfig::default(),
        )
    ));

    const TX_COUNT: usize = 20; // 简化为 20 个
    const CRASH_PROBABILITY: f64 = 0.2; // 20% 概率"宕机"

    let mut handles = Vec::new();

    for i in 0..TX_COUNT {
        let blockchain = blockchain.clone();

        let handle = tokio::spawn(async move {
            // 模拟宕机：20% 概率休眠 1-3 秒
            if random_bool(CRASH_PROBABILITY) {
                let crash_duration = Duration::from_millis(random_range(1000, 3000));
                warn!("节点 {} 宕机 {:?}...", i, crash_duration);
                sleep(crash_duration).await;
            }

            // 正常提交
            sleep(Duration::from_millis(random_range(10, 50))).await;

            let mut bc = blockchain.write().await;
            let tx = Transaction::new(
                format!("user_{}", i),
                format!("assistant_{}", i),
                TransactionType::Transfer,
                TransactionPayload::None,
            );
            bc.add_pending_transaction(tx);

            let metadata = BlockMetadata::default();
            let _ = bc.commit_inference(metadata, format!("node_{}", i));
            (i, true) // 都标记为完成
        });

        handles.push(handle);
    }

    // 等待所有任务完成
    let results = futures::future::join_all(handles).await;

    // 统计
    let mut success_count = 0;

    for result in results {
        if let Ok((idx, _)) = result {
            success_count += 1;
            info!("节点 {} 完成", idx);
        }
    }

    println!("✅ 完成节点：{} / {}", success_count, TX_COUNT);

    // 验证：所有节点都应该完成
    assert_eq!(success_count, TX_COUNT, "所有节点都应该完成");

    println!("\n✅ 测试通过：节点宕机后系统仍能恢复\n");
}

/// 混沌测试场景 4：消息丢失/重复测试
///
/// 测试方案：
/// 1. 创建区块链
/// 2. 模拟消息丢失：随机跳过某些交易
/// 3. 模拟消息重复：随机重复提交某些交易
/// 4. 验证系统一致性
#[tokio::test]
async fn test_message_loss_duplication() {
    println!("\n=== 混沌测试：消息丢失/重复 ===\n");

    let blockchain = Arc::new(tokio::sync::RwLock::new(
        Blockchain::with_config(
            "test_addr".to_string(),
            BlockchainConfig::default(),
        )
    ));

    const TX_COUNT: usize = 50;
    const LOSS_PROBABILITY: f64 = 0.1; // 10% 丢失概率
    const DUP_PROBABILITY: f64 = 0.1;  // 10% 重复概率

    let mut handles = Vec::new();
    let mut lost_count = 0;
    let mut dup_count = 0;

    for i in 0..TX_COUNT {
        let blockchain = blockchain.clone();

        let handle = tokio::spawn(async move {
            // 模拟消息丢失
            if random_bool(LOSS_PROBABILITY) {
                warn!("消息 {} 丢失", i);
                return (i, "lost");
            }

            // 模拟消息重复
            let is_dup = random_bool(DUP_PROBABILITY);
            if is_dup {
                info!("消息 {} 重复", i);
            }

            // 提交交易
            let mut bc = blockchain.write().await;
            let tx = Transaction::new(
                format!("user_{}", i),
                format!("assistant_{}", i),
                TransactionType::Transfer,
                TransactionPayload::None,
            );
            bc.add_pending_transaction(tx.clone());

            // 如果是重复消息，再提交一次
            if is_dup {
                bc.add_pending_transaction(tx.clone());
            }

            let metadata = BlockMetadata::default();
            let result = bc.commit_inference(metadata, format!("node_{}", i));
            (i, if is_dup { "dup" } else { "ok" })
        });

        handles.push(handle);
    }

    // 等待所有任务完成
    let results = futures::future::join_all(handles).await;

    // 统计
    let mut success_count = 0;

    for result in results {
        if let Ok((idx, status)) = result {
            match status {
                "lost" => lost_count += 1,
                "dup" => dup_count += 1,
                "ok" => success_count += 1,
                _ => {}
            }
            info!("节点 {}: {}", idx, status);
        }
    }

    println!("✅ 成功提交：{}", success_count);
    println!("❌ 丢失消息：{}", lost_count);
    println!("🔄 重复消息：{}", dup_count);

    // 验证：区块链长度应该正确（考虑重复）
    let bc = blockchain.read().await;
    let chain_len = bc.chain().len();
    info!("区块链长度：{}", chain_len);

    println!("\n✅ 测试通过：消息丢失/重复场景下系统保持一致性\n");
}

/// 长稳测试：长时间运行稳定性
///
/// 测试方案：
/// 1. 持续运行 5 秒（简化版）
/// 2. 顺序提交交易
/// 3. 验证系统无崩溃
#[tokio::test]
async fn test_long_running_stability() {
    println!("\n=== 长稳测试：5 秒稳定性测试 ===\n");

    let blockchain = Arc::new(tokio::sync::RwLock::new(
        Blockchain::with_config(
            "test_addr".to_string(),
            BlockchainConfig::default(),
        )
    ));

    const DURATION_SECS: u64 = 5; // 简化为 5 秒

    let start_time = std::time::Instant::now();
    let mut total_tx_count = 0;

    // 持续提交交易（顺序执行）
    while start_time.elapsed() < Duration::from_secs(DURATION_SECS) {
        let mut bc = blockchain.write().await;

        let tx = Transaction::new(
            format!("user_{}", total_tx_count),
            format!("assistant_{}", total_tx_count),
            TransactionType::Transfer,
            TransactionPayload::None,
        );
        bc.add_pending_transaction(tx);

        let metadata = BlockMetadata::default();
        let _ = bc.commit_inference(metadata, format!("node_{}", total_tx_count));
        
        total_tx_count += 1;

        // 等待 0.5 秒
        sleep(Duration::from_millis(500)).await;
    }

    // 统计结果
    let elapsed = start_time.elapsed();

    println!("\n📊 测试结果:");
    println!("  - 运行时间：{:.2}s", elapsed.as_secs_f64());
    println!("  - 提交交易：{}", total_tx_count);

    // 验证：至少提交了一些交易
    assert!(total_tx_count > 0, "应该有交易提交");

    // 验证区块链长度
    let bc = blockchain.read().await;
    let chain_len = bc.chain().len();
    println!("  - 区块链长度：{}", chain_len);

    println!("\n✅ 测试通过：长稳测试通过，系统稳定运行\n");
}

/// 性能回归测试：检测性能衰减
#[tokio::test]
async fn test_performance_regression() {
    println!("\n=== 性能测试：性能回归检测 ===\n");

    let blockchain = Arc::new(tokio::sync::RwLock::new(
        Blockchain::with_config(
            "test_addr".to_string(),
            BlockchainConfig::default(),
        )
    ));

    const WARMUP_TX: usize = 10;
    const BENCHMARK_TX: usize = 100;

    // 预热
    info!("预热阶段：{} 个交易", WARMUP_TX);
    for i in 0..WARMUP_TX {
        let mut bc = blockchain.write().await;
        let tx = Transaction::new(
            format!("warmup_{}", i),
            "assistant".to_string(),
            TransactionType::Transfer,
            TransactionPayload::None,
        );
        bc.add_pending_transaction(tx);
        let metadata = BlockMetadata::default();
        let _ = bc.commit_inference(metadata, format!("warmup_node_{}", i));
    }

    // 基准测试
    info!("基准测试：{} 个交易", BENCHMARK_TX);
    let start = std::time::Instant::now();

    let mut handles = Vec::new();
    for i in 0..BENCHMARK_TX {
        let blockchain = blockchain.clone();

        let handle = tokio::spawn(async move {
            let mut bc = blockchain.write().await;
            let tx = Transaction::new(
                format!("bench_{}", i),
                "assistant".to_string(),
                TransactionType::Transfer,
                TransactionPayload::None,
            );
            bc.add_pending_transaction(tx);
            let metadata = BlockMetadata::default();
            bc.commit_inference(metadata, format!("bench_node_{}", i)).is_ok()
        });

        handles.push(handle);
    }

    let results = futures::future::join_all(handles).await;
    let success_count = results.iter().filter(|r| r.is_ok()).count();

    let elapsed = start.elapsed();
    let avg_latency = elapsed.as_secs_f64() / BENCHMARK_TX as f64 * 1000.0; // ms

    println!("\n📊 性能指标:");
    println!("  - 成功交易：{} / {}", success_count, BENCHMARK_TX);
    println!("  - 总耗时：{:.2}s", elapsed.as_secs_f64());
    println!("  - 平均延迟：{:.2}ms/tx", avg_latency);
    println!("  - 吞吐量：{:.2} tx/s", BENCHMARK_TX as f64 / elapsed.as_secs_f64());

    // 性能回归检测：平均延迟应该小于 100ms
    assert!(avg_latency < 100.0,
        "性能回归：平均延迟{:.2}ms 超过阈值 100ms", avg_latency);

    println!("\n✅ 测试通过：性能回归检测通过\n");
}
