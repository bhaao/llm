use block_chain_with_context::{Blockchain, Transaction, TransactionType, TransactionPayload};
use block_chain_with_context::{BlockMetadata, KvCacheProof};
use block_chain_with_context::Serializable;

fn main() {
    // 创建区块链（用于分布式推理的可信记录）
    let mut blockchain = Blockchain::new("user_address".to_string());

    // 注册推理节点（创新 B：链上可信调度）
    blockchain.register_node("node_1".to_string());
    blockchain.register_node("node_2".to_string());

    // 添加推理请求（使用 Internal 交易类型，无需签名验证）
    let tx = Transaction::new_internal(
        "user".to_string(),
        "node_1".to_string(),
        TransactionType::InferenceRequest,
        TransactionPayload::InferenceRequest {
            prompt: "解释量子纠缠".to_string(),
            model_id: "llama-7b".to_string(),
            max_tokens: 500,
        },
    );
    blockchain.add_pending_transaction(tx);

    // 添加 KV Cache 存证（创新 A：KV Cache 链上存证）
    let kv_proof = KvCacheProof::new(
        "kv_001".to_string(),
        "kv_hash_abc123".to_string(),
        "node_1".to_string(),
        1024,
    );
    blockchain.add_kv_proof(kv_proof);

    // 提交推理记录到链上
    let metadata = BlockMetadata::new(
        "Llama-7B".to_string(),
        "1.0.0".to_string(),
        50,
        100,
        250,
        0.001,
        "Meta".to_string(),
    );

    // 先获取需要的数据，避免借用冲突
    let commit_result = blockchain.commit_inference(metadata, "node_1".to_string());
    
    match commit_result {
        Ok(_) => {
            // 获取最新区块用于显示
            if let Some(block) = blockchain.latest_block() {
                let block_json = block.to_json().unwrap_or_else(|e| format!("Error: {}", e));
                let block_index = block.index;
                let block_hash = block.hash.clone();
                let tx_count = block.transaction_count();
                let kv_count = block.kv_proof_count();
                let total_tokens = block.total_tokens();

                println!("=== 分布式推理记录已上链 ===");
                println!("区块高度：{}", block_index);
                println!("区块哈希：{}", block_hash);
                println!("交易数量：{}", tx_count);
                println!("KV 存证数量：{}", kv_count);
                println!("总 Token 数：{}", total_tokens);
                println!("链验证：{}", blockchain.verify_chain());
                println!();

                // 展示节点信誉（创新 B）
                println!("=== 节点信誉 ===");
                println!("可信节点数：{}/{}",
                    blockchain.get_trustworthy_nodes().len(),
                    blockchain.node_count()
                );

                if let Some(node) = blockchain.get_node_reputation("node_1") {
                    println!("node_1 信誉分：{:.2}", node.score);
                    println!("node_1 完成任务数：{}", node.completed_tasks);
                    println!("node_1 处理 Token 数：{}", node.total_tokens_processed);
                }
                println!();

                // 展示 KV 存证（创新 A）
                println!("=== KV Cache 存证 ===");
                let all_proofs = blockchain.get_all_kv_proofs();
                for proof in all_proofs {
                    println!("KV 块：{}, 哈希：{}, 节点：{}",
                        proof.kv_block_id, proof.kv_hash, proof.node_id);
                }
                println!();

                // 展示 JSON 序列化（问题 1 修复）
                println!("=== 区块 JSON 序列化 ===");
                println!("{}", block_json);
            }
        }
        Err(e) => {
            println!("提交失败：{}", e);
        }
    }
}
