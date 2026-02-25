//! 工具函数模块
//!
//! 提供通用的加密和数据结构工具函数

use crate::traits::Hashable;
use sha2::{Sha256, Digest};

/// 计算默克尔根
///
/// 默克尔树是一种二叉树结构，用于高效验证大数据集的完整性：
/// - 叶子节点是数据项的哈希
/// - 非叶子节点是子节点哈希的拼接后再哈希
/// - 根节点是整个数据集的唯一指纹
///
/// # 参数
///
/// - `items`: 可哈希的数据项切片
///
/// # 返回
///
/// - 默克尔根哈希字符串（64 字符十六进制）
///
/// # 示例
///
/// ```ignore
/// use block_chain_with_context::{utils::merkle_root, Hashable};
///
/// let items = vec!["a".to_string(), "b".to_string(), "c".to_string()];
/// let root = merkle_root(&items);
/// ```
pub fn merkle_root(items: &[impl Hashable]) -> String {
    if items.is_empty() {
        return "0000000000000000000000000000000000000000000000000000000000000000".to_string();
    }

    let mut hashes: Vec<String> = items.iter().map(|t| t.hash()).collect();

    while hashes.len() > 1 {
        let mut new_hashes = Vec::new();
        for chunk in hashes.chunks(2) {
            let combined = match chunk.len() {
                2 => format!("{}{}", chunk[0], chunk[1]),
                1 => format!("{}{}", chunk[0], chunk[0]),
                _ => unreachable!(),
            };
            new_hashes.push(sha256(&combined));
        }
        hashes = new_hashes;
    }

    hashes.into_iter().next().unwrap_or_else(|| "0000000000000000000000000000000000000000000000000000000000000000".to_string())
}

/// SHA256 哈希辅助函数
///
/// # 参数
///
/// - `data`: 输入数据字符串
///
/// # 返回
///
/// - SHA256 哈希字符串（64 字符十六进制）
pub fn sha256(data: &str) -> String {
    format!("{:x}", Sha256::digest(data.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::KvCacheProof;

    #[test]
    fn test_merkle_root_empty() {
        let items: Vec<KvCacheProof> = vec![];
        let root = merkle_root(&items);
        assert_eq!(root, "0000000000000000000000000000000000000000000000000000000000000000");
    }

    #[test]
    fn test_merkle_root_single() {
        let items = vec![KvCacheProof::new("test".to_string(), "hash".to_string(), "node".to_string(), 100)];
        let root = merkle_root(&items);
        assert_eq!(root.len(), 64);
    }

    #[test]
    fn test_merkle_root_multiple() {
        let items = vec![
            KvCacheProof::new("a".to_string(), "hash_a".to_string(), "node".to_string(), 100),
            KvCacheProof::new("b".to_string(), "hash_b".to_string(), "node".to_string(), 100),
            KvCacheProof::new("c".to_string(), "hash_c".to_string(), "node".to_string(), 100),
        ];
        let root = merkle_root(&items);
        assert_eq!(root.len(), 64);
    }

    #[test]
    fn test_sha256() {
        let hash = sha256("test");
        assert_eq!(hash.len(), 64);
        // SHA256("test") 的已知哈希值
        assert_eq!(
            hash,
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
        );
    }
}
