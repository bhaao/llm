# Wiki 索引

> **项目版本**: v0.5.0  
> **最后更新**: 2026-03-26

这是项目的 Wiki 风格文档集，适合团队内部协作维护和查阅。

---

## 快速导航

### 按角色

| 角色 | 推荐文档 |
|------|----------|
| **新用户** | [什么是本项目](01-intro/01-what-is.md) → [环境安装](01-intro/02-installation.md) → [快速开始](01-intro/03-quickstart.md) |
| **开发者** | [开发环境](03-development/01-setup.md) → [编码规范](03-development/02-coding-style.md) → [测试指南](03-development/04-testing.md) |
| **架构师** | [整体架构](02-architecture/01-overview.md) → [模块详解](02-architecture/02-modules.md) → [李群验证](02-architecture/04-lie-group.md) |
| **运维工程师** | [部署指南](04-operations/01-deployment.md) → [监控告警](04-operations/02-monitoring.md) → [故障排查](04-operations/03-troubleshooting.md) |
| **技术决策者** | [什么是本项目](01-intro/01-what-is.md) → [生产就绪度](../04-PRODUCTION_READINESS.md) → [路线图](../10-ROADMAP.md) |

---

## 文档分类

### 📖 入门篇 (01-intro)

适合新用户快速上手：

| 文档 | 说明 | 阅读时间 |
|------|------|----------|
| [什么是本项目](01-intro/01-what-is.md) | 项目定位、核心概念 | 5 分钟 |
| [环境安装](01-intro/02-installation.md) | Rust、protoc 安装 | 10 分钟 |
| [快速开始](01-intro/03-quickstart.md) | 构建、测试、运行示例 | 15 分钟 |
| [配置指南](01-intro/04-configuration.md) | 配置文件、环境变量 | 10 分钟 |

### 🏗️ 架构篇 (02-architecture)

适合架构师和开发者深入了解系统设计：

| 文档 | 说明 | 阅读时间 |
|------|------|----------|
| [整体架构](02-architecture/01-overview.md) | 三层架构、双链设计 | 20 分钟 |
| [模块详解](02-architecture/02-modules.md) | 5 个核心模块详解 | 30 分钟 |
| [数据流](02-architecture/03-dataflow.md) | 推理流程、共识流程 | 15 分钟 |
| [李群验证](02-architecture/04-lie-group.md) | 信任根上移、四层架构 | 25 分钟 |

### 🛠️ 开发篇 (03-development)

适合贡献者和开发者：

| 文档 | 说明 | 阅读时间 |
|------|------|----------|
| [开发环境](03-development/01-setup.md) | IDE、工具链配置 | 10 分钟 |
| [编码规范](03-development/02-coding-style.md) | Rust 代码规范 | 15 分钟 |
| [调试技巧](03-development/03-debugging.md) | 调试工具、常见问题 | 20 分钟 |
| [测试指南](03-development/04-testing.md) | 单元测试、并发测试 | 15 分钟 |
| [贡献流程](03-development/05-contributing.md) | Git 工作流、PR 流程 | 10 分钟 |

### 🔧 运维篇 (04-operations)

适合运维工程师和技术决策者：

| 文档 | 说明 | 阅读时间 |
|------|------|----------|
| [部署指南](04-operations/01-deployment.md) | 单节点、多节点部署 | 20 分钟 |
| [监控告警](04-operations/02-monitoring.md) | Prometheus、Grafana | 15 分钟 |
| [故障排查](04-operations/03-troubleshooting.md) | 常见问题、排查流程 | 20 分钟 |
| [性能调优](../06-KV_CACHE_OPTIMIZATION.md) | 性能指标、优化建议 | 25 分钟 |

### 📚 参考篇 (05-reference)

速查手册和参考资料：

| 文档 | 说明 |
|------|------|
| [API 参考](05-reference/01-api.md) | HTTP API、Rust API、gRPC API |
| [配置项参考](05-reference/02-config-options.md) | 所有配置项说明 |
| [常见问题](05-reference/03-faq.md) | FAQ 问答 |
| [术语表](05-reference/04-glossary.md) | 专业术语解释 |
| [变更日志](05-reference/05-changelog.md) | 版本历史 |

---

## 文档统计

| 分类 | 文档数 | 总阅读时间 |
|------|--------|------------|
| 入门篇 | 4 | 40 分钟 |
| 架构篇 | 4 | 90 分钟 |
| 开发篇 | 5 | 70 分钟 |
| 运维篇 | 3 | 55 分钟 |
| 参考篇 | 5 | - |
| **总计** | **21** | **约 4.5 小时** |

---

## 相关资源

### 内部资源

- [核心文档](../README.md) - 主 README
- [内部参考](../internal/项目总结.md) - 历史文档和详细实现说明

### 外部资源

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

欢迎贡献文档！请参考 [贡献流程](03-development/05-contributing.md)。

### 文档规范

- 使用 Markdown 格式
- 标题层级清晰（H1 → H2 → H3）
- 代码块标注语言
- 表格对齐整齐
- 链接使用相对路径

---

*最后更新：2026-03-27*
*项目版本：v0.5.0*
