//! KV Cache 性能基准测试
//!
//! **测试目标**：
//! - 测量 KV 读写延迟
//! - 测量多级缓存性能
//! - 测量并发性能
//! - 模拟真实 LLM 推理负载
//!
//! # 运行基准测试
//!
//! ```bash
//! # 运行所有基准测试
//! cargo bench --bench performance_bench
//!
//! # 运行特定基准测试
//! cargo bench --bench performance_bench kv_write
//! ```
//!
//! # 性能报告
//!
//! 基准测试结果会生成 HTML 报告，位于：
//! `target/criterion/report/index.html`

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use std::sync::Arc;
use rand::Rng;

use kv_cache::{KvCacheManager, KvSegment};

// ==================== KV Cache 性能基准 ====================

/// 基准测试：KV 写入性能（小数据）
fn bench_kv_write_small(c: &mut Criterion) {
    let mut manager = KvCacheManager::new();
    let value = vec![1u8; 100]; // 100 字节

    c.bench_function("kv_write_small_100b", |b| {
        b.iter(|| {
            let key = format!("key_{}", black_box(1));
            manager.write_kv(key, value.clone()).unwrap();
        })
    });
}

/// 基准测试：KV 写入性能（中等数据）
fn bench_kv_write_medium(c: &mut Criterion) {
    let mut manager = KvCacheManager::new();
    let value = vec![1u8; 1024]; // 1KB

    c.bench_function("kv_write_medium_1kb", |b| {
        b.iter(|| {
            let key = format!("key_{}", black_box(1));
            manager.write_kv(key, value.clone()).unwrap();
        })
    });
}

/// 基准测试：KV 写入性能（大数据）
fn bench_kv_write_large(c: &mut Criterion) {
    let mut manager = KvCacheManager::new();
    let value = vec![1u8; 10240]; // 10KB

    c.bench_function("kv_write_large_10kb", |b| {
        b.iter(|| {
            let key = format!("key_{}", black_box(1));
            manager.write_kv(key, value.clone()).unwrap();
        })
    });
}

/// 基准测试：KV 读取性能（命中热点缓存）
fn bench_kv_read_hot(c: &mut Criterion) {
    let mut manager = KvCacheManager::new();
    let key = "hot_key".to_string();
    let value = vec![1u8; 100];

    // 写入并多次访问使其成为热点
    manager.write_kv(key.clone(), value.clone()).unwrap();
    for _ in 0..15 {
        let _ = manager.read_kv(&key);
    }

    c.bench_function("kv_read_hot_cached", |b| {
        b.iter(|| {
            let _value = manager.read_kv(&key);
        })
    });
}

/// 基准测试：KV 读取性能（从分段读取）
fn bench_kv_read_cold(c: &mut Criterion) {
    let mut manager = KvCacheManager::new();
    let key = "cold_key".to_string();
    let value = vec![1u8; 100];

    // 只写入，不添加到热点缓存
    manager.write_kv(key.clone(), value).unwrap();

    c.bench_function("kv_read_from_segment", |b| {
        b.iter(|| {
            let _value = manager.read_kv(&key);
        })
    });
}

/// 基准测试：KV 读取性能（未命中）
fn bench_kv_read_miss(c: &mut Criterion) {
    let manager = KvCacheManager::new();

    c.bench_function("kv_read_miss", |b| {
        b.iter(|| {
            let _value = manager.read_kv("nonexistent_key");
        })
    });
}

// ==================== 分段管理基准 ====================

/// 基准测试：分段创建和密封
fn bench_segment_seal(c: &mut Criterion) {
    c.bench_function("segment_seal_10_shards", |b| {
        b.iter(|| {
            let mut segment = KvSegment::genesis();
            for i in 0..10 {
                segment.add_shard(
                    format!("key_{}", i),
                    vec![1u8; 100]
                ).unwrap();
            }
            // 注意：KvSegment 不再需要 seal() 方法
        })
    });
}

/// 基准测试：分段完整性验证
fn bench_segment_verify(c: &mut Criterion) {
    let mut segment = KvSegment::genesis();
    for i in 0..10 {
        segment.add_shard(
            format!("key_{}", i),
            vec![1u8; 100]
        ).unwrap();
    }

    c.bench_function("segment_verify_10_shards", |b| {
        b.iter(|| {
            // 验证每个 shard 的完整性
            for shard in segment.shards.values() {
                assert!(shard.verify_integrity());
            }
        })
    });
}

// ==================== 并发性能基准 ====================

/// 基准测试：并发读写（10 线程）
fn bench_concurrent_rw_10_threads(c: &mut Criterion) {
    let manager = Arc::new(KvCacheManager::new());
    let num_threads = 10;
    let num_ops = 100;

    c.bench_function("concurrent_rw_10_threads", |b| {
        b.iter(|| {
            let mut handles = vec![];

            for t in 0..num_threads {
                let manager = Arc::clone(&manager);
                let handle = std::thread::spawn(move || {
                    for i in 0..num_ops {
                        let key = format!("thread_{}_key_{}", t, i);
                        let value = vec![1u8; 100];

                        // 写入 - DashMap 内部处理并发，无需额外同步
                        manager.write_kv(key.clone(), value).unwrap();

                        // 读取
                        let _ = manager.read_kv(&key);
                    }
                });
                handles.push(handle);
            }

            for handle in handles {
                handle.join().unwrap();
            }
        })
    });
}

// ==================== 真实 LLM 推理负载基准 ====================

/// 模拟 LLM 推理场景：100 个并发请求，每个请求读取 10-100 个 KV chunks
fn bench_llm_inference_load(c: &mut Criterion) {
    let mut manager = KvCacheManager::new();

    // 预加载一些 KV 数据（模拟缓存的上下文）
    for i in 0..1000 {
        let key = format!("context_chunk_{}", i);
        let value = vec![1u8; 256]; // 256 字节，模拟一个 token 的 KV
        manager.write_kv(key, value).unwrap();
    }

    c.bench_function("llm_inference_100_requests", |b| {
        b.iter(|| {
            let mut rng = rand::thread_rng();

            // 模拟 100 个推理请求
            for _ in 0..100 {
                // 每个请求随机读取 10-100 个 chunks
                let num_chunks = rng.gen_range(10..=100);

                for i in 0..num_chunks {
                    let key = format!("context_chunk_{}", rng.gen_range(0..1000));
                    let _ = manager.read_kv(&key);
                }
            }
        })
    });
}

/// 模拟 LLM 推理场景：写入新生成的 KV 数据
fn bench_llm_kv_generation(c: &mut Criterion) {
    let mut manager = KvCacheManager::new();

    c.bench_function("llm_kv_generation_50_chunks", |b| {
        b.iter(|| {
            // 模拟生成 50 个新的 KV chunks
            for i in 0..50 {
                let key = format!("generated_chunk_{}", black_box(i));
                let value = vec![1u8; 256];
                manager.write_kv(key, value).unwrap();
            }

            // 注意：不再需要 seal_current_segment
        })
    });
}

/// 基准测试：完整性验证
fn bench_integrity_verification(c: &mut Criterion) {
    let mut manager = KvCacheManager::new();

    // 写入一些数据
    for i in 0..50 {
        let key = format!("key_{}", i);
        let value = vec![1u8; 100];
        manager.write_kv(key, value).unwrap();
    }

    c.bench_function("integrity_verification", |b| {
        b.iter(|| {
            // 验证所有 shard 的完整性
            if let Ok(segment) = manager.latest_segment() {
                for shard in segment.shards.values() {
                    assert!(shard.verify_integrity());
                }
            }
        })
    });
}

// ==================== Criterion 配置 ====================

criterion_group!(
    benches,
    bench_kv_write_small,
    bench_kv_write_medium,
    bench_kv_write_large,
    bench_kv_read_hot,
    bench_kv_read_cold,
    bench_kv_read_miss,
    bench_segment_seal,
    bench_segment_verify,
    bench_concurrent_rw_10_threads,
    bench_llm_inference_load,
    bench_llm_kv_generation,
    bench_integrity_verification,
);

criterion_main!(benches);
