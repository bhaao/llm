# 文档索引

> **项目版本**: v0.5.0  
> **最后更新**: 2026-03-26

---

## 核心文档

这是项目的主要文档集合，按使用场景组织：

| 编号 | 文档 | 说明 | 适用对象 |
|------|------|------|----------|
| **01** | [快速开始指南](01-GETTING_STARTED.md) | 环境安装、构建运行、使用示例 | 新用户 |
| **02** | [架构设计文档](02-ARCHITECTURE.md) | 三层架构、双链设计、数据流 | 架构师、开发者 |
| **03** | [开发者指南](03-DEVELOPER_GUIDE.md) | 开发环境、代码规范、贡献流程 | 贡献者 |
| **04** | [生产就绪度评估](04-PRODUCTION_READINESS.md) | 模块就绪度、性能指标、技术债务 | 技术决策者 |
| **05** | [P11 锐评与修复](05-P11_REVIEW_FIXES.md) | 业内锐评、核心问题、修复记录 | 所有读者 |
| **06** | [KV Cache 优化](06-KV_CACHE_OPTIMIZATION.md) | 优化方案、实施报告、使用示例 | 性能工程师 |
| **07** | [李群实现文档](07-LIE_GROUP_IMPLEMENTATION.md) | 信任根上移、四层架构、API 说明 | 高级开发者 |
| **08** | [变更日志](08-CHANGELOG.md) | 版本历史、核心功能、修复记录 | 所有用户 |
| **09** | [测试指南](09-TESTING_GUIDE.md) | 测试分类、运行方法、编写示例 | 测试工程师 |
| **10** | [路线图](10-ROADMAP.md) | 短期/中期/长期计划、技术债务 | 项目维护者 |

---

## 推荐阅读顺序

### 新用户
1. [快速开始指南](01-GETTING_STARTED.md) - 快速上手
2. [架构设计文档](02-ARCHITECTURE.md) - 了解架构
3. [生产就绪度评估](04-PRODUCTION_READINESS.md) - 评估适用性

### 贡献者
1. [开发者指南](03-DEVELOPER_GUIDE.md) - 开发规范
2. [测试指南](09-TESTING_GUIDE.md) - 测试方法
3. [P11 锐评与修复](05-P11_REVIEW_FIXES.md) - 了解历史问题

### 技术决策者
1. [生产就绪度评估](04-PRODUCTION_READINESS.md) - 生产适用性
2. [架构设计文档](02-ARCHITECTURE.md) - 架构设计
3. [路线图](10-ROADMAP.md) - 未来规划

### 性能工程师
1. [KV Cache 优化](06-KV_CACHE_OPTIMIZATION.md) - 优化方案
2. [李群实现文档](07-LIE_GROUP_IMPLEMENTATION.md) - 验证性能
3. [测试指南](09-TESTING_GUIDE.md) - 基准测试

---

## 内部参考文档

以下文档位于 `internal/` 目录，包含历史文档和详细实现说明：

| 文档 | 说明 |
|------|------|
| [internal/项目总结.md](internal/项目总结.md) | 完整项目总结（1.0 版本） |
| [internal/LIE_GROUP_PERFORMANCE_REPORT.md](internal/LIE_GROUP_PERFORMANCE_REPORT.md) | 李群性能基准报告 |
| [internal/LIBP2P_INTEGRATION_GUIDE.md](internal/LIBP2P_INTEGRATION_GUIDE.md) | libp2p 集成指南 |
| [internal/L3_REDIS_CACHE_GUIDE.md](internal/L3_REDIS_CACHE_GUIDE.md) | L3 Redis 缓存指南 |
| [internal/NETWORK_IMPLEMENTATION.md](internal/NETWORK_IMPLEMENTATION.md) | 网络层实现说明 |
| [internal/OLLAMA_IMPLEMENTATION.md](internal/OLLAMA_IMPLEMENTATION.md) | Ollama 集成说明 |
| [internal/ALIYUN_QWEN_PROVIDER.md](internal/ALIYUN_QWEN_PROVIDER.md) | 阿里云 Qwen 提供商 |
| [internal/ALIYUN_REMEDIATION.md](internal/ALIYUN_REMEDIATION.md) | 阿里云修复记录 |
| [internal/API_KEY_CONFIG.md](internal/API_KEY_CONFIG.md) | API Key 配置说明 |
| [internal/P11_FIX_SUMMARY.md](internal/P11_FIX_SUMMARY.md) | P11 修复摘要 |
| [internal/P11_FIXES.md](internal/P11_FIXES.md) | P11 修复详情 |
| [internal/RELEASE_v0.5.0.md](internal/RELEASE_v0.5.0.md) | v0.5.0 发布说明 |

---

## 文档结构

```
docs/
├── README.md                        # 本文档（索引）
├── 01-GETTING_STARTED.md            # 快速开始指南
├── 02-ARCHITECTURE.md               # 架构设计文档
├── 03-DEVELOPER_GUIDE.md            # 开发者指南
├── 04-PRODUCTION_READINESS.md       # 生产就绪度评估
├── 05-P11_REVIEW_FIXES.md           # P11 锐评与修复
├── 06-KV_CACHE_OPTIMIZATION.md      # KV Cache 优化
├── 07-LIE_GROUP_IMPLEMENTATION.md   # 李群实现文档
├── 08-CHANGELOG.md                  # 变更日志
├── 09-TESTING_GUIDE.md              # 测试指南
├── 10-ROADMAP.md                    # 路线图
└── internal/                        # 内部参考文档
    ├── 项目总结.md
    ├── LIE_GROUP_PERFORMANCE_REPORT.md
    ├── LIBP2P_INTEGRATION_GUIDE.md
    ├── L3_REDIS_CACHE_GUIDE.md
    ├── NETWORK_IMPLEMENTATION.md
    ├── OLLAMA_IMPLEMENTATION.md
    ├── ALIYUN_QWEN_PROVIDER.md
    ├── ALIYUN_REMEDIATION.md
    ├── API_KEY_CONFIG.md
    ├── P11_FIX_SUMMARY.md
    ├── P11_FIXES.md
    └── RELEASE_v0.5.0.md
```

---

## 外部文档

- [GitHub 仓库](https://github.com/user/block_chain_with_context) - 源代码
- [API 文档](https://docs.rs/block_chain_with_context) - Rust API 文档
- [Crates.io](https://crates.io/crates/block_chain_with_context) - Cargo 包

---

## 文档维护

### 更新频率

- **核心文档**: 每个版本更新
- **变更日志**: 每次发布更新
- **路线图**: 每季度更新

### 贡献文档

欢迎贡献文档！请参考 [开发者指南](03-DEVELOPER_GUIDE.md) 的贡献流程。

### 文档规范

- 使用 Markdown 格式
- 标题层级清晰（H1 → H2 → H3）
- 代码块标注语言
- 表格对齐整齐
- 链接使用相对路径

---

*最后更新：2026-03-26*  
*项目版本：v0.5.0*
