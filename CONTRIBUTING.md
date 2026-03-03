# 贡献指南

感谢您对 Webterm 项目的关注！我们欢迎各种形式的贡献。

## 如何贡献

### 报告问题

如果您发现了 bug 或有功能建议，请通过 [GitHub Issues](https://github.com/harryliang/webterm/issues) 提交：

1. 搜索现有 issues，避免重复提交
2. 使用清晰的标题描述问题
3. 提供详细的环境信息（操作系统、Rust 版本等）
4. 如果是 bug，请提供复现步骤

### 提交代码

1. Fork 本仓库
2. 创建您的特性分支 (`git checkout -b feature/amazing-feature`)
3. 提交您的更改 (`git commit -m 'Add some amazing feature'`)
4. 推送到分支 (`git push origin feature/amazing-feature`)
5. 创建 Pull Request

### 代码规范

- 遵循 Rust 官方代码风格 (`cargo fmt`)
- 确保代码通过 `cargo clippy` 检查
- 添加适当的文档注释
- 如有必要，添加单元测试

### 提交信息规范

提交信息应清晰描述更改内容：

```
类型: 简短描述

详细说明（可选）
```

类型包括：
- `feat`: 新功能
- `fix`: 修复 bug
- `docs`: 文档更新
- `style`: 代码格式调整
- `refactor`: 重构
- `test`: 测试相关
- `chore`: 构建/工具相关

## 开发环境

### 要求

- Rust 1.70+
- MQTT Broker（测试 Hub 功能时需要）

### 构建

```bash
# 构建所有组件
cargo build --release

# 运行测试
cargo test

# 代码格式化
cargo fmt

# 代码检查
cargo clippy
```

## 行为准则

- 尊重所有参与者
- 接受建设性的批评
- 以项目最佳利益为出发点

## 许可证

通过贡献代码，您同意您的贡献将在 MIT 许可证下发布。
