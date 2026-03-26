//! CLI 验证工具 - P3-3：命令行验证接口
//!
//! **设计目标**：
//! - 提供命令行工具验证推理包
//! - 支持多种输出格式（JSON/文本）
//! - 支持批量验证
//! - 生成验证报告
//!
//! **命令**：
//! - `verify-inference verify <package.json>` - 验证单个包
//! - `verify-inference verify-batch <dir>` - 批量验证
//! - `verify-inference inspect <package.json>` - 检查包内容
//! - `verify-inference stats` - 显示验证统计
//! - `provider register-ollama` - 注册 Ollama 提供商

use std::path::PathBuf;
use std::fs;
use anyhow::{Result, Context, anyhow};
use clap::{Parser, Subcommand};
use serde::Serialize;

use crate::verifiable_package::VerifiableInferencePackage;
use crate::verification_sdk::{VerificationSDK, VerificationSDKConfig, VerificationReport, VerificationStatus};
use crate::provider_layer::ProviderLayerManager;
use crate::provider_layer::ollama_provider::OllamaProvider;

/// CLI 主结构
#[derive(Parser)]
#[command(name = "verify-inference")]
#[command(author = "Blockchain with Context Team")]
#[command(version = "1.0.0")]
#[command(about = "可验证推理包验证工具", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// 输出格式（json/text）
    #[arg(short, long, default_value = "text")]
    pub format: String,

    /// 详细输出
    #[arg(short, long, default_value = "false")]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// 验证单个推理包
    Verify {
        /// 推理包 JSON 文件路径
        #[arg(value_parser)]
        package_file: PathBuf,
    },
    /// 批量验证目录中的所有推理包
    VerifyBatch {
        /// 包含推理包的目录
        #[arg(value_parser)]
        directory: PathBuf,
    },
    /// 检查推理包内容
    Inspect {
        /// 推理包 JSON 文件路径
        #[arg(value_parser)]
        package_file: PathBuf,
    },
    /// 显示验证统计
    Stats {
        /// 历史验证记录文件（可选）
        #[arg(short, long)]
        _history_file: Option<PathBuf>,
    },
    /// 提供商管理命令
    #[command(subcommand)]
    Provider(ProviderCommands),
}

/// 提供商管理子命令
#[derive(Subcommand, Clone)]
pub enum ProviderCommands {
    /// 注册 Ollama 提供商
    RegisterOllama {
        /// 提供商 ID
        #[arg(long)]
        id: String,

        /// Ollama 服务地址
        #[arg(long, default_value = "http://localhost:11434")]
        url: String,

        /// API Key（可选，线上服务需要）
        #[arg(long)]
        api_key: Option<String>,

        /// 默认模型
        #[arg(long, default_value = "qwen3-coder-next:q8_0")]
        model: String,

        /// 算力容量 (token/s)
        #[arg(long, default_value = "50")]
        capacity: u64,

        /// Token 分割阈值
        #[arg(long, default_value = "4096")]
        token_threshold: u32,
    },
    /// 从环境变量加载 Ollama 提供商配置
    RegisterOllamaFromEnv {
        /// 提供商 ID
        #[arg(long)]
        id: String,
    },
}

/// CLI 执行器
pub struct CliExecutor {
    sdk: VerificationSDK,
    format: OutputFormat,
    verbose: bool,
    /// 提供商管理器（用于注册和管理提供商）
    provider_manager: ProviderLayerManager,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputFormat {
    Json,
    Text,
}

impl CliExecutor {
    /// 创建新的 CLI 执行器
    pub fn new(format: OutputFormat, verbose: bool) -> Self {
        let config = VerificationSDKConfig::default();
        let sdk = VerificationSDK::new(config);

        CliExecutor {
            sdk,
            format,
            verbose,
            provider_manager: ProviderLayerManager::new(),
        }
    }

    /// 执行命令
    pub async fn execute(&self, command: &Commands) -> Result<()> {
        match command {
            Commands::Verify { package_file } => {
                self.verify_package(package_file).await
            }
            Commands::VerifyBatch { directory } => {
                self.verify_batch(directory).await
            }
            Commands::Inspect { package_file } => {
                self.inspect_package(package_file)
            }
            Commands::Stats { _history_file } => {
                self.show_stats(_history_file.as_ref()).await
            }
            Commands::Provider(provider_cmd) => {
                self.handle_provider_command(provider_cmd).await
            }
        }
    }

    /// 验证单个包
    async fn verify_package(&self, package_file: &PathBuf) -> Result<()> {
        // 读取包文件
        let content = fs::read_to_string(package_file)
            .context(format!("Failed to read package file: {:?}", package_file))?;

        // 反序列化包
        let package: VerifiableInferencePackage = VerifiableInferencePackage::from_json(&content)
            .context("Failed to parse package JSON")?;

        // 执行验证
        let report = self.sdk.verify(&package).await
            .context("Verification failed")?;

        // 输出结果
        self.output_verification_result(&report)
    }

    /// 批量验证
    async fn verify_batch(&self, directory: &PathBuf) -> Result<()> {
        if !directory.is_dir() {
            return Err(anyhow!("Path is not a directory: {:?}", directory));
        }

        // 查找所有 JSON 文件
        let mut package_files = Vec::new();
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                package_files.push(path);
            }
        }

        if package_files.is_empty() {
            return Err(anyhow!("No JSON files found in directory: {:?}", directory));
        }

        println!("Found {} package files, starting verification...", package_files.len());

        let mut reports = Vec::new();
        let mut errors = Vec::new();

        for file in &package_files {
            match self.load_and_verify_package(file).await {
                Ok(report) => {
                    reports.push(report);
                }
                Err(e) => {
                    errors.push((file.clone(), e));
                }
            }
        }

        // 输出结果
        self.output_batch_result(&reports, &errors)
    }

    /// 检查包内容
    fn inspect_package(&self, package_file: &PathBuf) -> Result<()> {
        // 读取包文件
        let content = fs::read_to_string(package_file)
            .context(format!("Failed to read package file: {:?}", package_file))?;

        // 反序列化包
        let package: VerifiableInferencePackage = VerifiableInferencePackage::from_json(&content)
            .context("Failed to parse package JSON")?;

        // 输出包信息
        self.output_package_info(&package)
    }

    /// 显示统计
    async fn show_stats(&self, _history_file: Option<&PathBuf>) -> Result<()> {
        let stats = self.sdk.get_verification_stats().await;

        match self.format {
            OutputFormat::Json => {
                let json = serde_json::to_string_pretty(&stats)?;
                println!("{}", json);
            }
            OutputFormat::Text => {
                println!("\n=== 验证统计 ===");
                println!("总验证数：{}", stats.total_verifications);
                println!("有效：{}", stats.valid_count);
                println!("部分有效：{}", stats.partially_valid_count);
                println!("无效：{}", stats.invalid_count);
                println!("平均置信度：{:.2}", stats.average_confidence);
            }
        }

        Ok(())
    }

    /// 处理提供商管理命令
    async fn handle_provider_command(&self, cmd: &ProviderCommands) -> Result<()> {
        match cmd {
            ProviderCommands::RegisterOllama {
                id,
                url,
                api_key,
                model,
                capacity,
                token_threshold,
            } => {
                self.register_ollama_provider(id, url, api_key.as_ref(), model, *capacity, *token_threshold)
            }
            ProviderCommands::RegisterOllamaFromEnv { id } => {
                self.register_ollama_from_env(id)
            }
        }
    }

    /// 注册 Ollama 提供商
    fn register_ollama_provider(
        &self,
        id: &str,
        url: &str,
        api_key: Option<&String>,
        model: &str,
        capacity: u64,
        token_threshold: u32,
    ) -> Result<()> {
        // 创建 Ollama 提供商
        let mut provider = OllamaProvider::new(
            id.to_string(),
            url,
            model.to_string(),
            capacity,
        )
        .with_token_split_threshold(token_threshold);

        // 如果提供了 API Key，设置到提供商
        if let Some(key) = api_key {
            provider = provider.with_api_key(key.clone());
        }

        // 注册到管理器
        let mut manager = self.provider_manager.clone();
        manager.register_provider(Box::new(provider))
            .map_err(|e| anyhow!("Failed to register Ollama provider: {}", e))?;

        // 输出结果
        match self.format {
            OutputFormat::Json => {
                let output = serde_json::json!({
                    "status": "success",
                    "message": "Ollama provider registered successfully",
                    "provider": {
                        "id": id,
                        "url": url,
                        "model": model,
                        "capacity": capacity,
                        "token_threshold": token_threshold,
                        "engine_type": "custom",
                        "api_key_configured": api_key.is_some()
                    }
                });
                println!("{}", serde_json::to_string_pretty(&output)?);
            }
            OutputFormat::Text => {
                println!("\n=== Ollama 提供商注册成功 ===");
                println!("提供商 ID: {}", id);
                println!("服务地址：{}", url);
                println!("默认模型：{}", model);
                println!("算力容量：{} token/s", capacity);
                println!("Token 分割阈值：{}", token_threshold);
                if api_key.is_some() {
                    println!("API Key: 已配置 (****)");
                } else {
                    println!("API Key: 未配置（本地服务不需要）");
                }
                println!();
                println!("提示：使用以下命令测试推理：");
                println!("  cargo run -- provider test --id {}", id);
            }
        }

        Ok(())
    }

    /// 从环境变量加载 Ollama 配置
    fn register_ollama_from_env(&self, id: &str) -> Result<()> {
        // 加载 .env 文件
        dotenv::dotenv().ok();

        // 从环境变量创建提供商
        let provider = OllamaProvider::from_env(id.to_string())
            .map_err(|e| anyhow!("Failed to load Ollama config from env: {}", e))?;

        // 注册到管理器
        let mut manager = self.provider_manager.clone();
        manager.register_provider(Box::new(provider))
            .map_err(|e| anyhow!("Failed to register Ollama provider: {}", e))?;

        // 输出结果
        match self.format {
            OutputFormat::Json => {
                let output = serde_json::json!({
                    "status": "success",
                    "message": "Ollama provider registered from environment variables",
                    "provider": {
                        "id": id,
                        "url": std::env::var("OLLAMA_URL").unwrap_or_else(|_| "default".to_string()),
                        "model": std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "default".to_string()),
                        "api_key_configured": std::env::var("OLLAMA_API_KEY").is_ok()
                    }
                });
                println!("{}", serde_json::to_string_pretty(&output)?);
            }
            OutputFormat::Text => {
                println!("\n=== 从环境变量加载 Ollama 配置 ===");
                println!("提供商 ID: {}", id);
                println!("服务地址：{}", std::env::var("OLLAMA_URL").unwrap_or_else(|_| "http://localhost:11434".to_string()));
                println!("默认模型：{}", std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "qwen3-coder-next:q8_0".to_string()));
                if std::env::var("OLLAMA_API_KEY").is_ok() {
                    println!("API Key: 已配置 (****)");
                } else {
                    println!("API Key: 未配置");
                }
                println!();
                println!("提示：编辑 .env 文件修改配置");
            }
        }

        Ok(())
    }

    // ========== 内部方法 ==========

    async fn load_and_verify_package(&self, file: &PathBuf) -> Result<VerificationReport> {
        let content = fs::read_to_string(file)?;
        let package: VerifiableInferencePackage = VerifiableInferencePackage::from_json(&content)?;
        let report = self.sdk.verify(&package).await?;
        Ok(report)
    }

    fn output_verification_result(&self, report: &VerificationReport) -> Result<()> {
        match self.format {
            OutputFormat::Json => {
                let json = serde_json::to_string_pretty(report)?;
                println!("{}", json);
            }
            OutputFormat::Text => {
                self.print_text_report(report);
            }
        }

        // 根据验证结果设置退出码
        if report.overall_result == VerificationStatus::Invalid {
            std::process::exit(1);
        }

        Ok(())
    }

    fn print_text_report(&self, report: &VerificationReport) {
        println!("\n=== 验证报告 ===");
        println!("报告 ID: {}", report.report_id);
        println!("包 ID: {}", report.package_id);
        println!("验证时间：{}", format_timestamp(report.verified_at));
        println!();

        // 总体结果
        let status_icon = match report.overall_result {
            VerificationStatus::Valid => "✅",
            VerificationStatus::PartiallyValid => "⚠️",
            VerificationStatus::Invalid => "❌",
            VerificationStatus::Unverifiable => "❓",
        };
        println!("总体结果：{} {:?}", status_icon, report.overall_result);
        println!("置信度：{:.2}", report.confidence);
        println!();

        // 包验证
        println!("--- 包完整性 ---");
        let pkg_icon = if report.package_verification.is_valid { "✅" } else { "❌" };
        println!("{} 包哈希验证：{}", pkg_icon, 
            if report.package_verification.is_valid { "通过" } else { "失败" });
        println!("  置信度：{:.2}", report.package_verification.confidence);
        println!();

        // 质量验证
        if let Some(ref quality) = report.quality_verification {
            println!("--- 质量证明 ---");
            let q_icon = if quality.is_valid { "✅" } else { "❌" };
            println!("{} 质量分数：{:.2}", q_icon, quality.quality_score);
            println!("  证明 ID: {}", quality.proof_id);
            println!("  验证器：{}", quality.validator_id);
            println!("  详情：{}", quality.details);
            println!();
        }

        // 共识验证
        if let Some(ref consensus) = report.consensus_verification {
            println!("--- 共识结果 ---");
            let c_icon = if consensus.is_valid { "✅" } else { "❌" };
            println!("{} 共识决策：{:?}", c_icon, consensus.decision);
            println!("  投票数：{}", consensus.vote_count);
            println!("  加权分数：{:.2}", consensus.weighted_score);
            println!("  置信度：{:.2}", consensus.confidence);
            println!();
        }

        // 警告和错误
        if !report.warnings.is_empty() {
            println!("--- 警告 ---");
            for warning in &report.warnings {
                println!("⚠️  {}", warning);
            }
            println!();
        }

        if !report.errors.is_empty() {
            println!("--- 错误 ---");
            for error in &report.errors {
                println!("❌ {}", error);
            }
            println!();
        }
    }

    fn output_batch_result(
        &self,
        reports: &[VerificationReport],
        errors: &[(PathBuf, anyhow::Error)],
    ) -> Result<()> {
        match self.format {
            OutputFormat::Json => {
                let output = BatchOutput {
                    total: reports.len() + errors.len(),
                    success: reports.len(),
                    failed: errors.len(),
                    reports: reports.to_vec(),
                    errors: errors.iter().map(|(p, e)| {
                        format!("{:?}: {}", p, e)
                    }).collect(),
                };
                let json = serde_json::to_string_pretty(&output)?;
                println!("{}", json);
            }
            OutputFormat::Text => {
                println!("\n=== 批量验证结果 ===");
                println!("总文件数：{}", reports.len() + errors.len());
                println!("成功：{}", reports.len());
                println!("失败：{}", errors.len());
                println!();

                if self.verbose {
                    println!("--- 成功验证 ---");
                    for report in reports {
                        let icon = match report.overall_result {
                            VerificationStatus::Valid => "✅",
                            VerificationStatus::PartiallyValid => "⚠️",
                            VerificationStatus::Invalid => "❌",
                            _ => "❓",
                        };
                        println!("{} {} (置信度：{:.2})", icon, report.package_id, report.confidence);
                    }

                    if !errors.is_empty() {
                        println!();
                        println!("--- 失败 ---");
                        for (path, error) in errors {
                            println!("❌ {:?}: {}", path, error);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn output_package_info(&self, package: &VerifiableInferencePackage) -> Result<()> {
        let summary = package.summary();

        match self.format {
            OutputFormat::Json => {
                let json = serde_json::to_string_pretty(&summary)?;
                println!("{}", json);
            }
            OutputFormat::Text => {
                println!("\n=== 推理包信息 ===");
                println!("包 ID: {}", summary.package_id);
                println!("请求 ID: {}", summary.request_id);
                println!("包哈希：{}", summary.package_hash);
                println!();
                println!("--- 内容 ---");
                println!("质量证明：{}", if summary.has_quality_proof { "有" } else { "无" });
                println!("共识结果：{}", if summary.has_consensus { "有" } else { "无" });
                println!("审计追踪：{}", if summary.has_audit_trail { "有" } else { "无" });
                println!("参与节点数：{}", summary.node_count);

                if self.verbose {
                    println!();
                    println!("--- 请求详情 ---");
                    println!("Prompt: {}", package.request.prompt);
                    println!("Model: {}", package.request.model_id);
                    println!();
                    println!("--- 响应详情 ---");
                    println!("Completion: {}", package.response.completion);
                    println!("Tokens: {} (prompt) + {} (completion)", 
                        package.response.prompt_tokens,
                        package.response.completion_tokens);
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
struct BatchOutput {
    total: usize,
    success: usize,
    failed: usize,
    reports: Vec<VerificationReport>,
    errors: Vec<String>,
}

fn format_timestamp(timestamp: u64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = UNIX_EPOCH + std::time::Duration::from_millis(timestamp);
    let datetime: SystemTime = duration;
    format!("{:?}", datetime)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parse_verify() {
        let cli = Cli::parse_from([
            "verify-inference",
            "verify",
            "package.json",
        ]);

        match cli.command {
            Commands::Verify { package_file } => {
                assert_eq!(package_file.to_str().unwrap(), "package.json");
            }
            _ => panic!("Wrong command parsed"),
        }
    }

    #[test]
    fn test_cli_parse_inspect() {
        let cli = Cli::parse_from([
            "verify-inference",
            "inspect",
            "package.json",
            "--format",
            "json",
        ]);

        assert_eq!(cli.format, "json");
        
        match cli.command {
            Commands::Inspect { package_file } => {
                assert_eq!(package_file.to_str().unwrap(), "package.json");
            }
            _ => panic!("Wrong command parsed"),
        }
    }
}
