//! 智能预取器模块
//!
//! **核心功能**：
//! - 基于访问模式预测下一个可能访问的 chunk
//! - 变长 N-gram 模式检测（支持 2-8）
//! - 时间衰减（最近的访问权重更高）
//! - 后台异步预取
//!
//! # 架构设计
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │  Prefetcher                             │
//! │  ├─ Access History (带时间戳的访问队列) │
//! │  ├─ Pattern Detector (变长 N-gram)      │
//! │  └─ Prefetch Window (预取窗口)          │
//! └─────────────────────────────────────────┘
//!           ↓
//! ┌─────────────────────────────────────────┐
//! │  Pattern Detector                       │
//! │  ├─ 变长 N-gram 分析 (2-8)              │
//! │  ├─ 时间衰减权重                        │
//! │  ├─ 序列模式识别                        │
//! │  └─ 频率统计                            │
//! └─────────────────────────────────────────┘
//! ```

use std::collections::{VecDeque, HashMap};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// 带时间戳的访问记录
#[derive(Debug, Clone)]
pub struct TimedAccess {
    /// Chunk 唯一标识
    pub chunk_id: String,
    /// 访问时间
    pub accessed_at: Instant,
}

impl TimedAccess {
    pub fn new(chunk_id: String) -> Self {
        TimedAccess {
            chunk_id,
            accessed_at: Instant::now(),
        }
    }

    /// 获取时间衰减权重（0.0 - 1.0）
    /// 
    /// # 参数
    /// 
    /// * `decay_factor` - 每小时衰减因子（0.9 表示每小时衰减 10%）
    pub fn decay_weight(&self, decay_factor: f64) -> f64 {
        let elapsed_hours = self.accessed_at.elapsed().as_secs_f64() / 3600.0;
        decay_factor.powf(elapsed_hours)
    }
}

/// 智能预取器
pub struct Prefetcher {
    /// 访问历史队列（带时间戳）
    access_history: Arc<RwLock<VecDeque<TimedAccess>>>,
    /// 模式检测器
    pattern_detector: Arc<RwLock<PatternDetector>>,
    /// 预取窗口大小
    prefetch_window: usize,
    /// 最大历史记录数
    max_history_size: usize,
    /// 预取统计
    stats: Arc<RwLock<PrefetchStats>>,
    /// 时间衰减因子（0.9 表示每小时衰减 10%）
    decay_factor: f64,
}

impl Prefetcher {
    /// 创建新的预取器
    ///
    /// # 参数
    ///
    /// * `prefetch_window` - 预取窗口大小 (预测多少个后续 chunks)
    /// * `max_history_size` - 最大历史记录数
    /// * `decay_factor` - 时间衰减因子（0.9 表示每小时衰减 10%）
    ///
    /// # 返回
    ///
    /// * `Self` - 新的预取器实例
    pub fn new(prefetch_window: usize, max_history_size: usize, decay_factor: f64) -> Self {
        Prefetcher {
            access_history: Arc::new(RwLock::new(VecDeque::with_capacity(max_history_size.min(1024)))),
            pattern_detector: Arc::new(RwLock::new(PatternDetector::new(decay_factor))),
            prefetch_window,
            max_history_size,
            stats: Arc::new(RwLock::new(PrefetchStats::new())),
            decay_factor,
        }
    }

    /// 创建默认预取器
    ///
    /// 生产环境配置：支持 10 万 + 历史访问记录，每小时衰减 10%
    pub fn default() -> Self {
        Self::new(5, 100_000, 0.9)
    }

    /// 记录一次访问
    ///
    /// # 参数
    ///
    /// * `chunk_id` - Chunk 唯一标识
    pub async fn record_access(&self, chunk_id: String) {
        let mut history = self.access_history.write().await;

        // 添加到队列
        history.push_back(TimedAccess::new(chunk_id));

        // 超出容量时移除最旧的
        while history.len() > self.max_history_size {
            history.pop_front();
        }

        // 更新模式检测器（带时间权重）
        let mut detector = self.pattern_detector.write().await;
        detector.update(&history, self.decay_factor);
    }

    /// 预测下一个可能访问的 chunks
    ///
    /// # 返回
    ///
    /// * `Vec<String>` - 预测的 chunk IDs
    pub async fn predict_next(&self) -> Vec<String> {
        let history = self.access_history.read().await;
        let detector = self.pattern_detector.read().await;

        detector.predict_next(&history, self.prefetch_window)
    }

    /// 获取访问历史
    pub async fn get_history(&self) -> Vec<TimedAccess> {
        let history = self.access_history.read().await;
        history.iter().cloned().collect()
    }

    /// 获取历史记录数量
    pub async fn history_len(&self) -> usize {
        let history = self.access_history.read().await;
        history.len()
    }

    /// 清空历史记录
    pub async fn clear_history(&self) {
        let mut history = self.access_history.write().await;
        history.clear();

        let mut detector = self.pattern_detector.write().await;
        detector.clear();
    }

    /// 获取模式检测器
    pub async fn get_pattern_detector(&self) -> PatternDetector {
        let detector = self.pattern_detector.read().await;
        detector.clone()
    }

    /// 获取预取统计
    pub async fn get_stats(&self) -> PrefetchStats {
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// 记录一次预取命中
    pub async fn record_prefetch_hit(&self) {
        let mut stats = self.stats.write().await;
        stats.record_hit();
    }

    /// 记录一次预取未命中
    pub async fn record_prefetch_miss(&self) {
        let mut stats = self.stats.write().await;
        stats.record_miss();
    }

    /// 预测下一个可能访问的 chunks，并记录实际访问结果用于统计
    ///
    /// # 参数
    ///
    /// * `actual_access` - 实际访问的 chunk ID（用于验证预取是否命中）
    ///
    /// # 返回
    ///
    /// * `Vec<String>` - 预测的 chunk IDs
    pub async fn predict_next_with_stats(&self, actual_access: String) -> Vec<String> {
        let predictions = self.predict_next().await;

        // 检查实际访问是否在预测结果中
        if predictions.contains(&actual_access) {
            self.record_prefetch_hit().await;
        } else {
            self.record_prefetch_miss().await;
        }

        predictions
    }

    /// 应用时间衰减到所有历史记录
    ///
    /// 应该定期调用（例如每小时）来衰减历史记录的权重
    pub async fn apply_decay(&self) {
        let mut detector = self.pattern_detector.write().await;
        detector.apply_decay(self.decay_factor);
    }
}

impl Clone for Prefetcher {
    fn clone(&self) -> Self {
        Prefetcher {
            access_history: Arc::clone(&self.access_history),
            pattern_detector: Arc::clone(&self.pattern_detector),
            prefetch_window: self.prefetch_window,
            max_history_size: self.max_history_size,
            stats: Arc::clone(&self.stats),
            decay_factor: self.decay_factor,
        }
    }
}

/// 变长 N-gram 模式检测器
#[derive(Debug, Clone)]
pub struct PatternDetector {
    /// 最小 N-gram 大小
    min_ngram_size: usize,
    /// 最大 N-gram 大小
    max_ngram_size: usize,
    /// 变长 N-gram 计数：ngram_size -> (context -> (next_token -> weight))
    /// 使用权重 (f64) 而不是计数，支持时间衰减
    ngram_counts: HashMap<usize, HashMap<Vec<String>, HashMap<String, f64>>>,
    /// 单个 chunk 的频率统计（带权重）
    chunk_frequencies: HashMap<String, f64>,
}

impl PatternDetector {
    /// 创建新的模式检测器
    ///
    /// # 参数
    ///
    /// * `_decay_factor` - 时间衰减因子（0.9 表示每小时衰减 10%）
    ///
    /// # 返回
    ///
    /// * `Self` - 新的检测器实例
    pub fn new(_decay_factor: f64) -> Self {
        PatternDetector {
            min_ngram_size: 2,
            max_ngram_size: 8,
            ngram_counts: HashMap::new(),
            chunk_frequencies: HashMap::new(),
        }
    }

    /// 更新模式检测器
    ///
    /// # 参数
    ///
    /// * `history` - 访问历史队列（带时间戳）
    /// * `decay_factor` - 时间衰减因子
    pub fn update(&mut self, history: &VecDeque<TimedAccess>, decay_factor: f64) {
        if history.len() < self.min_ngram_size + 1 {
            return;
        }

        // 转换为带权重的访问记录
        let weighted_history: Vec<(String, f64)> = history
            .iter()
            .map(|access| (access.chunk_id.clone(), access.decay_weight(decay_factor)))
            .collect();

        // 更新单个 chunk 频率（带权重）
        for (chunk_id, weight) in &weighted_history {
            *self.chunk_frequencies.entry(chunk_id.clone()).or_insert(0.0) += weight;
        }

        // 更新变长 N-gram 计数
        for ngram_size in self.min_ngram_size..=self.max_ngram_size {
            if weighted_history.len() < ngram_size + 1 {
                continue;
            }

            for i in 0..weighted_history.len() - ngram_size {
                let context: Vec<String> = weighted_history[i..i + ngram_size]
                    .iter()
                    .map(|(id, _)| id.clone())
                    .collect();
                let next = weighted_history[i + ngram_size].0.clone();
                let weight = weighted_history[i + ngram_size].1;

                self.ngram_counts
                    .entry(ngram_size)
                    .or_insert_with(HashMap::new)
                    .entry(context)
                    .or_insert_with(HashMap::new)
                    .entry(next)
                    .and_modify(|w| *w += weight)
                    .or_insert(weight);
            }
        }
    }

    /// 预测下一个可能访问的 chunks
    ///
    /// # 参数
    ///
    /// * `history` - 当前访问历史（带时间戳）
    /// * `num_predictions` - 预测数量
    ///
    /// # 返回
    ///
    /// * `Vec<String>` - 预测的 chunk IDs
    pub fn predict_next(&self, history: &VecDeque<TimedAccess>, num_predictions: usize) -> Vec<String> {
        if history.len() < self.min_ngram_size {
            // 历史不足，返回高频 chunks
            return self.get_frequent_chunks(num_predictions);
        }

        let history_vec: Vec<String> = history.iter().map(|a| a.chunk_id.clone()).collect();

        // 从最大的 N-gram 开始匹配（更精确）
        for ngram_size in (self.min_ngram_size..=self.max_ngram_size).rev() {
            if history_vec.len() < ngram_size {
                continue;
            }

            let context: Vec<String> = history_vec[history_vec.len() - ngram_size..].to_vec();

            if let Some(ngram_map) = self.ngram_counts.get(&ngram_size) {
                if let Some(next_weights) = ngram_map.get(&context) {
                    // 按权重排序，返回最常见的下一个
                    let mut next_vec: Vec<(&String, &f64)> = next_weights.iter().collect();
                    next_vec.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));

                    return next_vec.iter()
                        .take(num_predictions)
                        .map(|(s, _)| (*s).clone())
                        .collect();
                }
            }
        }

        // 没有匹配的模式，返回高频 chunks
        self.get_frequent_chunks(num_predictions)
    }

    /// 获取高频 chunks
    fn get_frequent_chunks(&self, num: usize) -> Vec<String> {
        let mut freq_vec: Vec<(&String, &f64)> = self.chunk_frequencies.iter().collect();
        freq_vec.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));

        freq_vec.iter()
            .take(num)
            .map(|(s, _)| (*s).clone())
            .collect()
    }

    /// 应用时间衰减到所有计数
    pub fn apply_decay(&mut self, decay_factor: f64) {
        // 衰减 N-gram 权重
        for ngram_map in self.ngram_counts.values_mut() {
            for next_weights in ngram_map.values_mut() {
                for weight in next_weights.values_mut() {
                    *weight *= decay_factor;
                }
            }
        }

        // 衰减 chunk 频率
        for freq in self.chunk_frequencies.values_mut() {
            *freq *= decay_factor;
        }
    }

    /// 清空检测器
    pub fn clear(&mut self) {
        self.ngram_counts.clear();
        self.chunk_frequencies.clear();
    }

    /// 获取 N-gram 模式数量
    pub fn pattern_count(&self) -> usize {
        self.ngram_counts.values().map(|m| m.len()).sum()
    }

    /// 获取已学习的 chunk 数量
    pub fn learned_chunk_count(&self) -> usize {
        self.chunk_frequencies.len()
    }

    /// 获取 N-gram 大小范围
    pub fn ngram_range(&self) -> (usize, usize) {
        (self.min_ngram_size, self.max_ngram_size)
    }

    /// 设置 N-gram 大小范围
    pub fn set_ngram_range(&mut self, min_size: usize, max_size: usize) {
        if min_size >= 2 && max_size <= 8 && min_size <= max_size {
            self.min_ngram_size = min_size;
            self.max_ngram_size = max_size;
        }
    }
}

/// 预取统计信息
#[derive(Debug, Clone, Default)]
pub struct PrefetchStats {
    /// 预取次数
    pub prefetch_count: u64,
    /// 预取命中次数
    pub prefetch_hits: u64,
    /// 预取未命中次数
    pub prefetch_misses: u64,
}

impl PrefetchStats {
    /// 创建新的统计
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一次预取命中
    pub fn record_hit(&mut self) {
        self.prefetch_count += 1;
        self.prefetch_hits += 1;
    }

    /// 记录一次预取未命中
    pub fn record_miss(&mut self) {
        self.prefetch_count += 1;
        self.prefetch_misses += 1;
    }

    /// 获取预取命中率
    pub fn hit_rate(&self) -> f64 {
        if self.prefetch_count == 0 {
            0.0
        } else {
            self.prefetch_hits as f64 / self.prefetch_count as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_prefetcher_record_access() {
        let prefetcher = Prefetcher::default();

        prefetcher.record_access("chunk_1".to_string()).await;
        prefetcher.record_access("chunk_2".to_string()).await;
        prefetcher.record_access("chunk_3".to_string()).await;

        assert_eq!(prefetcher.history_len().await, 3);

        let history = prefetcher.get_history().await;
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].chunk_id, "chunk_1");
        assert_eq!(history[2].chunk_id, "chunk_3");
    }

    #[tokio::test]
    async fn test_prefetcher_history_limit() {
        let prefetcher = Prefetcher::new(5, 10, 0.9);

        // 记录 15 次访问 (超过最大历史 10)
        for i in 0..15 {
            prefetcher.record_access(format!("chunk_{}", i)).await;
        }

        // 应该只保留最近 10 个
        assert_eq!(prefetcher.history_len().await, 10);

        let history = prefetcher.get_history().await;
        assert_eq!(history[0].chunk_id, "chunk_5"); // 最旧的是 chunk_5
        assert_eq!(history[9].chunk_id, "chunk_14"); // 最新的是 chunk_14
    }

    #[tokio::test]
    async fn test_pattern_detector_variable_ngram() {
        let mut detector = PatternDetector::new(0.9);
        detector.set_ngram_range(2, 4);

        // 创建模式：chunk_1 -> chunk_2 -> chunk_3 重复出现
        let mut history = VecDeque::new();
        for _ in 0..5 {
            history.push_back(TimedAccess::new("chunk_1".to_string()));
            history.push_back(TimedAccess::new("chunk_2".to_string()));
            history.push_back(TimedAccess::new("chunk_3".to_string()));
        }

        detector.update(&history, 0.9);

        // 预测下一个
        let mut predict_history = VecDeque::new();
        predict_history.push_back(TimedAccess::new("chunk_1".to_string()));
        predict_history.push_back(TimedAccess::new("chunk_2".to_string()));

        let predictions = detector.predict_next(&predict_history, 1);

        // 应该预测 chunk_3
        assert!(!predictions.is_empty());
        assert_eq!(predictions[0], "chunk_3");
    }

    #[tokio::test]
    async fn test_pattern_detector_decay() {
        let mut detector = PatternDetector::new(0.9);

        // 添加一些访问
        let mut history = VecDeque::new();
        for i in 0..10 {
            history.push_back(TimedAccess::new(format!("chunk_{}", i)));
        }
        detector.update(&history, 0.9);

        let initial_count = detector.pattern_count();

        // 应用衰减
        detector.apply_decay(0.9);

        // 模式数量应该不变，但权重会降低
        assert_eq!(detector.pattern_count(), initial_count);
    }

    #[tokio::test]
    async fn test_pattern_detector_fallback() {
        let mut detector = PatternDetector::new(0.9);

        // 添加一些随机访问
        let mut history = VecDeque::new();
        for i in 0..20 {
            history.push_back(TimedAccess::new(format!("random_chunk_{}", i)));
        }

        detector.update(&history, 0.9);

        // 没有匹配的模式，应该返回高频 chunks
        let predictions = detector.predict_next(&history, 5);
        assert!(!predictions.is_empty());
        assert!(predictions.len() <= 5);
    }

    #[tokio::test]
    async fn test_pattern_detector_clear() {
        let mut detector = PatternDetector::new(0.9);

        let mut history = VecDeque::new();
        for i in 0..10 {
            history.push_back(TimedAccess::new(format!("chunk_{}", i)));
        }
        detector.update(&history, 0.9);

        assert!(detector.pattern_count() > 0);
        assert!(detector.learned_chunk_count() > 0);

        detector.clear();

        assert_eq!(detector.pattern_count(), 0);
        assert_eq!(detector.learned_chunk_count(), 0);
    }

    #[tokio::test]
    async fn test_prefetcher_clone() {
        let prefetcher = Prefetcher::default();

        prefetcher.record_access("chunk_1".to_string()).await;

        let cloned = prefetcher.clone();

        // 克隆体应该共享状态
        assert_eq!(cloned.history_len().await, 1);

        cloned.record_access("chunk_2".to_string()).await;

        assert_eq!(prefetcher.history_len().await, 2);
        assert_eq!(cloned.history_len().await, 2);
    }

    #[tokio::test]
    async fn test_prefetcher_clear() {
        let prefetcher = Prefetcher::default();

        for i in 0..10 {
            prefetcher.record_access(format!("chunk_{}", i)).await;
        }

        assert_eq!(prefetcher.history_len().await, 10);

        prefetcher.clear_history().await;

        assert_eq!(prefetcher.history_len().await, 0);
    }

    #[tokio::test]
    async fn test_sequential_access_pattern() {
        let prefetcher = Prefetcher::new(3, 100, 0.9);

        // 模拟顺序访问模式：0->1->2->3->4->...
        for i in 0..20 {
            prefetcher.record_access(format!("chunk_{}", i)).await;
        }

        // 预测下一个
        let predictions = prefetcher.predict_next().await;

        // 由于有历史访问记录，预测器应该返回一些结果（高频 chunks）
        let _ = predictions; // 验证能正常执行即可
    }

    #[tokio::test]
    async fn test_prefetch_stats() {
        let mut stats = PrefetchStats::new();

        assert_eq!(stats.hit_rate(), 0.0);

        stats.record_hit();
        stats.record_hit();
        stats.record_miss();

        assert_eq!(stats.prefetch_count, 3);
        assert_eq!(stats.prefetch_hits, 2);
        assert_eq!(stats.prefetch_misses, 1);
        assert!((stats.hit_rate() - 0.666).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_predict_next_with_stats_hit() {
        let prefetcher = Prefetcher::default();

        // 创建顺序访问模式：chunk_1 -> chunk_2 -> chunk_3
        prefetcher.record_access("chunk_1".to_string()).await;
        prefetcher.record_access("chunk_2".to_string()).await;
        prefetcher.record_access("chunk_3".to_string()).await;

        // 预测并验证
        let _predictions = prefetcher.predict_next_with_stats("chunk_4".to_string()).await;

        // 检查统计是否记录
        let stats = prefetcher.get_stats().await;
        assert!(stats.prefetch_count > 0);
    }

    #[tokio::test]
    async fn test_variable_ngram_range() {
        let mut detector = PatternDetector::new(0.9);
        
        // 默认范围
        assert_eq!(detector.ngram_range(), (2, 8));
        
        // 设置自定义范围
        detector.set_ngram_range(3, 6);
        assert_eq!(detector.ngram_range(), (3, 6));
        
        // 无效范围应该被忽略
        detector.set_ngram_range(1, 10); // min < 2
        assert_eq!(detector.ngram_range(), (3, 6)); // 保持不变
    }
}
