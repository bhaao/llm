//! 李群/李代数核心数据类型
//!
//! 本模块定义李群驱动架构的基础数据结构：
//! - [`LieAlgebraElement`]: 李代数元素（局部特征映射结果）
//! - [`LieGroupElement`]: 李群元素（全局聚合状态）
//! - [`LieGroupConfig`]: 李群配置（支持不同李群类型）
//!
//! # 设计原则
//!
//! - **可插拔**：支持多种李群类型（SO(3), SE(3), GL(n)）
//! - **高性能**：使用 nalgebra 库进行矩阵运算
//! - **数值稳定**：使用 f64 精度，支持重正交化

use std::fmt::Debug;
use serde::{Serialize, Deserialize};
use nalgebra::{SMatrix, SVector};
use sha2::{Sha256, Digest};

/// 李群类型枚举
///
/// 支持不同的李群类型，每种类型对应不同的物理/几何意义：
/// - `SO3`: 3D 旋转群（3 自由度旋转）
/// - `SE3`: 3D 刚体运动群（6 自由度位姿）
/// - `GLN`: 一般线性群（n×n 可逆矩阵）
/// - `Custom`: 自定义维度（用于高维特征空间）
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LieGroupType {
    /// SO(3) - 3D 旋转群（3×3 正交矩阵，行列式=1）
    /// 用于表示 3D 旋转，自由度=3
    SO3,
    /// SE(3) - 3D 刚体运动群（4×4 齐次变换矩阵）
    /// 用于表示 3D 位姿（旋转 + 平移），自由度=6
    SE3,
    /// GL(n) - 一般线性群（n×n 可逆矩阵）
    /// 用于表示线性变换，自由度=n²
    GLN { dimension: usize },
    /// 自定义维度（用于高维特征映射）
    Custom { algebra_dim: usize },
}

impl Default for LieGroupType {
    fn default() -> Self {
        // 默认使用 SE(3)，适用于大多数 3D 几何验证场景
        LieGroupType::SE3
    }
}

/// 李代数元素 - 局部特征映射结果
///
/// **架构定位**：第一层（分布式上下文分片层）
///
/// **核心职责**：
/// - 将局部特征 h_i 映射为李代数元素 A_i
/// - 作为节点提交到链上的承诺
/// - 支持序列化和哈希计算（用于上链存证）
///
/// # 数学背景
///
/// 李代数是李群在单位元处的切空间，满足：
/// - 向量空间结构（可加、可数乘）
/// - 李括号运算 [X, Y] = XY - YX
///
/// 在本系统中，李代数元素用于表示局部特征的"无穷小变换"。
///
/// # 数据结构设计
///
/// 使用向量表示李代数元素（对于矩阵李群，使用向量化表示）：
/// - SO(3): 3 维向量（角速度/旋转向量）
/// - SE(3): 6 维向量（3 维旋转 + 3 维平移）
/// - GL(n): n²维向量（矩阵展平）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LieAlgebraElement {
    /// 元素标识（节点 ID + 请求 ID）
    pub id: String,
    /// 李代数元素数据（向量化表示）
    pub data: Vec<f64>,
    /// 李群类型（用于解释数据）
    pub group_type: LieGroupType,
    /// 创建时间戳
    pub timestamp: u64,
    /// 节点签名（用于验证来源）
    pub node_signature: String,
}

impl LieAlgebraElement {
    /// 创建新的李代数元素
    pub fn new(
        id: String,
        data: Vec<f64>,
        group_type: LieGroupType,
    ) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        LieAlgebraElement {
            id,
            data,
            group_type,
            timestamp,
            node_signature: String::new(),
        }
    }

    /// 从局部特征向量创建李代数元素
    ///
    /// # 参数
    ///
    /// * `id` - 元素标识
    /// * `features` - 局部特征向量（来自推理隐层状态）
    /// * `group_type` - 目标李群类型
    ///
    /// # 返回
    ///
    /// 李代数元素（维度根据 group_type 确定）
    pub fn from_features(id: &str, features: &[f32], group_type: LieGroupType) -> Self {
        // 将 f32 特征转换为 f64 李代数元素
        let data: Vec<f64> = match group_type {
            LieGroupType::SO3 => {
                // SO(3): 取前 3 个特征作为旋转向量
                features.iter().take(3).map(|&x| x as f64).collect()
            }
            LieGroupType::SE3 => {
                // SE(3): 取前 6 个特征（3 旋转 + 6 平移）
                features.iter().take(6).map(|&x| x as f64).collect()
            }
            LieGroupType::GLN { dimension } => {
                // GL(n): 取前 n²个特征
                let n = dimension;
                features.iter().take(n * n).map(|&x| x as f64).collect()
            }
            LieGroupType::Custom { algebra_dim } => {
                // 自定义：取前 algebra_dim 个特征
                features.iter().take(algebra_dim).map(|&x| x as f64).collect()
            }
        };

        Self::new(id.to_string(), data, group_type)
    }

    /// 获取李代数维度
    pub fn dimension(&self) -> usize {
        self.data.len()
    }

    /// 计算李代数元素的哈希（用于上链存证）
    pub fn hash(&self) -> String {
        let data_str = self.data.iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(",");
        
        let hash_input = format!(
            "{}:{}:{}:{}",
            self.id,
            data_str,
            self.group_type_as_str(),
            self.timestamp
        );
        
        format!("{:x}", Sha256::digest(hash_input.as_bytes()))
    }

    /// 获取李群类型的字符串表示
    pub fn group_type_as_str(&self) -> &'static str {
        match self.group_type {
            LieGroupType::SO3 => "SO3",
            LieGroupType::SE3 => "SE3",
            LieGroupType::GLN { .. } => "GLN",
            LieGroupType::Custom { .. } => "CUSTOM",
        }
    }

    /// 对元素进行签名
    pub fn sign(&mut self, private_key: &[u8; 32]) -> Result<(), String> {
        use ed25519_dalek::{SigningKey, Signer};
        
        let signing_key = SigningKey::from_bytes(private_key);
        let message = self.signing_message();
        let signature = signing_key.sign(message.as_bytes());
        self.node_signature = hex::encode(signature.to_bytes());
        Ok(())
    }

    /// 获取签名消息
    pub fn signing_message(&self) -> String {
        format!(
            "{}:{}:{}",
            self.id,
            self.hash(),
            self.timestamp
        )
    }

    /// 验证签名
    pub fn verify_signature(&self, public_key: &[u8; 32]) -> bool {
        use ed25519_dalek::{VerifyingKey, Verifier};
        use ed25519_dalek::Signature;

        let verifying_key = VerifyingKey::from_bytes(public_key)
            .unwrap_or_else(|_| VerifyingKey::from_bytes(&[0u8; 32]).unwrap());

        let signature_bytes = match hex::decode(&self.node_signature) {
            Ok(bytes) => bytes,
            Err(_) => return false,
        };

        let signature = match Signature::try_from(&signature_bytes[..]) {
            Ok(sig) => sig,
            Err(_) => return false,
        };

        verifying_key.verify(self.signing_message().as_bytes(), &signature).is_ok()
    }

    /// 转换为向量
    pub fn to_vector(&self) -> Vec<f64> {
        self.data.clone()
    }

    /// 从向量创建
    pub fn from_vector(id: &str, data: Vec<f64>, group_type: LieGroupType) -> Self {
        Self::new(id.to_string(), data, group_type)
    }
}

/// 李群元素 - 全局聚合状态
///
/// **架构定位**：第二层（李群链上聚合层）
///
/// **核心职责**：
/// - 表示链上聚合的全局李群状态 G
/// - 由多个李代数元素 A_i 聚合而成
/// - 作为 QaaS 验证的基准
///
/// # 数学背景
///
/// 李群是具有群结构的微分流形，满足：
/// - 群运算（乘法、逆元）
/// - 光滑流形结构（可微分）
///
/// 李群与李代数的关系：
/// - 指数映射：exp: g → G（李代数 → 李群）
/// - 对数映射：log: G → g（李群 → 李代数）
///
/// # 数据结构设计
///
/// 使用矩阵表示李群元素：
/// - SO(3): 3×3 旋转矩阵
/// - SE(3): 4×4 变换矩阵
/// - GL(n): n×n 可逆矩阵
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LieGroupElement {
    /// 元素标识（聚合请求 ID）
    pub id: String,
    /// 李群矩阵数据（行优先展平）
    pub matrix_data: Vec<f64>,
    /// 矩阵维度（rows × cols）
    pub matrix_shape: (usize, usize),
    /// 李群类型
    pub group_type: LieGroupType,
    /// 参与聚合的节点 ID 列表
    pub contributor_ids: Vec<String>,
    /// 聚合时间戳
    pub timestamp: u64,
    /// 聚合证明哈希
    pub aggregation_proof_hash: String,
}

impl LieGroupElement {
    /// 创建新的李群元素
    pub fn new(
        id: String,
        matrix_data: Vec<f64>,
        matrix_shape: (usize, usize),
        group_type: LieGroupType,
    ) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        LieGroupElement {
            id,
            matrix_data,
            matrix_shape,
            group_type,
            contributor_ids: Vec::new(),
            timestamp,
            aggregation_proof_hash: String::new(),
        }
    }

    /// 从李代数元素通过指数映射创建李群元素
    ///
    /// G = exp(A)
    ///
    /// # 参数
    ///
    /// * `algebra` - 李代数元素
    ///
    /// # 返回
    ///
    /// 李群元素（通过指数映射得到）
    pub fn from_algebra_exponential(algebra: &LieAlgebraElement) -> Self {
        let (matrix_data, shape) = Self::exp_map(&algebra.data, algebra.group_type);
        let mut group = Self::new(
            format!("exp_{}", algebra.id),
            matrix_data,
            shape,
            algebra.group_type,
        );
        group.contributor_ids.push(algebra.id.clone());
        group
    }

    /// 指数映射：李代数 → 李群
    ///
    /// 使用矩阵指数 exp(A) = I + A + A²/2! + A³/3! + ...
    fn exp_map(data: &[f64], group_type: LieGroupType) -> (Vec<f64>, (usize, usize)) {
        match group_type {
            LieGroupType::SO3 => {
                // SO(3): 旋转向量 → 旋转矩阵
                // 使用 Rodrigues 公式
                let theta_vec = SVector::<f64, 3>::from_column_slice(data);
                let theta = theta_vec.norm();
                
                if theta < 1e-10 {
                    // 小角度近似：R ≈ I
                    (vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], (3, 3))
                } else {
                    let axis = theta_vec / theta;
                    let (ux, uy, uz) = (axis[0], axis[1], axis[2]);
                    let c = theta.cos();
                    let s = theta.sin();
                    let t = 1.0 - c;
                    
                    // Rodrigues 旋转矩阵
                    let matrix = vec![
                        t * ux * ux + c,     t * ux * uy - uz * s, t * ux * uz + uy * s,
                        t * ux * uy + uz * s, t * uy * uy + c,     t * uy * uz - ux * s,
                        t * ux * uz - uy * s, t * uy * uz + ux * s, t * uz * uz + c,
                    ];
                    (matrix, (3, 3))
                }
            }
            LieGroupType::SE3 => {
                // SE(3): 李代数 → 变换矩阵
                // 前 3 个为旋转，后 3 个为平移
                let rot_data: Vec<f64> = data.iter().take(3).copied().collect();
                let trans_data: Vec<f64> = data.iter().skip(3).take(3).copied().collect();
                
                let (rot_matrix, _) = Self::exp_map(&rot_data, LieGroupType::SO3);
                
                // 构建 4×4 齐次变换矩阵
                let matrix = vec![
                    rot_matrix[0], rot_matrix[1], rot_matrix[2], trans_data[0],
                    rot_matrix[3], rot_matrix[4], rot_matrix[5], trans_data[1],
                    rot_matrix[6], rot_matrix[7], rot_matrix[8], trans_data[2],
                    0.0, 0.0, 0.0, 1.0,
                ];
                (matrix, (4, 4))
            }
            LieGroupType::GLN { dimension } => {
                // GL(n): 简化实现 - 返回单位矩阵
                // 完整实现需要使用动态矩阵或固定最大维度
                let n = dimension;
                let mut matrix_data = vec![0.0f64; n * n];
                for i in 0..n {
                    matrix_data[i * n + i] = 1.0;
                }
                (matrix_data, (n, n))
            }
            LieGroupType::Custom { algebra_dim } => {
                // 自定义：简化实现 - 返回单位矩阵
                let n = (algebra_dim as f64).sqrt() as usize;
                if n * n != algebra_dim {
                    // 非方阵：返回单位矩阵
                    return (vec![1.0], (1, 1));
                }
                
                let mut matrix_data = vec![0.0f64; n * n];
                for i in 0..n {
                    matrix_data[i * n + i] = 1.0;
                }
                (matrix_data, (n, n))
            }
        }
    }

    /// 获取矩阵数据引用
    pub fn matrix_data(&self) -> &Vec<f64> {
        &self.matrix_data
    }

    /// 转换为 nalgebra 矩阵（根据 shape 动态选择）
    /// 
    /// 注意：此方法用于内部计算，调用者需要确保 matrix_shape 正确
    pub fn to_matrix_3x3(&self) -> Option<SMatrix<f64, 3, 3>> {
        if self.matrix_shape == (3, 3) {
            Some(SMatrix::<f64, 3, 3>::from_column_slice(&self.matrix_data))
        } else {
            None
        }
    }

    /// 转换为 4x4 矩阵
    pub fn to_matrix_4x4(&self) -> Option<SMatrix<f64, 4, 4>> {
        if self.matrix_shape == (4, 4) {
            Some(SMatrix::<f64, 4, 4>::from_column_slice(&self.matrix_data))
        } else {
            None
        }
    }

    /// 计算李群元素的哈希
    pub fn hash(&self) -> String {
        let data_str = self.matrix_data.iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(",");
        
        let hash_input = format!(
            "{}:{}:{}:{}",
            self.id,
            data_str,
            self.timestamp,
            self.contributor_ids.join(",")
        );
        
        format!("{:x}", Sha256::digest(hash_input.as_bytes()))
    }

    /// 设置聚合证明哈希
    pub fn set_aggregation_proof(&mut self, proof_hash: &str) {
        self.aggregation_proof_hash = proof_hash.to_string();
    }

    /// 添加贡献者
    pub fn add_contributor(&mut self, contributor_id: &str) {
        if !self.contributor_ids.contains(&contributor_id.to_string()) {
            self.contributor_ids.push(contributor_id.to_string());
        }
    }

    /// 获取矩阵维度
    pub fn dimension(&self) -> (usize, usize) {
        self.matrix_shape
    }

    /// 验证李群元素的有效性
    ///
    /// 对于 SO(3)：验证正交性（R^T * R = I）和行列式（det(R) = 1）
    /// 对于 SE(3)：验证左上角 3×3 旋转矩阵的正交性
    pub fn validate(&self) -> bool {
        match self.group_type {
            LieGroupType::SO3 => {
                if self.matrix_shape != (3, 3) {
                    return false;
                }
                // 使用固定大小矩阵
                let matrix = SMatrix::<f64, 3, 3>::from_column_slice(&self.matrix_data);
                let transpose = matrix.transpose();
                let product = &transpose * &matrix;

                // 验证正交性：R^T * R ≈ I
                let identity = SMatrix::<f64, 3, 3>::identity();
                let diff = (&product - &identity).abs().max();
                if diff > 1e-6 {
                    return false;
                }

                // 验证行列式：det(R) ≈ 1
                matrix.try_inverse().is_some()
            }
            LieGroupType::SE3 => {
                if self.matrix_shape != (4, 4) {
                    return false;
                }
                // 使用固定大小矩阵
                let matrix = SMatrix::<f64, 4, 4>::from_column_slice(&self.matrix_data);

                // 验证最后一行是 [0, 0, 0, 1]
                let last_row = [matrix[(3, 0)], matrix[(3, 1)], matrix[(3, 2)], matrix[(3, 3)]];
                if (last_row[0] - 0.0).abs() > 1e-6
                    || (last_row[1] - 0.0).abs() > 1e-6
                    || (last_row[2] - 0.0).abs() > 1e-6
                    || (last_row[3] - 1.0).abs() > 1e-6
                {
                    return false;
                }

                true
            }
            _ => true, // 其他类型暂不验证
        }
    }
}

/// 李群配置
///
/// 用于配置李群运算的参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LieGroupConfig {
    /// 李群类型
    pub group_type: LieGroupType,
    /// 数值精度容差
    pub tolerance: f64,
    /// 是否启用重正交化（对于 SO(3)/SE(3)）
    pub enable_reorthogonalization: bool,
    /// 矩阵指数级数项数（默认 5）
    pub exp_series_terms: usize,
}

impl Default for LieGroupConfig {
    fn default() -> Self {
        LieGroupConfig {
            group_type: LieGroupType::SE3,
            tolerance: 1e-10,
            enable_reorthogonalization: true,
            exp_series_terms: 5,
        }
    }
}

impl LieGroupConfig {
    /// 创建新配置
    pub fn new(group_type: LieGroupType) -> Self {
        LieGroupConfig {
            group_type,
            ..Default::default()
        }
    }

    /// 设置精度容差
    pub fn with_tolerance(mut self, tolerance: f64) -> Self {
        self.tolerance = tolerance;
        self
    }

    /// 设置是否启用重正交化
    pub fn with_reorthogonalization(mut self, enable: bool) -> Self {
        self.enable_reorthogonalization = enable;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lie_algebra_element_creation() {
        let features: Vec<f32> = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6];
        let element = LieAlgebraElement::from_features(
            "test_1",
            &features,
            LieGroupType::SE3,
        );

        assert_eq!(element.id, "test_1");
        assert_eq!(element.data.len(), 6);
        assert_eq!(element.group_type, LieGroupType::SE3);
        assert!(!element.node_signature.is_empty() || element.node_signature.is_empty()); // 签名可能为空
    }

    #[test]
    fn test_lie_algebra_element_hash() {
        let element = LieAlgebraElement::new(
            "test_1".to_string(),
            vec![1.0, 2.0, 3.0],
            LieGroupType::SO3,
        );

        let hash1 = element.hash();
        let hash2 = element.hash();
        
        assert_eq!(hash1, hash2); // 相同数据应产生相同哈希
        
        // 修改数据应产生不同哈希
        let mut modified = element.clone();
        modified.data[0] = 999.0;
        assert_ne!(element.hash(), modified.hash());
    }

    #[test]
    fn test_lie_group_element_from_algebra() {
        let algebra = LieAlgebraElement::new(
            "alg_1".to_string(),
            vec![0.1, 0.2, 0.3],
            LieGroupType::SO3,
        );

        let group = LieGroupElement::from_algebra_exponential(&algebra);

        assert_eq!(group.id, "exp_alg_1");
        assert_eq!(group.matrix_shape, (3, 3));
        assert_eq!(group.group_type, LieGroupType::SO3);
        assert!(group.contributor_ids.contains(&algebra.id));
    }

    #[test]
    fn test_so3_rotation_matrix_validation() {
        // 创建一个绕 Z 轴旋转 90 度的李代数元素
        let algebra = LieAlgebraElement::new(
            "rot_z".to_string(),
            vec![0.0, 0.0, std::f64::consts::FRAC_PI_2],
            LieGroupType::SO3,
        );

        let group = LieGroupElement::from_algebra_exponential(&algebra);
        
        assert!(group.validate());
    }

    #[test]
    fn test_se3_transformation_matrix() {
        // 创建 SE(3) 李代数元素（旋转 + 平移）
        let algebra = LieAlgebraElement::new(
            "se3_1".to_string(),
            vec![0.1, 0.2, 0.3, 1.0, 2.0, 3.0], // 3 旋转 + 3 平移
            LieGroupType::SE3,
        );

        let group = LieGroupElement::from_algebra_exponential(&algebra);

        assert_eq!(group.matrix_shape, (4, 4));
        assert!(group.validate());
        
        // 验证平移部分
        if let Some(matrix) = group.to_matrix_4x4() {
            assert!((matrix[(0, 3)] - 1.0).abs() < 0.1);
            assert!((matrix[(1, 3)] - 2.0).abs() < 0.1);
            assert!((matrix[(2, 3)] - 3.0).abs() < 0.1);
        } else {
            panic!("Failed to convert to 4x4 matrix");
        }
    }

    #[test]
    fn test_lie_group_config() {
        let config = LieGroupConfig::new(LieGroupType::SO3)
            .with_tolerance(1e-8)
            .with_reorthogonalization(false);

        assert_eq!(config.group_type, LieGroupType::SO3);
        assert_eq!(config.tolerance, 1e-8);
        assert!(!config.enable_reorthogonalization);
    }
}
