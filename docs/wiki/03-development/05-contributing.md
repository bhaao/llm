# 贡献流程

> **阅读时间**: 10 分钟  
> **适用对象**: 贡献者

---

## 1. 贡献方式

### 1.1 代码贡献

- 修复 Bug
- 实现新功能
- 性能优化
- 重构代码

### 1.2 文档贡献

- 修复文档错误
- 补充缺失文档
- 改进文档结构
- 翻译文档

### 1.3 其他贡献

- 报告问题
- 提出建议
- 回答问题
- 分享经验

---

## 2. Git 工作流

### 2.1 Fork 项目

```bash
# 1. 在 GitHub 上 Fork 项目
# 点击 Fork 按钮

# 2. 克隆 Fork 的项目
git clone https://github.com/your-username/block_chain_with_context.git
cd block_chain_with_context

# 3. 添加上游仓库
git remote add upstream https://github.com/original-owner/block_chain_with_context.git
```

### 2.2 创建分支

```bash
# 同步主分支
git checkout main
git pull upstream main

# 创建功能分支
git checkout -b feature/my-feature

# 分支命名规范:
# - feature/*: 新功能
# - bugfix/*: Bug 修复
# - docs/*: 文档更新
# - refactor/*: 代码重构
# - test/*: 测试相关
```

### 2.3 提交更改

```bash
# 添加文件
git add path/to/file

# 提交（遵循提交信息规范）
git commit -m "feat: add my feature"

# 推送到远程
git push origin feature/my-feature
```

### 2.4 创建 Pull Request

1. 在 GitHub 上访问你的 Fork
2. 点击 "Compare & pull request"
3. 填写 PR 描述
4. 等待 CI 检查
5. 等待维护者审查

---

## 3. 提交信息规范

### 3.1 格式

```
<type>(<scope>): <subject>

<body>

<footer>
```

### 3.2 类型（type）

| 类型 | 说明 |
|------|------|
| `feat` | 新功能 |
| `fix` | Bug 修复 |
| `docs` | 文档更新 |
| `style` | 代码格式（不影响代码运行） |
| `refactor` | 重构（既不是新功能也不是修复） |
| `test` | 测试相关 |
| `chore` | 构建过程或辅助工具变动 |

### 3.3 示例

```
feat(memory_layer): add L3 Redis cache support

- Add Redis connection pool
- Implement async read/write
- Add configuration options

Closes #123

Signed-off-by: Your Name <your.email@example.com>
```

```
fix(blockchain): fix consensus race condition

The consensus algorithm had a race condition that could cause
deadlock when multiple nodes submit simultaneously.

This fix adds proper locking mechanism to prevent the issue.

Fixes #456

Signed-off-by: Your Name <your.email@example.com>
```

---

## 4. PR 审查流程

### 4.1 CI 检查

提交 PR 后，CI 会自动运行：

- ✅ 编译检查
- ✅ 单元测试
- ✅ 代码格式
- ✅ Clippy Lint

### 4.2 代码审查

维护者会审查：

- 代码质量
- 测试覆盖
- 文档完整性
- 性能影响

### 4.3 合并

审查通过后：

- 维护者会合并 PR
- 删除功能分支
- 更新变更日志

---

## 5. 开发环境设置

### 5.1 安装工具

```bash
# Rust 工具链
rustup install stable
rustup component add rustfmt clippy

# 其他工具
cargo install cargo-edit
cargo install cargo-watch
cargo install cargo-audit
```

### 5.2 配置 IDE

参考 [开发环境](01-setup.md) 配置 IDE。

### 5.3 运行测试

```bash
# 运行所有测试
cargo test

# 运行 Clippy
cargo clippy -- -D warnings

# 格式化代码
cargo fmt
```

---

## 6. 贡献指南

### 6.1 代码规范

- 遵循 [编码规范](02-coding-style.md)
- 添加必要的文档注释
- 编写单元测试
- 通过 Clippy 检查

### 6.2 测试要求

- 新功能必须包含测试
- Bug 修复添加回归测试
- 确保所有测试通过
- 覆盖率不降低

### 6.3 文档要求

- 公共 API 添加文档注释
- 更新相关文档
- 添加使用示例
- 更新变更日志

### 6.4 性能要求

- 性能不降低
- 添加基准测试（如适用）
- 说明性能影响

---

## 7. 常见问题

### 7.1 如何开始？

**答**: 从简单的 Bug 修复或文档更新开始，熟悉流程后再贡献更复杂的功能。

### 7.2 如何找到可以贡献的内容？

**答**: 查看 GitHub Issues，寻找标记为 `good first issue` 或 `help wanted` 的问题。

### 7.3 PR 多久能被合并？

**答**: 取决于 PR 的复杂性和维护者的时间。通常 1-2 周内会有反馈。

### 7.4 如何联系维护者？

**答**: 在 PR 中留言，或通过邮件联系。

---

## 8. 贡献者权益

- 在 README 中列出贡献者名单
- 参与项目决策讨论
- 获得社区认可

---

## 9. 相关文档

- [开发环境](01-setup.md) - IDE、工具链配置
- [编码规范](02-coding-style.md) - Rust 代码规范
- [调试技巧](03-debugging.md) - 调试工具、常见问题
- [测试指南](04-testing.md) - 单元测试、并发测试

---

*最后更新：2026-03-26*  
*项目版本：v0.5.0*
