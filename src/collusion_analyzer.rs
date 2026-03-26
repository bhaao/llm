//! 节点合谋检测器 - P1-5
//!
//! **设计目标**：
//! - 检测多个节点之间的合谋行为
//! - 识别异常的协作模式
//! - 防止节点联合作弊
//!
//! **检测维度**：
//! 1. **输出相似性检测** - 不同节点输出异常相似
//! 2. **时间同步性检测** - 节点响应时间异常同步
//! 3. **评分一致性检测** - 节点互相评分异常一致
//! 4. **网络关系分析** - 识别异常紧密的节点子图

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;
use tracing::{info, instrument};
use serde::{Serialize, Deserialize};

use crate::provider_layer::InferenceResponse;

/// 合谋检测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollusionAnalysisResult {
    /// 是否检测到合谋
    pub collusion_detected: bool,
    /// 合谋可信度（0.0 - 1.0）
    pub confidence: f64,
    /// 涉及的节点列表
    pub suspected_nodes: Vec<String>,
    /// 检测到的问题列表
    pub issues: Vec<CollusionIssue>,
    /// 分析详情
    pub details: AnalysisDetails,
}

/// 合谋问题
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollusionIssue {
    /// 问题类型
    pub issue_type: CollusionType,
    /// 涉及的节点
    pub involved_nodes: Vec<String>,
    /// 问题描述
    pub description: String,
    /// 严重程度（0.0 - 1.0）
    pub severity: f64,
    /// 证据
    pub evidence: String,
}

/// 合谋类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CollusionType {
    /// 输出相似性过高
    OutputSimilarity,
    /// 时间同步性异常
    TimeSynchronization,
    /// 评分一致性异常
    RatingConsistency,
    /// 循环评分
    CircularRating,
    /// 小团体行为
    CliqueBehavior,
    /// 其他异常
    OtherAnomaly,
}

/// 分析详情
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnalysisDetails {
    /// 分析的节点对数量
    pub analyzed_pairs: usize,
    /// 输出相似性平均分
    pub avg_output_similarity: f64,
    /// 时间同步性平均分
    pub avg_time_sync: f64,
    /// 评分一致性平均分
    pub avg_rating_consistency: f64,
    /// 检测时间戳
    pub analysis_timestamp: u64,
}

/// 合谋检测器配置
#[derive(Debug, Clone)]
pub struct CollusionAnalyzerConfig {
    /// 输出相似性阈值（高于此值认为异常）
    pub output_similarity_threshold: f64,
    /// 时间同步性阈值（毫秒）
    pub time_sync_threshold_ms: f64,
    /// 评分一致性阈值
    pub rating_consistency_threshold: f64,
    /// 最小可疑节点数
    pub min_suspected_nodes: usize,
    /// 启用输出相似性检测
    pub enable_output_similarity: bool,
    /// 启用时间同步性检测
    pub enable_time_sync: bool,
    /// 启用评分一致性检测
    pub enable_rating_consistency: bool,
    /// 启用网络关系分析
    pub enable_network_analysis: bool,
}

impl Default for CollusionAnalyzerConfig {
    fn default() -> Self {
        CollusionAnalyzerConfig {
            output_similarity_threshold: 0.95, // 95% 相似认为异常
            time_sync_threshold_ms: 100.0,     // 100ms 内认为同步
            rating_consistency_threshold: 0.98, // 98% 一致认为异常
            min_suspected_nodes: 2,
            enable_output_similarity: true,
            enable_time_sync: true,
            enable_rating_consistency: true,
            enable_network_analysis: true,
        }
    }
}

/// 节点对历史记录
#[derive(Debug, Clone, Default)]
pub struct NodePairHistory {
    /// 历史输出相似性
    pub output_similarities: VecDeque<f64>,
    /// 历史时间差（毫秒）
    pub time_differences_ms: VecDeque<f64>,
    /// 历史评分差异
    pub rating_differences: VecDeque<f64>,
}

impl NodePairHistory {
    fn new(max_size: usize) -> Self {
        NodePairHistory {
            output_similarities: VecDeque::with_capacity(max_size),
            time_differences_ms: VecDeque::with_capacity(max_size),
            rating_differences: VecDeque::with_capacity(max_size),
        }
    }

    fn record(&mut self, output_sim: f64, time_diff_ms: f64, rating_diff: f64) {
        self.output_similarities.push_back(output_sim);
        self.time_differences_ms.push_back(time_diff_ms);
        self.rating_differences.push_back(rating_diff);

        // 限制历史记录大小
        let max_size = self.output_similarities.capacity();
        while self.output_similarities.len() > max_size {
            self.output_similarities.pop_front();
            self.time_differences_ms.pop_front();
            self.rating_differences.pop_front();
        }
    }

    fn avg_output_similarity(&self) -> Option<f64> {
        if self.output_similarities.is_empty() {
            return None;
        }
        let sum: f64 = self.output_similarities.iter().sum();
        Some(sum / self.output_similarities.len() as f64)
    }

    fn avg_time_difference(&self) -> Option<f64> {
        if self.time_differences_ms.is_empty() {
            return None;
        }
        let sum: f64 = self.time_differences_ms.iter().sum();
        Some(sum / self.time_differences_ms.len() as f64)
    }

    fn avg_rating_difference(&self) -> Option<f64> {
        if self.rating_differences.is_empty() {
            return None;
        }
        let sum: f64 = self.rating_differences.iter().sum();
        Some(sum / self.rating_differences.len() as f64)
    }
}

/// 节点关系图
#[derive(Debug, Clone)]
pub struct NodeRelationshipGraph {
    /// 节点列表
    pub nodes: HashSet<String>,
    /// 边（节点对 -> 关系强度）
    pub edges: HashMap<(String, String), f64>,
}

impl NodeRelationshipGraph {
    fn new() -> Self {
        NodeRelationshipGraph {
            nodes: HashSet::new(),
            edges: HashMap::new(),
        }
    }

    fn add_node(&mut self, node_id: String) {
        self.nodes.insert(node_id);
    }

    fn add_edge(&mut self, node1: String, node2: String, strength: f64) {
        let key = if node1 < node2 {
            (node1, node2)
        } else {
            (node2, node1)
        };
        self.edges.insert(key, strength);
    }

    /// 检测小团体（clique）
    fn find_cliques(&self, min_size: usize) -> Vec<HashSet<String>> {
        // 简化实现：寻找高度连接的子图
        let mut cliques = Vec::new();

        // 构建邻接表
        let mut adjacency: HashMap<String, HashSet<String>> = HashMap::new();
        for ((node1, node2), &strength) in &self.edges {
            if strength > 0.8 {
                // 高强度连接
                adjacency.entry(node1.clone()).or_default().insert(node2.clone());
                adjacency.entry(node2.clone()).or_default().insert(node1.clone());
            }
        }

        // 寻找紧密连接的子图
        for node in &self.nodes {
            if let Some(neighbors) = adjacency.get(node) {
                if neighbors.len() >= min_size - 1 {
                    let mut clique = HashSet::new();
                    clique.insert(node.clone());
                    
                    // 检查邻居之间是否也互相连接
                    for neighbor in neighbors {
                        if let Some(neighbor_edges) = adjacency.get(neighbor) {
                            if neighbor_edges.contains(node) {
                                clique.insert(neighbor.clone());
                            }
                        }
                    }

                    if clique.len() >= min_size {
                        cliques.push(clique);
                    }
                }
            }
        }

        cliques
    }
}

/// 节点合谋检测器
///
/// **功能**：
/// - 分析节点间输出相似性
/// - 检测时间同步性
/// - 分析评分一致性
/// - 识别小团体行为
pub struct CollusionAnalyzer {
    /// 配置
    config: CollusionAnalyzerConfig,
    /// 节点对历史
    pair_history: Arc<RwLock<HashMap<(String, String), NodePairHistory>>>,
    /// 节点关系图
    relationship_graph: Arc<RwLock<NodeRelationshipGraph>>,
    /// 最大历史记录数
    max_history_size: usize,
}

impl CollusionAnalyzer {
    /// 创建新的检测器
    pub fn new(config: CollusionAnalyzerConfig) -> Self {
        CollusionAnalyzer {
            config,
            pair_history: Arc::new(RwLock::new(HashMap::new())),
            relationship_graph: Arc::new(RwLock::new(NodeRelationshipGraph::new())),
            max_history_size: 50,
        }
    }

    /// 创建默认检测器
    pub fn with_defaults() -> Self {
        Self::new(CollusionAnalyzerConfig::default())
    }

    /// 执行合谋分析
    #[instrument(skip(self, responses))]
    pub async fn analyze(&self, responses: &[InferenceResponse]) -> Result<CollusionAnalysisResult> {
        info!("Performing collusion analysis on {} responses", responses.len());

        let mut issues = Vec::new();
        let mut details = AnalysisDetails {
            analysis_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            ..Default::default()
        };

        if responses.len() < 2 {
            return Ok(CollusionAnalysisResult {
                collusion_detected: false,
                confidence: 0.0,
                suspected_nodes: Vec::new(),
                issues,
                details,
            });
        }

        // 1. 输出相似性检测
        if self.config.enable_output_similarity {
            if let Some(issue) = self.analyze_output_similarity(responses).await {
                issues.push(issue);
            }
        }

        // 2. 时间同步性检测
        if self.config.enable_time_sync {
            if let Some(issue) = self.analyze_time_synchronization(responses).await {
                issues.push(issue);
            }
        }

        // 3. 更新关系图
        if self.config.enable_network_analysis {
            self.update_relationship_graph(responses).await;
            
            // 检测小团体
            if let Some(issue) = self.detect_cliques().await {
                issues.push(issue);
            }
        }

        // 计算总体结果
        let collusion_detected = !issues.is_empty();
        let confidence = if issues.is_empty() {
            0.0
        } else {
            issues.iter().map(|i| i.severity).fold(0.0_f64, f64::max)
        };

        let suspected_nodes: HashSet<String> = issues.iter()
            .flat_map(|i| i.involved_nodes.clone())
            .collect();

        details.analyzed_pairs = responses.len() * (responses.len() - 1) / 2;

        Ok(CollusionAnalysisResult {
            collusion_detected,
            confidence,
            suspected_nodes: suspected_nodes.into_iter().collect(),
            issues,
            details,
        })
    }

    /// 分析输出相似性
    async fn analyze_output_similarity(
        &self,
        responses: &[InferenceResponse],
    ) -> Option<CollusionIssue> {
        let mut high_similarity_pairs = Vec::new();
        let mut total_similarity = 0.0;
        let mut pair_count = 0;

        for i in 0..responses.len() {
            for j in (i + 1)..responses.len() {
                let node1 = &responses[i].request_id;
                let node2 = &responses[j].request_id;

                let similarity = self.calculate_text_similarity(
                    &responses[i].completion,
                    &responses[j].completion,
                );

                total_similarity += similarity;
                pair_count += 1;

                // 记录历史
                self.record_pair_history(node1, node2, Some(similarity), None, None).await;

                if similarity > self.config.output_similarity_threshold {
                    high_similarity_pairs.push((node1.clone(), node2.clone(), similarity));
                }
            }
        }

        let avg_similarity = if pair_count > 0 {
            total_similarity / pair_count as f64
        } else {
            0.0
        };

        if !high_similarity_pairs.is_empty() {
            let involved_nodes: HashSet<String> = high_similarity_pairs.iter()
                .flat_map(|(n1, n2, _)| vec![n1.clone(), n2.clone()])
                .collect();

            let severity = high_similarity_pairs.iter()
                .map(|(_, _, s)| *s)
                .fold(0.0_f64, f64::max);

            return Some(CollusionIssue {
                issue_type: CollusionType::OutputSimilarity,
                involved_nodes: involved_nodes.into_iter().collect(),
                description: format!(
                    "{} node pairs have abnormally high output similarity (> {:.1}%)",
                    high_similarity_pairs.len(),
                    self.config.output_similarity_threshold * 100.0
                ),
                severity,
                evidence: format!(
                    "avg_similarity={:.3}, max_pairs={}",
                    avg_similarity,
                    high_similarity_pairs.len()
                ),
            });
        }

        None
    }

    /// 分析时间同步性
    async fn analyze_time_synchronization(
        &self,
        responses: &[InferenceResponse],
    ) -> Option<CollusionIssue> {
        let mut sync_pairs = Vec::new();

        for i in 0..responses.len() {
            for j in (i + 1)..responses.len() {
                let node1 = &responses[i].request_id;
                let node2 = &responses[j].request_id;

                let time_diff = (responses[i].latency_ms
                    as i64 - responses[j].latency_ms as i64).abs() as f64;

                // 记录历史
                self.record_pair_history(node1, node2, None, Some(time_diff), None).await;

                if time_diff < self.config.time_sync_threshold_ms {
                    sync_pairs.push((node1.clone(), node2.clone(), time_diff));
                }
            }
        }

        if sync_pairs.len() > responses.len() {
            // 超过一半的节点对时间同步
            let involved_nodes: HashSet<String> = sync_pairs.iter()
                .flat_map(|(n1, n2, _)| vec![n1.clone(), n2.clone()])
                .collect();

            let avg_sync = sync_pairs.iter()
                .map(|(_, _, t)| *t)
                .sum::<f64>() / sync_pairs.len() as f64;

            return Some(CollusionIssue {
                issue_type: CollusionType::TimeSynchronization,
                involved_nodes: involved_nodes.into_iter().collect(),
                description: format!(
                    "{} node pairs have abnormally synchronized response times (< {:.1}ms)",
                    sync_pairs.len(),
                    self.config.time_sync_threshold_ms
                ),
                severity: 1.0 - (avg_sync / self.config.time_sync_threshold_ms),
                evidence: format!("avg_time_diff={:.2}ms", avg_sync),
            });
        }

        None
    }

    /// 检测小团体
    async fn detect_cliques(&self) -> Option<CollusionIssue> {
        let graph = self.relationship_graph.read().await;
        let cliques = graph.find_cliques(3); // 最小 3 个节点

        if !cliques.is_empty() {
            let all_nodes: HashSet<String> = cliques.iter()
                .flatten()
                .cloned()
                .collect();
            
            let total_nodes = all_nodes.len();

            return Some(CollusionIssue {
                issue_type: CollusionType::CliqueBehavior,
                involved_nodes: all_nodes.into_iter().collect(),
                description: format!(
                    "Detected {} suspicious clique(s) with {} total nodes",
                    cliques.len(),
                    total_nodes
                ),
                severity: 0.8,
                evidence: format!("clique_count={}, total_nodes={}", cliques.len(), total_nodes),
            });
        }

        None
    }

    /// 更新关系图
    async fn update_relationship_graph(&self, responses: &[InferenceResponse]) {
        let mut graph = self.relationship_graph.write().await;

        for response in responses {
            graph.add_node(response.request_id.clone());
        }

        // 基于相似性更新边
        for i in 0..responses.len() {
            for j in (i + 1)..responses.len() {
                let similarity = self.calculate_text_similarity(
                    &responses[i].completion,
                    &responses[j].completion,
                );

                // 简化实现：使用 request_id 作为节点标识
                graph.add_edge(
                    responses[i].request_id.clone(),
                    responses[j].request_id.clone(),
                    similarity,
                );
            }
        }
    }

    /// 记录节点对历史
    async fn record_pair_history(
        &self,
        node1: &str,
        node2: &str,
        output_sim: Option<f64>,
        time_diff: Option<f64>,
        rating_diff: Option<f64>,
    ) {
        let key = if node1 < node2 {
            (node1.to_string(), node2.to_string())
        } else {
            (node2.to_string(), node1.to_string())
        };

        let mut history = self.pair_history.write().await;
        let entry = history.entry(key)
            .or_insert_with(|| NodePairHistory::new(self.max_history_size));

        entry.record(
            output_sim.unwrap_or(0.0),
            time_diff.unwrap_or(0.0),
            rating_diff.unwrap_or(0.0),
        );
    }

    /// 计算文本相似性（简化版 Jaccard 相似性）
    fn calculate_text_similarity(&self, text1: &str, text2: &str) -> f64 {
        let words1: HashSet<_> = text1.split_whitespace().collect();
        let words2: HashSet<_> = text2.split_whitespace().collect();

        if words1.is_empty() && words2.is_empty() {
            return 1.0;
        }

        let intersection = words1.intersection(&words2).count();
        let union = words1.union(&words2).count();

        if union == 0 {
            0.0
        } else {
            intersection as f64 / union as f64
        }
    }

    /// 获取节点对统计
    pub async fn get_pair_stats(&self, node1: &str, node2: &str) -> Option<PairStats> {
        let key = if node1 < node2 {
            (node1.to_string(), node2.to_string())
        } else {
            (node2.to_string(), node1.to_string())
        };

        let history = self.pair_history.read().await;
        history.get(&key).map(|h| PairStats {
            node1: node1.to_string(),
            node2: node2.to_string(),
            total_comparisons: h.output_similarities.len() as u64,
            avg_output_similarity: h.avg_output_similarity().unwrap_or(0.0),
            avg_time_difference_ms: h.avg_time_difference().unwrap_or(0.0),
            avg_rating_difference: h.avg_rating_difference().unwrap_or(0.0),
        })
    }

    /// 清除历史
    pub async fn clear_history(&self, pair: Option<(&str, &str)>) {
        let mut history = self.pair_history.write().await;
        
        if let Some((node1, node2)) = pair {
            let key = if node1 < node2 {
                (node1.to_string(), node2.to_string())
            } else {
                (node2.to_string(), node1.to_string())
            };
            history.remove(&key);
        } else {
            history.clear();
        }
    }
}

/// 节点对统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairStats {
    /// 节点 1
    pub node1: String,
    /// 节点 2
    pub node2: String,
    /// 总比较次数
    pub total_comparisons: u64,
    /// 平均输出相似性
    pub avg_output_similarity: f64,
    /// 平均时间差（毫秒）
    pub avg_time_difference_ms: f64,
    /// 平均评分差异
    pub avg_rating_difference: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_collusion_analyzer_no_collusion() {
        let analyzer = CollusionAnalyzer::with_defaults();

        let responses = vec![
            InferenceResponse {
                request_id: "test1".to_string(),
                completion: "This is output from node 1 with unique content.".to_string(),
                prompt_tokens: 5,
                completion_tokens: 10,
                latency_ms: 100,
                efficiency: 10.0,
                new_kv: HashMap::new(),
                success: true,
                error_message: None,
            },
            InferenceResponse {
                request_id: "test2".to_string(),
                completion: "This is completely different output from node 2.".to_string(),
                prompt_tokens: 5,
                completion_tokens: 8,
                latency_ms: 500,
                efficiency: 8.0,
                new_kv: HashMap::new(),
                success: true,
                error_message: None,
            },
        ];

        let result = analyzer.analyze(&responses).await.unwrap();
        
        assert!(!result.collusion_detected);
        assert!(result.confidence < 0.5);
    }

    #[tokio::test]
    async fn test_collusion_analyzer_high_similarity() {
        let analyzer = CollusionAnalyzer::with_defaults();

        let identical_output = "This is identical output that suggests collusion.".to_string();
        let responses = vec![
            InferenceResponse {
                request_id: "test1".to_string(),
                completion: identical_output.clone(),
                prompt_tokens: 5,
                completion_tokens: 10,
                latency_ms: 100,
                efficiency: 10.0,
                new_kv: HashMap::new(),
                success: true,
                error_message: None,
            },
            InferenceResponse {
                request_id: "test2".to_string(),
                completion: identical_output.clone(),
                prompt_tokens: 5,
                completion_tokens: 10,
                latency_ms: 101, // 几乎相同的时间
                efficiency: 10.0,
                new_kv: HashMap::new(),
                success: true,
                error_message: None,
            },
        ];

        let result = analyzer.analyze(&responses).await.unwrap();
        
        assert!(result.collusion_detected);
        assert!(result.confidence > 0.5);
        assert!(result.suspected_nodes.len() >= 2);
    }

    #[test]
    fn test_text_similarity() {
        let analyzer = CollusionAnalyzer::with_defaults();

        // 完全相同
        let sim1 = analyzer.calculate_text_similarity("hello world", "hello world");
        assert!((sim1 - 1.0).abs() < 0.01);

        // 完全不同
        let sim2 = analyzer.calculate_text_similarity("hello world", "foo bar");
        assert!(sim2 < 0.1);

        // 部分相同
        let sim3 = analyzer.calculate_text_similarity("hello world foo", "hello world bar");
        assert!(sim3 > 0.5 && sim3 < 1.0);
    }
}
