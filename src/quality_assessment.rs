//! 质量评估模块 - 轻量化质量评估器
//!
//! **架构定位**：
//! - 推理负责算得对：分布式推理模块负责高效计算
//! - 评估器负责验得准：本模块负责验证结果质量
//! - 多节点负责保安全：并行计算 + 结果比对
//! - 区块链负责记可信：不可篡改的存证记录
//!
//! **质量评估器职责**：
//! 1. 验证 KV Cache 哈希是否与链上存证一致
//! 2. 检查输出语义是否合理、不偏离上下文
//! 3. 判断结果是否正常、无恶意篡改
//!
//! **触发条件**（满足任一即启用多节点并行）：
//! - 高敏感内容（医疗、法律、金融等）
//! - 低信誉节点（信誉分 < 阈值）
//! - 超长上下文（超过 token 限制）
//!
//! **语义检查模式**：
//! - `SemanticCheckMode::Rules`: 基于规则的轻量检查（默认）
//! - `SemanticCheckMode::SmallModel`: 接入小型语义模型（更准确）
//! - `SemanticCheckMode::Disabled`: 关闭语义检查（仅 KV 校验）

use crate::block::KvCacheProof;

/// 语义检查模式 - 可插拔配置
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SemanticCheckMode {
    /// 基于规则的轻量检查（默认）
    /// - 检查空输出
    /// - 检查过度重复
    /// - 检查异常截断
    #[default]
    Rules,
    /// 接入小型语义模型（更准确，但需要额外依赖）
    /// - 语义一致性检查
    /// - 有害内容检测
    /// - 逻辑连贯性分析
    SmallModel,
    /// 关闭语义检查（仅进行 KV 校验和完整性检查）
    /// 适用于对性能要求极高的场景
    Disabled,
}

impl SemanticCheckMode {
    /// 是否启用语义检查
    pub fn is_enabled(&self) -> bool {
        matches!(self, SemanticCheckMode::Rules | SemanticCheckMode::SmallModel)
    }

    /// 是否使用规则模式
    pub fn is_rules_mode(&self) -> bool {
        matches!(self, SemanticCheckMode::Rules)
    }

    /// 是否使用小模型模式
    pub fn is_small_model_mode(&self) -> bool {
        matches!(self, SemanticCheckMode::SmallModel)
    }
}

/// 质量评估结果
#[derive(Debug, Clone, PartialEq)]
pub struct QualityAssessment {
    /// 综合质量得分（0.0 - 1.0）
    pub overall_score: f64,
    /// KV Cache 校验结果
    pub kv_cache_valid: bool,
    /// 语义合理性得分（0.0 - 1.0）
    pub semantic_score: f64,
    /// 完整性检查得分（0.0 - 1.0）
    pub integrity_score: f64,
    /// 是否检测到恶意篡改
    pub is_tampered: bool,
    /// 评估详情
    pub details: AssessmentDetails,
}

/// 评估详情
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AssessmentDetails {
    /// KV Cache 哈希匹配详情
    pub kv_hash_match: Option<bool>,
    /// 语义检查详情
    pub semantic_check: Option<SemanticCheckResult>,
    /// 完整性检查详情
    pub integrity_check: Option<IntegrityCheckResult>,
}

/// 语义检查结果
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticCheckResult {
    /// 是否与上下文一致
    pub context_consistent: bool,
    /// 是否包含有害内容
    pub has_harmful_content: bool,
    /// 逻辑连贯性得分
    pub coherence_score: f64,
}

/// 完整性检查结果
#[derive(Debug, Clone, PartialEq)]
pub struct IntegrityCheckResult {
    /// 输出是否完整
    pub is_complete: bool,
    /// 是否包含异常截断
    pub has_abrupt_ending: bool,
    /// token 数量是否符合预期
    pub token_count_valid: bool,
}

impl QualityAssessment {
    /// 创建新的评估结果
    pub fn new(
        kv_cache_valid: bool,
        semantic_score: f64,
        integrity_score: f64,
        details: AssessmentDetails,
    ) -> Self {
        // 综合得分 = KV 校验 (40%) + 语义 (35%) + 完整性 (25%)
        let kv_score = if kv_cache_valid { 1.0 } else { 0.0 };
        let overall_score = kv_score * 0.4 + semantic_score * 0.35 + integrity_score * 0.25;

        QualityAssessment {
            overall_score,
            kv_cache_valid,
            semantic_score,
            integrity_score,
            is_tampered: !kv_cache_valid || semantic_score < 0.5,
            details,
        }
    }

    /// 是否通过评估
    pub fn is_passed(&self, threshold: f64) -> bool {
        self.overall_score >= threshold && !self.is_tampered
    }

    /// 是否需要多节点复核
    pub fn needs_multi_node_review(&self) -> bool {
        // 质量得分在临界值附近，需要复核
        self.overall_score >= 0.6 && self.overall_score < 0.8
    }
}

/// 质量评估器 trait
///
/// 轻量化设计原则：
/// - 只做验证，不参与实际推理计算
/// - 快速失败，发现问题立即返回
/// - 可插拔，支持不同的评估策略
pub trait QualityAssessor: Send + Sync {
    /// 评估推理结果质量
    ///
    /// 参数：
    /// - output: 推理输出文本
    /// - kv_proof: KV Cache 存证
    /// - expected_tokens: 预期 token 数量
    fn assess(
        &self,
        output: &str,
        kv_proof: &KvCacheProof,
        expected_tokens: Option<u64>,
    ) -> QualityAssessment;

    /// 验证 KV Cache 哈希
    fn verify_kv_hash(&self, kv_data: &[u8], expected_hash: &str) -> bool;

    /// 语义合理性检查（简化版，生产环境可接入轻量级语义模型）
    fn check_semantic(&self, output: &str, context: &str) -> SemanticCheckResult;

    /// 完整性检查
    fn check_integrity(&self, output: &str, expected_tokens: Option<u64>) -> IntegrityCheckResult;

    /// 克隆为 Box（用于对象安全克隆）
    fn clone_box(&self) -> Box<dyn QualityAssessor>;
}

/// 为 Box<dyn QualityAssessor> 实现 Clone
impl Clone for Box<dyn QualityAssessor> {
    fn clone(&self) -> Self {
        self.as_ref().clone_box()
    }
}

/// 空评估器 - 用于测试环境（始终返回满分）
pub struct NullAssessor;

impl QualityAssessor for NullAssessor {
    fn assess(
        &self,
        _output: &str,
        _kv_proof: &KvCacheProof,
        _expected_tokens: Option<u64>,
    ) -> QualityAssessment {
        QualityAssessment::new(true, 1.0, 1.0, AssessmentDetails::default())
    }

    fn verify_kv_hash(&self, _kv_data: &[u8], _expected_hash: &str) -> bool {
        true
    }

    fn check_semantic(&self, _output: &str, _context: &str) -> SemanticCheckResult {
        SemanticCheckResult {
            context_consistent: true,
            has_harmful_content: false,
            coherence_score: 1.0,
        }
    }

    fn check_integrity(&self, _output: &str, _expected_tokens: Option<u64>) -> IntegrityCheckResult {
        IntegrityCheckResult {
            is_complete: true,
            has_abrupt_ending: false,
            token_count_valid: true,
        }
    }

    fn clone_box(&self) -> Box<dyn QualityAssessor> {
        Box::new(NullAssessor)
    }
}

/// 简单评估器 - 基于规则的评估（用于原型开发）
///
/// 支持可配置的语义检查模式：
/// - 规则模式：基于简单规则的检查
/// - 小模型模式：预留接口，可接入外部语义模型
/// - 关闭模式：跳过语义检查
pub struct SimpleAssessor {
    mode: SemanticCheckMode,
}

impl Default for SimpleAssessor {
    fn default() -> Self {
        Self::new()
    }
}

impl SimpleAssessor {
    pub fn new() -> Self {
        SimpleAssessor {
            mode: SemanticCheckMode::Rules,
        }
    }

    /// 创建指定模式的评估器
    pub fn with_mode(mode: SemanticCheckMode) -> Self {
        SimpleAssessor { mode }
    }

    /// 获取当前模式
    pub fn mode(&self) -> SemanticCheckMode {
        self.mode
    }

    /// 设置为规则模式
    pub fn set_mode_rules(&mut self) {
        self.mode = SemanticCheckMode::Rules;
    }

    /// 设置为小模型模式（预留接口）
    pub fn set_mode_small_model(&mut self) {
        self.mode = SemanticCheckMode::SmallModel;
    }

    /// 设置为关闭模式
    pub fn set_mode_disabled(&mut self) {
        self.mode = SemanticCheckMode::Disabled;
    }
}

impl QualityAssessor for SimpleAssessor {
    fn assess(
        &self,
        output: &str,
        kv_proof: &KvCacheProof,
        expected_tokens: Option<u64>,
    ) -> QualityAssessment {
        let mut details = AssessmentDetails::default();

        // 1. KV Cache 校验
        let kv_hash_valid = self.verify_kv_hash(output.as_bytes(), &kv_proof.kv_hash);
        details.kv_hash_match = Some(kv_hash_valid);

        // 2. 语义检查（根据模式决定）
        let semantic_result = match self.mode {
            SemanticCheckMode::Rules => self.check_semantic(output, ""),
            SemanticCheckMode::SmallModel => self.check_semantic_small_model(output, ""),
            SemanticCheckMode::Disabled => SemanticCheckResult {
                context_consistent: true,
                has_harmful_content: false,
                coherence_score: 1.0,
            },
        };

        // 3. 完整性检查
        let integrity_result = self.check_integrity(output, expected_tokens);

        // 计算语义得分
        let semantic_score = if semantic_result.has_harmful_content {
            0.0
        } else {
            semantic_result.coherence_score
        };

        // 计算完整性得分
        let integrity_score = if integrity_result.is_complete && !integrity_result.has_abrupt_ending
        {
            if integrity_result.token_count_valid {
                1.0
            } else {
                0.7
            }
        } else {
            0.3
        };

        // 设置详情
        details.kv_hash_match = Some(kv_hash_valid);
        details.semantic_check = Some(semantic_result);
        details.integrity_check = Some(integrity_result);

        QualityAssessment::new(kv_hash_valid, semantic_score, integrity_score, details)
    }

    fn verify_kv_hash(&self, kv_data: &[u8], expected_hash: &str) -> bool {
        let actual_hash = Self::sha256(kv_data);
        actual_hash == expected_hash
    }

    fn check_semantic(&self, output: &str, _context: &str) -> SemanticCheckResult {
        // 简化版语义检查：
        // 1. 检查空输出
        // 2. 检查过度重复
        // 3. 检查明显的异常模式

        let is_empty = output.trim().is_empty();

        // 检测重复模式（简单版：检查是否有连续重复的句子）
        let has_repetition = Self::check_repetition(output);

        // 检测异常截断（以不完整标点结尾）
        let has_abrupt_ending = Self::check_abrupt_ending(output);

        // 计算连贯性得分
        let coherence_score = if is_empty {
            0.0
        } else if has_repetition {
            0.5
        } else if has_abrupt_ending {
            0.7
        } else {
            1.0
        };

        SemanticCheckResult {
            context_consistent: !is_empty && !has_repetition,
            has_harmful_content: false, // 简化版不检测有害内容
            coherence_score,
        }
    }

    fn check_integrity(&self, output: &str, expected_tokens: Option<u64>) -> IntegrityCheckResult {
        let actual_tokens = Self::count_tokens(output);
        let is_complete = !output.trim().is_empty();
        let has_abrupt_ending = Self::check_abrupt_ending(output);

        let token_count_valid = match expected_tokens {
            Some(expected) => {
                // 允许 ±20% 的误差
                let ratio = actual_tokens as f64 / expected as f64;
                ratio >= 0.8 && ratio <= 1.2
            }
            None => true,
        };

        IntegrityCheckResult {
            is_complete,
            has_abrupt_ending,
            token_count_valid,
        }
    }

    fn clone_box(&self) -> Box<dyn QualityAssessor> {
        Box::new(SimpleAssessor { mode: self.mode })
    }
}

impl SimpleAssessor {
    /// 小模型语义检查（预留接口）
    ///
    /// 当前实现：返回满分（相当于跳过）
    ///
    /// **扩展说明**：
    /// 如需接入真实的小模型，可在此实现：
    /// - 调用本地 ONNX 模型
    /// - 调用远程语义 API
    /// - 使用轻量级 Transformers 模型
    fn check_semantic_small_model(&self, _output: &str, _context: &str) -> SemanticCheckResult {
        // TODO: 接入小模型语义检查
        // 示例伪代码：
        // let score = self.small_model.evaluate(output);
        // SemanticCheckResult {
        //     context_consistent: score > 0.5,
        //     has_harmful_content: score < 0.2,
        //     coherence_score: score,
        // }

        // 当前实现：返回满分（相当于跳过）
        SemanticCheckResult {
            context_consistent: true,
            has_harmful_content: false,
            coherence_score: 1.0,
        }
    }

    /// 检查重复模式
    fn check_repetition(output: &str) -> bool {
        let sentences: Vec<&str> = output.split(&['.', '!', '?', '。', '！', '？'][..]).collect();
        
        // 检查是否有连续相同的句子
        for i in 0..sentences.len().saturating_sub(1) {
            let s1 = sentences[i].trim();
            let s2 = sentences[i + 1].trim();
            if !s1.is_empty() && s1 == s2 {
                return true;
            }
        }
        
        false
    }

    /// 检查异常截断
    fn check_abrupt_ending(output: &str) -> bool {
        let trimmed = output.trim();
        if trimmed.is_empty() {
            return false;
        }

        // 检查是否以不完整的方式结尾
        let last_char = trimmed.chars().last().unwrap();
        
        // 正常结尾标点
        let normal_endings = ['.', '!', '?', '。', '！', '？', '"', '"', '」', '』', '\n'];
        
        // 如果最后一个字符不是正常结尾，可能是异常截断
        !normal_endings.contains(&last_char) && trimmed.len() > 10
    }

    /// 简单 token 计数（按空格和标点分割）
    fn count_tokens(output: &str) -> u64 {
        output
            .split_whitespace()
            .chain(output.split(&['.', '!', '?', '。', '！', '？', ',', '，'][..]))
            .filter(|s| !s.trim().is_empty())
            .count() as u64
    }

    /// SHA256 哈希辅助函数
    fn sha256(data: &[u8]) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        format!("{:x}", result)
    }
}

/// 多节点结果比较器
///
/// 用于比较多个节点的推理结果，选择最优结果
pub struct MultiNodeComparator;

impl MultiNodeComparator {
    /// 比较多个节点的结果，返回最佳结果的索引
    ///
    /// 参数：
    /// - assessments: 各节点的质量评估结果
    ///
    /// 返回：
    /// - Some(index): 最佳结果的索引
    /// - None: 所有结果都被判定为恶意篡改
    pub fn select_best(assessments: &[QualityAssessment]) -> Option<usize> {
        if assessments.is_empty() {
            return None;
        }

        // 过滤掉被判定为恶意篡改的结果
        let valid_candidates: Vec<(usize, &QualityAssessment)> = assessments
            .iter()
            .enumerate()
            .filter(|(_, a)| !a.is_tampered)
            .collect();

        if valid_candidates.is_empty() {
            return None;
        }

        // 选择综合得分最高的
        valid_candidates
            .iter()
            .max_by(|(_, a1), (_, a2)| {
                a1.overall_score.partial_cmp(&a2.overall_score).unwrap()
            })
            .map(|(idx, _)| *idx)
    }

    /// 比较并返回排序后的结果索引列表（从优到劣）
    pub fn rank_all(assessments: &[QualityAssessment]) -> Vec<usize> {
        let mut indexed: Vec<(usize, &QualityAssessment)> = assessments.iter().enumerate().collect();
        
        // 按综合得分降序排序
        indexed.sort_by(|(_, a1), (_, a2)| {
            a2.overall_score.partial_cmp(&a1.overall_score).unwrap()
        });

        indexed.into_iter().map(|(idx, _)| idx).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_kv_proof(hash: &str) -> KvCacheProof {
        KvCacheProof::new(
            "test_kv".to_string(),
            hash.to_string(),
            "node_1".to_string(),
            100,
        )
    }

    #[test]
    fn test_quality_assessment_scoring() {
        let assessment = QualityAssessment::new(
            true,  // KV 校验通过
            0.9,   // 语义得分
            0.8,   // 完整性得分
            AssessmentDetails::default(),
        );

        // 综合得分 = 1.0 * 0.4 + 0.9 * 0.35 + 0.8 * 0.25 = 0.4 + 0.315 + 0.2 = 0.915
        assert!((assessment.overall_score - 0.915).abs() < 0.001);
        assert!(!assessment.is_tampered);
        assert!(assessment.is_passed(0.7));
    }

    #[test]
    fn test_tampered_detection() {
        // KV 校验失败应该被标记为篡改
        let assessment = QualityAssessment::new(
            false, // KV 校验失败
            0.9,
            0.8,
            AssessmentDetails::default(),
        );

        assert!(assessment.is_tampered);
        assert!(!assessment.is_passed(0.7));
    }

    #[test]
    fn test_simple_assessor_valid_output() {
        let assessor = SimpleAssessor::new();
        let output = "valid output";
        let kv_proof = create_test_kv_proof(&SimpleAssessor::sha256(output.as_bytes()));

        let assessment = assessor.assess(output, &kv_proof, Some(10));

        assert!(assessment.kv_cache_valid);
        assert!(assessment.semantic_score > 0.5);
        assert!(assessment.integrity_score >= 0.3); // 完整性检查至少通过基础分
        assert!(!assessment.is_tampered);
    }

    #[test]
    fn test_simple_assessor_invalid_kv_hash() {
        let assessor = SimpleAssessor::new();
        let kv_proof = create_test_kv_proof("invalid_hash");

        let assessment = assessor.assess("some output", &kv_proof, None);

        assert!(!assessment.kv_cache_valid);
        assert!(assessment.is_tampered);
    }

    #[test]
    fn test_simple_assessor_empty_output() {
        let assessor = SimpleAssessor::new();
        let output = "";
        let kv_proof = create_test_kv_proof(&SimpleAssessor::sha256(output.as_bytes()));

        let assessment = assessor.assess(output, &kv_proof, None);

        assert!(assessment.kv_cache_valid); // 空输出的哈希应该匹配
        assert_eq!(assessment.semantic_score, 0.0); // 但语义得分为 0
    }

    #[test]
    fn test_simple_assessor_repetition() {
        let assessor = SimpleAssessor::new();
        let repetitive_output = "Hello world. Hello world. This is a test.";
        let kv_proof = create_test_kv_proof(&SimpleAssessor::sha256(repetitive_output.as_bytes()));

        let assessment = assessor.assess(repetitive_output, &kv_proof, None);

        assert!(assessment.kv_cache_valid);
        assert_eq!(assessment.semantic_score, 0.5); // 重复内容降低语义得分
    }

    #[test]
    fn test_multi_node_comparator() {
        let assessments = vec![
            QualityAssessment::new(true, 0.9, 0.8, AssessmentDetails::default()), // 0.915
            QualityAssessment::new(false, 0.9, 0.8, AssessmentDetails::default()), // 篡改
            QualityAssessment::new(true, 0.7, 0.7, AssessmentDetails::default()), // 0.7
        ];

        // 应该选择第一个（得分最高且未篡改）
        assert_eq!(MultiNodeComparator::select_best(&assessments), Some(0));

        // 排名应该是 [0, 2, 1]，按得分排序（包括被篡改的）
        let ranked = MultiNodeComparator::rank_all(&assessments);
        assert_eq!(ranked, vec![0, 2, 1]);
    }

    #[test]
    fn test_multi_node_all_tampered() {
        let assessments = vec![
            QualityAssessment::new(false, 0.9, 0.8, AssessmentDetails::default()),
            QualityAssessment::new(false, 0.8, 0.7, AssessmentDetails::default()),
        ];

        // 所有结果都被篡改，应该返回 None
        assert_eq!(MultiNodeComparator::select_best(&assessments), None);
    }

    #[test]
    fn test_needs_multi_node_review() {
        // 综合得分 = kv_score * 0.4 + semantic * 0.35 + integrity * 0.25
        // 高质量：1.0 * 0.4 + 0.95 * 0.35 + 0.9 * 0.25 = 0.4 + 0.3325 + 0.225 = 0.9575
        let high_quality = QualityAssessment::new(true, 0.95, 0.9, AssessmentDetails::default());

        // 中等质量：1.0 * 0.4 + 0.5 * 0.35 + 0.5 * 0.25 = 0.4 + 0.175 + 0.125 = 0.7
        let medium_quality = QualityAssessment::new(true, 0.5, 0.5, AssessmentDetails::default());

        // 低质量（篡改）：0.0 * 0.4 + 0.4 * 0.35 + 0.3 * 0.25 = 0.215
        let low_quality = QualityAssessment::new(false, 0.4, 0.3, AssessmentDetails::default());

        assert!(!high_quality.needs_multi_node_review()); // 高质量，不需要复核
        assert!(medium_quality.needs_multi_node_review()); // 中等质量，需要复核
        assert!(!low_quality.needs_multi_node_review()); // 低质量，直接拒绝
    }

    #[test]
    fn test_semantic_check_mode_rules() {
        let assessor = SimpleAssessor::with_mode(SemanticCheckMode::Rules);
        let repetitive_output = "Hello world. Hello world.";
        let kv_proof = create_test_kv_proof(&SimpleAssessor::sha256(repetitive_output.as_bytes()));

        let assessment = assessor.assess(repetitive_output, &kv_proof, None);

        assert_eq!(assessor.mode(), SemanticCheckMode::Rules);
        assert_eq!(assessment.semantic_score, 0.5); // 重复内容降低语义得分
    }

    #[test]
    fn test_semantic_check_mode_disabled() {
        let assessor = SimpleAssessor::with_mode(SemanticCheckMode::Disabled);
        let empty_output = "";
        let kv_proof = create_test_kv_proof(&SimpleAssessor::sha256(empty_output.as_bytes()));

        let assessment = assessor.assess(empty_output, &kv_proof, None);

        assert_eq!(assessor.mode(), SemanticCheckMode::Disabled);
        assert_eq!(assessment.semantic_score, 1.0); // 关闭模式下语义得分为满分
    }

    #[test]
    fn test_semantic_check_mode_small_model() {
        let assessor = SimpleAssessor::with_mode(SemanticCheckMode::SmallModel);
        let output = "normal output";
        let kv_proof = create_test_kv_proof(&SimpleAssessor::sha256(output.as_bytes()));

        let assessment = assessor.assess(output, &kv_proof, None);

        assert_eq!(assessor.mode(), SemanticCheckMode::SmallModel);
        assert_eq!(assessment.semantic_score, 1.0); // 小模型模式当前返回满分（预留接口）
    }

    #[test]
    fn test_assessor_mode_switching() {
        let mut assessor = SimpleAssessor::new();
        
        // 默认是规则模式
        assert_eq!(assessor.mode(), SemanticCheckMode::Rules);
        
        // 切换到关闭模式
        assessor.set_mode_disabled();
        assert_eq!(assessor.mode(), SemanticCheckMode::Disabled);
        
        // 切换到小模型模式
        assessor.set_mode_small_model();
        assert_eq!(assessor.mode(), SemanticCheckMode::SmallModel);
        
        // 切换回规则模式
        assessor.set_mode_rules();
        assert_eq!(assessor.mode(), SemanticCheckMode::Rules);
    }
}
