//! 监控指标模块 - Prometheus 指标导出
//!
//! 提供 KV 缓存系统的核心监控指标
//!
//! **核心指标**：
//! - 推理延迟（Histogram）
//! - KV 缓存命中率（Gauge）
//! - KV 缓存大小（Gauge）
//! - 读取/写入延迟（Histogram）
//! - 压缩率（Gauge）
//!
//! **使用示例**：
//!
//! ```rust,no_run
//! use kv_cache::metrics::MetricsRegistry;
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

#[cfg(feature = "metrics")]
use prometheus::{Registry, Gauge, Histogram, HistogramOpts, TextEncoder};
use std::sync::Arc;
use std::time::Instant;

/// 指标注册表
#[derive(Clone)]
pub struct MetricsRegistry {
    #[cfg(feature = "metrics")]
    registry: Registry,

    // 推理指标
    #[cfg(feature = "metrics")]
    inference_latency: Histogram,

    // KV 缓存指标
    #[cfg(feature = "metrics")]
    kv_cache_hit_ratio: Gauge,
    #[cfg(feature = "metrics")]
    kv_cache_size: Gauge,

    // 读写指标
    #[cfg(feature = "metrics")]
    read_latency: Histogram,
    #[cfg(feature = "metrics")]
    write_latency: Histogram,

    // 压缩指标
    #[cfg(feature = "metrics")]
    compression_ratio: Gauge,
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

/// 读延迟观察器（RAII 模式）
pub struct ReadTimer {
    start: Instant,
    registry: Arc<MetricsRegistry>,
}

impl ReadTimer {
    pub fn new(registry: Arc<MetricsRegistry>) -> Self {
        ReadTimer {
            start: Instant::now(),
            registry,
        }
    }
}

impl Drop for ReadTimer {
    fn drop(&mut self) {
        let duration = self.start.elapsed().as_secs_f64();
        self.registry.observe_read_latency(duration);
    }
}

/// 写延迟观察器（RAII 模式）
pub struct WriteTimer {
    start: Instant,
    registry: Arc<MetricsRegistry>,
}

impl WriteTimer {
    pub fn new(registry: Arc<MetricsRegistry>) -> Self {
        WriteTimer {
            start: Instant::now(),
            registry,
        }
    }
}

impl Drop for WriteTimer {
    fn drop(&mut self) {
        let duration = self.start.elapsed().as_secs_f64();
        self.registry.observe_write_latency(duration);
    }
}

impl MetricsRegistry {
    /// 创建新的指标注册表
    pub fn new() -> Self {
        #[cfg(feature = "metrics")]
        {
            let registry = Registry::new();

            // 推理延迟
            let inference_latency = Histogram::with_opts(
                HistogramOpts::new("inference_latency_seconds", "Inference latency in seconds")
                    .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]),
            )
            .unwrap();
            registry.register(Box::new(inference_latency.clone())).unwrap();

            // KV 缓存命中率
            let kv_cache_hit_ratio = Gauge::new("kv_cache_hit_ratio", "KV cache hit ratio (0-1)").unwrap();
            registry.register(Box::new(kv_cache_hit_ratio.clone())).unwrap();

            // KV 缓存大小
            let kv_cache_size = Gauge::new("kv_cache_size_bytes", "KV cache size in bytes").unwrap();
            registry.register(Box::new(kv_cache_size.clone())).unwrap();

            // 读延迟
            let read_latency = Histogram::with_opts(
                HistogramOpts::new("read_latency_seconds", "Read latency in seconds")
                    .buckets(vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1]),
            )
            .unwrap();
            registry.register(Box::new(read_latency.clone())).unwrap();

            // 写延迟
            let write_latency = Histogram::with_opts(
                HistogramOpts::new("write_latency_seconds", "Write latency in seconds")
                    .buckets(vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1]),
            )
            .unwrap();
            registry.register(Box::new(write_latency.clone())).unwrap();

            // 压缩率
            let compression_ratio = Gauge::new("compression_ratio", "Compression ratio (compressed/original)")
                .unwrap();
            registry.register(Box::new(compression_ratio.clone())).unwrap();

            MetricsRegistry {
                registry,
                inference_latency,
                kv_cache_hit_ratio,
                kv_cache_size,
                read_latency,
                write_latency,
                compression_ratio,
            }
        }

        #[cfg(not(feature = "metrics"))]
        {
            MetricsRegistry {}
        }
    }

    /// 观察推理延迟
    pub fn observe_inference_latency(&self, duration_secs: f64) {
        #[cfg(feature = "metrics")]
        {
            self.inference_latency.observe(duration_secs);
        }
        #[cfg(not(feature = "metrics"))]
        {
            let _ = duration_secs;
        }
    }

    /// 设置 KV 缓存命中率
    pub fn set_kv_cache_hit_ratio(&self, ratio: f64) {
        #[cfg(feature = "metrics")]
        {
            self.kv_cache_hit_ratio.set(ratio);
        }
        #[cfg(not(feature = "metrics"))]
        {
            let _ = ratio;
        }
    }

    /// 设置 KV 缓存大小
    pub fn set_kv_cache_size(&self, size_bytes: f64) {
        #[cfg(feature = "metrics")]
        {
            self.kv_cache_size.set(size_bytes);
        }
        #[cfg(not(feature = "metrics"))]
        {
            let _ = size_bytes;
        }
    }

    /// 观察读延迟
    pub fn observe_read_latency(&self, duration_secs: f64) {
        #[cfg(feature = "metrics")]
        {
            self.read_latency.observe(duration_secs);
        }
        #[cfg(not(feature = "metrics"))]
        {
            let _ = duration_secs;
        }
    }

    /// 观察写延迟
    pub fn observe_write_latency(&self, duration_secs: f64) {
        #[cfg(feature = "metrics")]
        {
            self.write_latency.observe(duration_secs);
        }
        #[cfg(not(feature = "metrics"))]
        {
            let _ = duration_secs;
        }
    }

    /// 设置压缩率
    pub fn set_compression_ratio(&self, ratio: f64) {
        #[cfg(feature = "metrics")]
        {
            self.compression_ratio.set(ratio);
        }
        #[cfg(not(feature = "metrics"))]
        {
            let _ = ratio;
        }
    }

    /// 导出 Prometheus 格式
    pub fn gather(&self) -> String {
        #[cfg(feature = "metrics")]
        {
            let encoder = TextEncoder::new();
            let metric_families = self.registry.gather();
            let mut output = String::new();
            encoder.encode_utf8(&metric_families, &mut output).unwrap();
            output
        }

        #[cfg(not(feature = "metrics"))]
        {
            String::new()
        }
    }

    /// 创建推理定时器
    pub fn start_inference_timer(&self) -> InferenceTimer {
        InferenceTimer::new(Arc::new(self.clone()))
    }

    /// 创建读定时器
    pub fn start_read_timer(&self) -> ReadTimer {
        ReadTimer::new(Arc::new(self.clone()))
    }

    /// 创建写定时器
    pub fn start_write_timer(&self) -> WriteTimer {
        WriteTimer::new(Arc::new(self.clone()))
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_registry_creation() {
        let registry = MetricsRegistry::new();
        // 基本测试，确保创建成功
        drop(registry);
    }

    #[test]
    fn test_inference_timer() {
        let registry = MetricsRegistry::new();
        {
            let _timer = registry.start_inference_timer();
            // 定时器作用域内
        }
        // 定时器已 drop，自动记录指标
    }

    #[test]
    fn test_gather_empty_metrics() {
        let registry = MetricsRegistry::new();
        let output = registry.gather();
        // 在没有启用 metrics feature 时返回空字符串
        #[cfg(feature = "metrics")]
        assert!(!output.is_empty());
        #[cfg(not(feature = "metrics"))]
        assert!(output.is_empty());
    }
}
