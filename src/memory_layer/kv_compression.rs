//! KV Cache 压缩模块 - INT8 量化/稀疏化压缩
//!
//! **核心功能**：
//! - INT8 量化（FP32 -> INT8，压缩率 75%）
//! - 稀疏化（保留 top-k 元素）
//! - 压缩/解压缩
//!
//! # 压缩算法对比
//!
//! | 算法 | 压缩率 | 精度损失 | 速度 | 适用场景 |
//! |-----|-------|---------|------|---------|
//! | INT8 量化 | 75% | < 1% | 快 | 通用 |
//! | 稀疏化 | 50-90% | 1-5% | 中 | 稀疏注意力 |
//! | PCA 降维 | 60-80% | 2-10% | 慢 | 离线压缩 |
//!
//! # 使用示例
//!
//! ```
//! use block_chain_with_context::memory_layer::kv_compression::{KvCompressor, CompressionAlgorithm, CompressedKv};
//!
//! // 创建压缩器（INT8 量化）
//! let compressor = KvCompressor::new(CompressionAlgorithm::Quantization, 0.25);
//!
//! // 压缩 KV
//! let kv_data = vec![1.0f32, 2.0, 3.0, 4.0, 5.0];
//! let compressed = compressor.compress(&kv_data).unwrap();
//!
//! // 解压缩
//! let decompressed = compressor.decompress(compressed).unwrap();
//! ```

use serde::{Serialize, Deserialize};

/// KV Cache 压缩器
#[derive(Debug, Clone)]
pub struct KvCompressor {
    /// 压缩算法
    algorithm: CompressionAlgorithm,
    /// 目标压缩率（0.0-1.0）
    target_ratio: f32,
}

/// 压缩算法枚举
#[derive(Debug, Clone)]
pub enum CompressionAlgorithm {
    /// 量化（FP32 -> INT8）
    Quantization,
    /// PCA 降维
    Pca { components: usize },
    /// 稀疏化（保留 top-k）
    Sparsification { top_k: usize },
}

/// 压缩后的 KV 数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompressedKv {
    /// INT8 量化数据
    Quantized(QuantizedKv),
    /// PCA 压缩数据
    Pca { coefficients: Vec<f32>, components: Vec<Vec<f32>> },
    /// 稀疏化数据
    Sparse { indices: Vec<usize>, values: Vec<f32>, shape: usize },
}

/// INT8 量化数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizedKv {
    /// 量化数据（INT8）
    pub data: Vec<u8>,
    /// 最小值（用于反量化）
    pub min: f32,
    /// 最大值（用于反量化）
    pub max: f32,
    /// 原始形状
    pub shape: usize,
}

impl KvCompressor {
    /// 创建新的压缩器
    ///
    /// # 参数
    ///
    /// * `algorithm` - 压缩算法
    /// * `target_ratio` - 目标压缩率（0.0-1.0）
    ///
    /// # 返回
    ///
    /// * `Self` - 新的压缩器实例
    pub fn new(algorithm: CompressionAlgorithm, target_ratio: f32) -> Self {
        KvCompressor {
            algorithm,
            target_ratio: target_ratio.clamp(0.0, 1.0),
        }
    }

    /// 创建 INT8 量化压缩器
    ///
    /// # 返回
    ///
    /// * `Self` - 新的压缩器实例
    pub fn quantization() -> Self {
        KvCompressor::new(CompressionAlgorithm::Quantization, 0.25)
    }

    /// 创建稀疏化压缩器
    ///
    /// # 参数
    ///
    /// * `top_k` - 保留的最大元素数
    ///
    /// # 返回
    ///
    /// * `Self` - 新的压缩器实例
    pub fn sparsification(top_k: usize) -> Self {
        KvCompressor::new(CompressionAlgorithm::Sparsification { top_k }, 0.0)
    }

    /// 压缩 KV 数据
    ///
    /// # 参数
    ///
    /// * `kv` - 原始 KV 数据（f32 向量）
    ///
    /// # 返回
    ///
    /// * `Result<CompressedKv, String>` - 压缩后的数据或错误
    pub fn compress(&self, kv: &[f32]) -> Result<CompressedKv, String> {
        match &self.algorithm {
            CompressionAlgorithm::Quantization => {
                self.quantize(kv)
            }
            CompressionAlgorithm::Pca { components } => {
                self.pca_compress(kv, *components)
            }
            CompressionAlgorithm::Sparsification { top_k } => {
                self.sparse_compress(kv, *top_k)
            }
        }
    }

    /// 解压缩 KV 数据
    ///
    /// # 参数
    ///
    /// * `compressed` - 压缩后的数据
    ///
    /// # 返回
    ///
    /// * `Result<Vec<f32>, String>` - 解压缩后的数据或错误
    pub fn decompress(&self, compressed: CompressedKv) -> Result<Vec<f32>, String> {
        match compressed {
            CompressedKv::Quantized(q) => self.dequantize(q),
            CompressedKv::Pca { coefficients, components } => {
                self.pca_decompress(coefficients, components)
            }
            CompressedKv::Sparse { indices, values, shape } => {
                self.sparse_decompress(indices, values, shape)
            }
        }
    }

    /// 获取压缩算法
    pub fn algorithm(&self) -> &CompressionAlgorithm {
        &self.algorithm
    }

    /// 获取目标压缩率
    pub fn target_ratio(&self) -> f32 {
        self.target_ratio
    }

    /// 计算实际压缩率
    ///
    /// # 参数
    ///
    /// * `original_size` - 原始大小（字节）
    /// * `compressed_size` - 压缩后大小（字节）
    ///
    /// # 返回
    ///
    /// * `f32` - 压缩率（压缩后大小/原始大小）
    pub fn calculate_compression_ratio(original_size: usize, compressed_size: usize) -> f32 {
        if original_size == 0 {
            1.0
        } else {
            compressed_size as f32 / original_size as f32
        }
    }

    /// INT8 量化压缩
    fn quantize(&self, kv: &[f32]) -> Result<CompressedKv, String> {
        if kv.is_empty() {
            return Err("Cannot compress empty KV data".to_string());
        }

        // 找到最小值和最大值
        let min_val = kv.iter().cloned().fold(f32::INFINITY, f32::min);
        let max_val = kv.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

        // 处理所有值相同的情况
        if min_val == max_val {
            return Ok(CompressedKv::Quantized(QuantizedKv {
                data: vec![128; kv.len()], // 中间值
                min: min_val,
                max: max_val,
                shape: kv.len(),
            }));
        }

        // 计算缩放因子
        let scale = (max_val - min_val) / 255.0;

        // 量化：FP32 -> INT8 (0-255)
        let quantized: Vec<u8> = kv
            .iter()
            .map(|&v| {
                let q = ((v - min_val) / scale) as u16;
                q.clamp(0, 255) as u8
            })
            .collect();

        // 计算压缩率
        let original_bytes = kv.len() * 4; // f32 = 4 bytes
        let compressed_bytes = quantized.len() + 12; // 1 byte per value + 8 bytes for min/max + 4 for shape
        let ratio = Self::calculate_compression_ratio(original_bytes, compressed_bytes);

        log::debug!(
            "Quantization: {} -> {} bytes, ratio: {:.2}%",
            original_bytes,
            compressed_bytes,
            ratio * 100.0
        );

        Ok(CompressedKv::Quantized(QuantizedKv {
            data: quantized,
            min: min_val,
            max: max_val,
            shape: kv.len(),
        }))
    }

    /// INT8 反量化
    fn dequantize(&self, q: QuantizedKv) -> Result<Vec<f32>, String> {
        if q.data.is_empty() {
            return Ok(Vec::new());
        }

        let scale = (q.max - q.min) / 255.0;

        // 反量化：INT8 -> FP32
        let dequantized: Vec<f32> = q
            .data
            .iter()
            .map(|&v| q.min + v as f32 * scale)
            .collect();

        Ok(dequantized)
    }

    /// PCA 压缩（简化版，实际应该用 linfa 或 smartcore）
    fn pca_compress(&self, _kv: &[f32], _components: usize) -> Result<CompressedKv, String> {
        Err("PCA compression requires external library (linfa/smartcore)".to_string())
    }

    /// PCA 解压缩
    fn pca_decompress(&self, _coefficients: Vec<f32>, _components: Vec<Vec<f32>>) -> Result<Vec<f32>, String> {
        Err("PCA decompression not implemented".to_string())
    }

    /// 稀疏化压缩（保留绝对值最大的 top-k 个元素）
    fn sparse_compress(&self, kv: &[f32], top_k: usize) -> Result<CompressedKv, String> {
        if kv.is_empty() {
            return Err("Cannot compress empty KV data".to_string());
        }

        let actual_top_k = top_k.min(kv.len());

        // 创建索引 - 值对
        let mut indexed: Vec<(usize, f32)> = kv.iter()
            .enumerate()
            .map(|(i, &v)| (i, v))
            .collect();

        // 按绝对值排序
        indexed.sort_by(|a, b| {
            b.1.abs().partial_cmp(&a.1.abs()).unwrap_or(std::cmp::Ordering::Equal)
        });

        // 保留 top-k
        let selected: Vec<(usize, f32)> = indexed.into_iter().take(actual_top_k).collect();

        let indices: Vec<usize> = selected.iter().map(|(i, _)| *i).collect();
        let values: Vec<f32> = selected.iter().map(|(_, v)| *v).collect();

        // 计算压缩率
        let original_bytes = kv.len() * 4; // f32 = 4 bytes
        let compressed_bytes = indices.len() * 4 + values.len() * 4 + 4; // indices + values + shape
        let ratio = Self::calculate_compression_ratio(original_bytes, compressed_bytes);

        log::debug!(
            "Sparsification: {} -> {} bytes (top-k={}), ratio: {:.2}%",
            original_bytes,
            compressed_bytes,
            actual_top_k,
            ratio * 100.0
        );

        Ok(CompressedKv::Sparse {
            indices,
            values,
            shape: kv.len(),
        })
    }

    /// 稀疏化解压缩
    fn sparse_decompress(&self, indices: Vec<usize>, values: Vec<f32>, shape: usize) -> Result<Vec<f32>, String> {
        let mut kv = vec![0.0f32; shape];

        for (&idx, &val) in indices.iter().zip(values.iter()) {
            if idx < shape {
                kv[idx] = val;
            }
        }

        Ok(kv)
    }
}

/// 计算压缩前后的精度损失（MSE）
///
/// # 参数
///
/// * `original` - 原始数据
/// * `decompressed` - 解压缩后的数据
///
/// # 返回
///
/// * `f32` - 均方误差（MSE）
pub fn calculate_mse(original: &[f32], decompressed: &[f32]) -> f32 {
    if original.len() != decompressed.len() {
        return f32::INFINITY;
    }

    let sum_squared_error: f32 = original
        .iter()
        .zip(decompressed.iter())
        .map(|(o, d)| (o - d).powi(2))
        .sum();

    sum_squared_error / original.len() as f32
}

/// 计算压缩前后的最大绝对误差
///
/// # 参数
///
/// * `original` - 原始数据
/// * `decompressed` - 解压缩后的数据
///
/// # 返回
///
/// * `f32` - 最大绝对误差
pub fn calculate_max_absolute_error(original: &[f32], decompressed: &[f32]) -> f32 {
    if original.len() != decompressed.len() {
        return f32::INFINITY;
    }

    original
        .iter()
        .zip(decompressed.iter())
        .map(|(o, d)| (o - d).abs())
        .fold(f32::NEG_INFINITY, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantization_compression() {
        let compressor = KvCompressor::quantization();

        // 创建测试数据
        let original: Vec<f32> = (0..100).map(|i| i as f32 / 100.0).collect();

        // 压缩
        let compressed = compressor.compress(&original).unwrap();

        // 验证压缩类型
        assert!(matches!(compressed, CompressedKv::Quantized(_)));

        // 解压缩
        let decompressed = compressor.decompress(compressed).unwrap();

        // 验证长度
        assert_eq!(decompressed.len(), original.len());

        // 验证精度（允许一定误差）
        let mse = calculate_mse(&original, &decompressed);
        assert!(mse < 0.01, "MSE too high: {}", mse);
    }

    #[test]
    fn test_quantization_compression_ratio() {
        let compressor = KvCompressor::quantization();

        // 创建大数据集
        let original: Vec<f32> = (0..10000).map(|i| i as f32 / 10000.0).collect();

        // 压缩
        let compressed = compressor.compress(&original).unwrap();

        // 计算压缩率
        let original_bytes = original.len() * 4;
        let compressed_bytes = match &compressed {
            CompressedKv::Quantized(q) => q.data.len() + 12, // data + min/max/shape
            _ => unreachable!(),
        };

        let ratio = KvCompressor::calculate_compression_ratio(original_bytes, compressed_bytes);

        // INT8 量化应该达到约 25% 的压缩率
        assert!(ratio < 0.30, "Compression ratio too high: {}", ratio);
        println!("Compression ratio: {:.2}%", ratio * 100.0);
    }

    #[test]
    fn test_sparsification_compression() {
        let compressor = KvCompressor::sparsification(10);

        // 创建稀疏测试数据（大部分为 0）
        let mut original = vec![0.0f32; 100];
        original[10] = 1.0;
        original[20] = 2.0;
        original[30] = 3.0;
        original[40] = 4.0;
        original[50] = 5.0;

        // 压缩
        let compressed = compressor.compress(&original).unwrap();

        // 验证压缩类型
        assert!(matches!(compressed, CompressedKv::Sparse { .. }));

        // 解压缩
        let decompressed = compressor.decompress(compressed).unwrap();

        // 验证长度
        assert_eq!(decompressed.len(), original.len());

        // 验证非零元素被保留
        assert_eq!(decompressed[10], 1.0);
        assert_eq!(decompressed[20], 2.0);
        assert_eq!(decompressed[30], 3.0);
        assert_eq!(decompressed[40], 4.0);
        assert_eq!(decompressed[50], 5.0);
    }

    #[test]
    fn test_empty_data_compression() {
        let compressor = KvCompressor::quantization();

        // 压缩空数据应该失败
        let result = compressor.compress(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_uniform_data_compression() {
        let compressor = KvCompressor::quantization();

        // 所有值相同的数据
        let original = vec![5.0f32; 100];

        // 压缩
        let compressed = compressor.compress(&original).unwrap();

        // 解压缩
        let decompressed = compressor.decompress(compressed).unwrap();

        // 验证所有值都接近原始值
        for (o, d) in original.iter().zip(decompressed.iter()) {
            assert!((o - d).abs() < 0.1, "Decompressed value {} differs from original {}", d, o);
        }
    }

    #[test]
    fn test_compression_error_metrics() {
        let compressor = KvCompressor::quantization();

        // 创建测试数据
        let original: Vec<f32> = (0..1000).map(|i| i as f32 / 1000.0).collect();

        // 压缩和解压缩
        let compressed = compressor.compress(&original).unwrap();
        let decompressed = compressor.decompress(compressed).unwrap();

        // 计算误差
        let mse = calculate_mse(&original, &decompressed);
        let max_error = calculate_max_absolute_error(&original, &decompressed);

        println!("MSE: {:.6}", mse);
        println!("Max Absolute Error: {:.6}", max_error);

        // 验证误差在可接受范围内
        assert!(mse < 0.001, "MSE too high: {}", mse);
        assert!(max_error < 0.01, "Max error too high: {}", max_error);
    }

    #[test]
    fn test_compressor_builder() {
        // 测试 INT8 量化压缩器
        let quant_compressor = KvCompressor::quantization();
        assert!(matches!(quant_compressor.algorithm(), CompressionAlgorithm::Quantization));

        // 测试稀疏化压缩器
        let sparse_compressor = KvCompressor::sparsification(50);
        assert!(matches!(sparse_compressor.algorithm(), CompressionAlgorithm::Sparsification { top_k: 50 }));
    }

    #[test]
    fn test_target_ratio_clamping() {
        // 测试压缩率限制在 0-1 之间
        let compressor1 = KvCompressor::new(CompressionAlgorithm::Quantization, -0.5);
        assert_eq!(compressor1.target_ratio(), 0.0);

        let compressor2 = KvCompressor::new(CompressionAlgorithm::Quantization, 1.5);
        assert_eq!(compressor2.target_ratio(), 1.0);
    }
}
