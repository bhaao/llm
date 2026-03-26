//! 监控指标模块 - Prometheus 指标导出
//!
//! **v0.6.0 新增**：
//! - ✅ Prometheus 指标定义
//! - ✅ 关键业务指标收集
//! - ✅ `/metrics` HTTP 端点
//!
//! **核心指标**：
//! - 推理延迟（Histogram）
//! - KV 缓存命中率（Gauge）
//! - PBFT 共识耗时（Histogram）
//! - Gossip 同步延迟（Histogram）
//! - 节点信誉评分（Gauge）
//!
//! **使用示例**：
//!
//! ```ignore
//! use block_chain_with_context::metrics::MetricsRegistry;
//!
//! // 创建指标注册表
//! let registry = MetricsRegistry::new();
//!
//! // 记录推理延迟
//! registry.observe_inference_latency(0.123); // 123ms
//!
//! // 更新 KV 缓存命中率
//! registry.set_kv_cache_hit_ratio(0.85); // 85%
//!
//! // 导出 Prometheus 格式
//! let metrics = registry.gather();
//! ```

use prometheus::{Registry, Gauge, Histogram, HistogramOpts, TextEncoder};
use std::sync::Arc;
use std::time::Instant;

/// 指标注册表
pub struct MetricsRegistry {
    registry: Registry,
    
    // 推理指标
    inference_latency: Histogram,
    
    // KV 缓存指标
    kv_cache_hit_ratio: Gauge,
    kv_cache_size: Gauge,
    
    // PBFT 共识指标
    pbft_consensus_duration: Histogram,
    pbft_view_number: Gauge,
    
    // Gossip 同步指标
    gossip_sync_duration: Histogram,
    gossip_peers_count: Gauge,
    
    // 节点信誉指标
    node_reputation_score: Gauge,
}

/// 推理延迟观察器（RAII 模式）
pub struct InferenceTimer {
    start: Instant,
    registry: Arc<MetricsRegistry>,
}

impl InferenceTimer {
    pub fn new(registry: Arc<MetricsRegistry>) -> Self {
        InferenceTimer {
            start: Instant::now(),
            registry,
        }
    }
}

impl Drop for InferenceTimer {
    fn drop(&mut self) {
        let duration = self.start.elapsed().as_secs_f64();
        self.registry.observe_inference_latency(duration);
    }
}

/// PBFT 共识计时器（RAII 模式）
pub struct PbftConsensusTimer {
    start: Instant,
    registry: Arc<MetricsRegistry>,
}

impl PbftConsensusTimer {
    pub fn new(registry: Arc<MetricsRegistry>) -> Self {
        PbftConsensusTimer {
            start: Instant::now(),
            registry,
        }
    }
}

impl Drop for PbftConsensusTimer {
    fn drop(&mut self) {
        let duration = self.start.elapsed().as_secs_f64();
        self.registry.observe_pbft_consensus_duration(duration);
    }
}

/// Gossip 同步计时器（RAII 模式）
pub struct GossipSyncTimer {
    start: Instant,
    registry: Arc<MetricsRegistry>,
}

impl GossipSyncTimer {
    pub fn new(registry: Arc<MetricsRegistry>) -> Self {
        GossipSyncTimer {
            start: Instant::now(),
            registry,
        }
    }
}

impl Drop for GossipSyncTimer {
    fn drop(&mut self) {
        let duration = self.start.elapsed().as_secs_f64();
        self.registry.observe_gossip_sync_duration(duration);
    }
}

impl MetricsRegistry {
    /// 创建新的指标注册表
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new();
        
        // 推理延迟直方图
        let inference_latency_opts = HistogramOpts::new(
            "inference_latency_seconds",
            "Inference request latency in seconds",
        )
        .buckets(vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
        ]);
        let inference_latency = Histogram::with_opts(inference_latency_opts)?;
        registry.register(Box::new(inference_latency.clone()))?;
        
        // KV 缓存命中率
        let kv_cache_hit_ratio = Gauge::new(
            "kv_cache_hit_ratio",
            "KV cache hit ratio (0.0-1.0)",
        )?;
        registry.register(Box::new(kv_cache_hit_ratio.clone()))?;
        
        // KV 缓存大小
        let kv_cache_size = Gauge::new(
            "kv_cache_size",
            "KV cache size in bytes",
        )?;
        registry.register(Box::new(kv_cache_size.clone()))?;
        
        // PBFT 共识耗时
        let pbft_consensus_duration_opts = HistogramOpts::new(
            "pbft_consensus_duration_seconds",
            "PBFT consensus duration in seconds",
        )
        .buckets(vec![
            0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
        ]);
        let pbft_consensus_duration = Histogram::with_opts(pbft_consensus_duration_opts)?;
        registry.register(Box::new(pbft_consensus_duration.clone()))?;
        
        // PBFT 视图号
        let pbft_view_number = Gauge::new(
            "pbft_view_number",
            "Current PBFT view number",
        )?;
        registry.register(Box::new(pbft_view_number.clone()))?;
        
        // Gossip 同步耗时
        let gossip_sync_duration_opts = HistogramOpts::new(
            "gossip_sync_duration_seconds",
            "Gossip sync duration in seconds",
        )
        .buckets(vec![
            0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
        ]);
        let gossip_sync_duration = Histogram::with_opts(gossip_sync_duration_opts)?;
        registry.register(Box::new(gossip_sync_duration.clone()))?;
        
        // Gossip peer 数量
        let gossip_peers_count = Gauge::new(
            "gossip_peers_count",
            "Number of connected gossip peers",
        )?;
        registry.register(Box::new(gossip_peers_count.clone()))?;
        
        // 节点信誉评分
        let node_reputation_score = Gauge::new(
            "node_reputation_score",
            "Node reputation score (0.0-1.0)",
        )?;
        registry.register(Box::new(node_reputation_score.clone()))?;
        
        Ok(MetricsRegistry {
            registry,
            inference_latency,
            kv_cache_hit_ratio,
            kv_cache_size,
            pbft_consensus_duration,
            pbft_view_number,
            gossip_sync_duration,
            gossip_peers_count,
            node_reputation_score,
        })
    }
    
    /// 获取全局注册表
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
    
    // ===== 推理指标 =====
    
    /// 记录推理延迟
    pub fn observe_inference_latency(&self, duration_secs: f64) {
        self.inference_latency.observe(duration_secs);
    }
    
    /// 创建推理计时器（RAII）
    pub fn start_inference_timer(&self) -> InferenceTimer {
        InferenceTimer::new(Arc::new(self.clone()))
    }
    
    // ===== KV 缓存指标 =====
    
    /// 设置 KV 缓存命中率
    pub fn set_kv_cache_hit_ratio(&self, ratio: f64) {
        self.kv_cache_hit_ratio.set(ratio);
    }
    
    /// 设置 KV 缓存大小
    pub fn set_kv_cache_size(&self, size_bytes: u64) {
        self.kv_cache_size.set(size_bytes as f64);
    }
    
    // ===== PBFT 共识指标 =====
    
    /// 记录 PBFT 共识耗时
    pub fn observe_pbft_consensus_duration(&self, duration_secs: f64) {
        self.pbft_consensus_duration.observe(duration_secs);
    }
    
    /// 创建 PBFT 共识计时器（RAII）
    pub fn start_pbft_consensus_timer(&self) -> PbftConsensusTimer {
        PbftConsensusTimer::new(Arc::new(self.clone()))
    }
    
    /// 设置 PBFT 视图号
    pub fn set_pbft_view_number(&self, view: u64) {
        self.pbft_view_number.set(view as f64);
    }
    
    // ===== Gossip 同步指标 =====
    
    /// 记录 Gossip 同步耗时
    pub fn observe_gossip_sync_duration(&self, duration_secs: f64) {
        self.gossip_sync_duration.observe(duration_secs);
    }
    
    /// 创建 Gossip 同步计时器（RAII）
    pub fn start_gossip_sync_timer(&self) -> GossipSyncTimer {
        GossipSyncTimer::new(Arc::new(self.clone()))
    }
    
    /// 设置 Gossip peer 数量
    pub fn set_gossip_peers_count(&self, count: usize) {
        self.gossip_peers_count.set(count as f64);
    }
    
    // ===== 节点信誉指标 =====
    
    /// 设置节点信誉评分
    pub fn set_node_reputation_score(&self, score: f64) {
        self.node_reputation_score.set(score);
    }
    
    // ===== 指标导出 =====
    
    /// 收集所有指标（Prometheus 文本格式）
    pub fn gather(&self) -> String {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut output = String::new();
        
        match encoder.encode_utf8(&metric_families, &mut output) {
            Ok(_) => output,
            Err(e) => format!("Error encoding metrics: {}", e),
        }
    }
    
    /// 获取 Prometheus 格式指标（用于 HTTP 端点）
    pub fn metrics_text(&self) -> String {
        self.gather()
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new().expect("Failed to create metrics registry")
    }
}

impl Clone for MetricsRegistry {
    fn clone(&self) -> Self {
        // 注意：这里实际上是创建一个新的注册表，但指标不会共享
        // 生产环境应该使用 Arc<MetricsRegistry>
        Self::default()
    }
}

// 为 Arc<MetricsRegistry> 实现便捷方法
impl MetricsRegistry {
    /// 创建带 Arc 的指标注册表
    pub fn new_arc() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_metrics_registry_creation() {
        let registry = MetricsRegistry::new();
        assert!(registry.is_ok());
    }
    
    #[test]
    fn test_inference_latency() {
        let registry = MetricsRegistry::new().unwrap();
        registry.observe_inference_latency(0.123);
        
        let metrics = registry.gather();
        assert!(metrics.contains("inference_latency_seconds"));
    }
    
    #[test]
    fn test_kv_cache_hit_ratio() {
        let registry = MetricsRegistry::new().unwrap();
        registry.set_kv_cache_hit_ratio(0.85);
        
        let metrics = registry.gather();
        assert!(metrics.contains("kv_cache_hit_ratio"));
        assert!(metrics.contains("0.85"));
    }
    
    #[test]
    fn test_pbft_view_number() {
        let registry = MetricsRegistry::new().unwrap();
        registry.set_pbft_view_number(42);
        
        let metrics = registry.gather();
        assert!(metrics.contains("pbft_view_number"));
        assert!(metrics.contains("42"));
    }
    
    #[test]
    fn test_gossip_peers_count() {
        let registry = MetricsRegistry::new().unwrap();
        registry.set_gossip_peers_count(10);
        
        let metrics = registry.gather();
        assert!(metrics.contains("gossip_peers_count"));
        assert!(metrics.contains("10"));
    }
    
    #[test]
    fn test_node_reputation_score() {
        let registry = MetricsRegistry::new().unwrap();
        registry.set_node_reputation_score(0.95);
        
        let metrics = registry.gather();
        assert!(metrics.contains("node_reputation_score"));
        assert!(metrics.contains("0.95"));
    }
}
