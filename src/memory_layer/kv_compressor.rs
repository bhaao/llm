//! KV Chunk 压缩模块 - zstd 压缩
//!
//! **核心功能**：
//! - zstd 压缩/解压缩
//! - 压缩率统计
//! - 压缩质量评估
//!
//! # 压缩算法对比
//!
//! | 算法 | 压缩率 | 速度 | 内存占用 | 适用场景 |
//! |-----|-------|------|---------|---------|
//! | zstd | 60-80% | 快 | 中 | 通用 KV 压缩 |
//! | lz4 | 40-60% | 最快 | 低 | 低延迟场景 |
//! | gzip | 70-90% | 慢 | 高 | 归档存储 |
//!
//! # 使用示例
//!
//! ```
//! use block_chain_with_context::memory_layer::kv_compressor::KvChunkCompressor;
//!
//! let compressor = KvChunkCompressor::new(3);
//! let data = vec![1u8; 1000];
//!
//! // 压缩
//! let compressed = compressor.compress(&data).unwrap();
//!
//! // 解压缩
//! let decompressed = compressor.decompress(&compressed).unwrap();
//!
//! assert_eq!(data, decompressed);
//! ```

use zstd::{encode_all, decode_all};
use anyhow::{Result, Context};

/// KV Chunk 压缩器
#[derive(Debug, Clone)]
pub struct KvChunkCompressor {
    /// zstd 压缩级别 (1-22, 推荐 3-6)
    compression_level: i32,
}

impl KvChunkCompressor {
    /// 创建新的压缩器
    ///
    /// # 参数
    ///
    /// * `compression_level` - 压缩级别 (1-22)
    ///
    /// # 返回
    ///
    /// * `Self` - 新的压缩器实例
    pub fn new(compression_level: i32) -> Self {
        KvChunkCompressor {
            compression_level: compression_level.clamp(1, 22),
        }
    }

    /// 创建默认压缩器 (级别 3)
    pub fn default() -> Self {
        Self::new(3)
    }

    /// 创建快速压缩器 (级别 1, 最快速度)
    pub fn fast() -> Self {
        Self::new(1)
    }

    /// 创建高压缩率压缩器 (级别 6, 更高压缩率)
    pub fn high_compression() -> Self {
        Self::new(6)
    }

    /// 压缩数据
    ///
    /// # 参数
    ///
    /// * `data` - 原始数据
    ///
    /// # 返回
    ///
    /// * `Result<Vec<u8>>` - 压缩后的数据或错误
    pub fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.is_empty() {
            return Ok(Vec::new());
        }

        encode_all(data, self.compression_level)
            .context("Compression failed")
    }

    /// 解压缩数据
    ///
    /// # 参数
    ///
    /// * `compressed` - 压缩后的数据
    ///
    /// # 返回
    ///
    /// * `Result<Vec<u8>>` - 解压缩后的数据或错误
    pub fn decompress(&self, compressed: &[u8]) -> Result<Vec<u8>> {
        if compressed.is_empty() {
            return Ok(Vec::new());
        }

        decode_all(compressed)
            .context("Decompression failed")
    }

    /// 计算压缩率
    ///
    /// # 参数
    ///
    /// * `original_size` - 原始大小 (字节)
    /// * `compressed_size` - 压缩后大小 (字节)
    ///
    /// # 返回
    ///
    /// * `f64` - 压缩率 (压缩后大小/原始大小)
    pub fn compression_ratio(original_size: usize, compressed_size: usize) -> f64 {
        if original_size == 0 {
            1.0
        } else {
            compressed_size as f64 / original_size as f64
        }
    }

    /// 计算压缩率百分比
    ///
    /// # 参数
    ///
    /// * `original_size` - 原始大小 (字节)
    /// * `compressed_size` - 压缩后大小 (字节)
    ///
    /// # 返回
    ///
    /// * `f64` - 压缩率百分比 (0-100)
    pub fn compression_ratio_percent(original_size: usize, compressed_size: usize) -> f64 {
        Self::compression_ratio(original_size, compressed_size) * 100.0
    }

    /// 获取压缩级别
    pub fn compression_level(&self) -> i32 {
        self.compression_level
    }

    /// 压缩并返回统计信息
    ///
    /// # 参数
    ///
    /// * `data` - 原始数据
    ///
    /// # 返回
    ///
    /// * `Result<CompressionStats>` - 压缩统计信息或错误
    pub fn compress_with_stats(&self, data: &[u8]) -> Result<CompressionStats> {
        let original_size = data.len();
        let compressed = self.compress(data)?;
        let compressed_size = compressed.len();

        Ok(CompressionStats {
            original_size,
            compressed_size,
            compression_ratio: Self::compression_ratio(original_size, compressed_size),
            compressed_data: compressed,
        })
    }
}

/// 压缩统计信息
#[derive(Debug, Clone)]
pub struct CompressionStats {
    /// 原始大小 (字节)
    pub original_size: usize,
    /// 压缩后大小 (字节)
    pub compressed_size: usize,
    /// 压缩率
    pub compression_ratio: f64,
    /// 压缩后的数据
    pub compressed_data: Vec<u8>,
}

impl CompressionStats {
    /// 获取压缩率百分比
    pub fn ratio_percent(&self) -> f64 {
        self.compression_ratio * 100.0
    }

    /// 获取空间节省百分比
    pub fn space_saved_percent(&self) -> f64 {
        (1.0 - self.compression_ratio) * 100.0
    }

    /// 获取空间节省字节数
    pub fn space_saved_bytes(&self) -> usize {
        self.original_size.saturating_sub(self.compressed_size)
    }
}

/// 验证压缩/解压缩的正确性
///
/// # 参数
///
/// * `compressor` - 压缩器
/// * `original` - 原始数据
///
/// # 返回
///
/// * `Result<bool>` - 是否正确或错误
pub fn verify_compression(compressor: &KvChunkCompressor, original: &[u8]) -> Result<bool> {
    let compressed = compressor.compress(original)?;
    let decompressed = compressor.decompress(&compressed)?;
    Ok(original == decompressed)
}

/// 批量压缩统计
#[derive(Debug, Clone, Default)]
pub struct BatchCompressionStats {
    /// 总原始大小
    pub total_original_size: usize,
    /// 总压缩后大小
    pub total_compressed_size: usize,
    /// 平均压缩率
    pub average_ratio: f64,
    /// 压缩的 chunk 数量
    pub chunk_count: usize,
}

impl BatchCompressionStats {
    /// 创建新的统计
    pub fn new() -> Self {
        Self::default()
    }

    /// 从多个统计中计算
    ///
    /// # 参数
    ///
    /// * `stats` - 压缩统计列表
    ///
    /// # 返回
    ///
    /// * `Self` - 批量统计
    pub fn from_stats(stats: &[CompressionStats]) -> Self {
        if stats.is_empty() {
            return Self::new();
        }

        let total_original: usize = stats.iter().map(|s| s.original_size).sum();
        let total_compressed: usize = stats.iter().map(|s| s.compressed_size).sum();

        BatchCompressionStats {
            total_original_size: total_original,
            total_compressed_size: total_compressed,
            average_ratio: KvChunkCompressor::compression_ratio(total_original, total_compressed),
            chunk_count: stats.len(),
        }
    }

    /// 获取平均压缩率百分比
    pub fn average_ratio_percent(&self) -> f64 {
        self.average_ratio * 100.0
    }

    /// 获取总空间节省百分比
    pub fn total_space_saved_percent(&self) -> f64 {
        if self.total_original_size == 0 {
            0.0
        } else {
            (self.total_original_size - self.total_compressed_size) as f64
                / self.total_original_size as f64
                * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_compression() {
        let compressor = KvChunkCompressor::default();
        let data = vec![1u8; 1000];

        let compressed = compressor.compress(&data).unwrap();
        let decompressed = compressor.decompress(&compressed).unwrap();

        assert_eq!(data, decompressed);
        assert!(compressed.len() < data.len());
    }

    #[test]
    fn test_compression_levels() {
        let data = vec![1u8; 10000];

        let fast = KvChunkCompressor::fast();
        let default = KvChunkCompressor::default();
        let high = KvChunkCompressor::high_compression();

        let fast_compressed = fast.compress(&data).unwrap();
        let default_compressed = default.compress(&data).unwrap();
        let high_compressed = high.compress(&data).unwrap();

        // 验证都能正确解压缩
        assert_eq!(data, fast.decompress(&fast_compressed).unwrap());
        assert_eq!(data, default.decompress(&default_compressed).unwrap());
        assert_eq!(data, high.decompress(&high_compressed).unwrap());

        // 高级别应该压缩率更高 (或相等)
        assert!(high_compressed.len() <= fast_compressed.len());
    }

    #[test]
    fn test_empty_data_compression() {
        let compressor = KvChunkCompressor::default();

        let compressed = compressor.compress(&[]).unwrap();
        assert!(compressed.is_empty());

        let decompressed = compressor.decompress(&compressed).unwrap();
        assert!(decompressed.is_empty());
    }

    #[test]
    fn test_compression_ratio_calculation() {
        let original_size = 1000;
        let compressed_size = 250;

        let ratio = KvChunkCompressor::compression_ratio(original_size, compressed_size);
        assert!((ratio - 0.25).abs() < 0.01);

        let ratio_percent = KvChunkCompressor::compression_ratio_percent(original_size, compressed_size);
        assert!((ratio_percent - 25.0).abs() < 0.01);
    }

    #[test]
    fn test_compression_with_stats() {
        let compressor = KvChunkCompressor::new(3);
        let data = vec![1u8; 5000];

        let stats = compressor.compress_with_stats(&data).unwrap();

        assert_eq!(stats.original_size, 5000);
        assert!(stats.compressed_size < stats.original_size);
        assert!(stats.compression_ratio < 1.0);
        assert!(stats.space_saved_percent() > 0.0);
    }

    #[test]
    fn test_compression_stats_methods() {
        let stats = CompressionStats {
            original_size: 1000,
            compressed_size: 300,
            compression_ratio: 0.3,
            compressed_data: vec![1u8; 300],
        };

        assert!((stats.ratio_percent() - 30.0).abs() < 0.01);
        assert!((stats.space_saved_percent() - 70.0).abs() < 0.01);
        assert_eq!(stats.space_saved_bytes(), 700);
    }

    #[test]
    fn test_verify_compression() {
        let compressor = KvChunkCompressor::default();
        let data = vec![1u8; 1000];

        let valid = verify_compression(&compressor, &data).unwrap();
        assert!(valid);
    }

    #[test]
    fn test_batch_compression_stats() {
        let stats = vec![
            CompressionStats {
                original_size: 1000,
                compressed_size: 300,
                compression_ratio: 0.3,
                compressed_data: vec![],
            },
            CompressionStats {
                original_size: 2000,
                compressed_size: 600,
                compression_ratio: 0.3,
                compressed_data: vec![],
            },
        ];

        let batch = BatchCompressionStats::from_stats(&stats);

        assert_eq!(batch.total_original_size, 3000);
        assert_eq!(batch.total_compressed_size, 900);
        assert!((batch.average_ratio - 0.3).abs() < 0.01);
        assert_eq!(batch.chunk_count, 2);
        assert!((batch.total_space_saved_percent() - 70.0).abs() < 0.01);
    }

    #[test]
    fn test_compression_level_clamping() {
        // 测试压缩级别被限制在 1-22 之间
        let compressor1 = KvChunkCompressor::new(0);
        assert_eq!(compressor1.compression_level(), 1);

        let compressor2 = KvChunkCompressor::new(23);
        assert_eq!(compressor2.compression_level(), 22);

        let compressor3 = KvChunkCompressor::new(10);
        assert_eq!(compressor3.compression_level(), 10);
    }

    #[test]
    fn test_realistic_kv_data_compression() {
        let compressor = KvChunkCompressor::default();

        // 模拟 KV 数据 (有一定随机性)
        let mut kv_data = vec![0u8; 4096];
        for (i, byte) in kv_data.iter_mut().enumerate() {
            *byte = ((i * 7 + 13) % 256) as u8;
        }

        let stats = compressor.compress_with_stats(&kv_data).unwrap();

        println!("Original size: {} bytes", stats.original_size);
        println!("Compressed size: {} bytes", stats.compressed_size);
        println!("Compression ratio: {:.2}%", stats.ratio_percent());
        println!("Space saved: {:.2}%", stats.space_saved_percent());

        // zstd 应该能压缩这种数据
        assert!(stats.compression_ratio < 1.0);
        assert!(stats.space_saved_percent() > 10.0);
    }
}
