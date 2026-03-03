# 更新日志

所有重要的更改都将记录在此文件中。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，
并且本项目遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [未发布]

### 新增
- 静态文件嵌入到可执行文件，支持单文件分发
- Hub 重连机制，webterm 先启动时自动重连

### 优化
- 优化 release 构建体积（从 6.5MB 降至 2.95MB）

## [0.2.0] - 2026-03-03

### 新增
- 多会话支持，可同时管理多个终端会话
- Hub 模式心跳检测和自动清理
- Android 客户端支持
- 配置文件支持（TOML 格式）
- MQTT 通知功能

### 改进
- WebSocket 自动重连
- 终端自适应大小
- 彩色终端支持（16 色 ANSI）

## [0.1.0] - 2026-02-27

### 新增
- 基础 Web 终端功能
- Hub 中心服务
- 多 Server 管理
- 会话保持功能
- 基本终端操作（Tab 补全、Ctrl+C 等）

[未发布]: https://github.com/harryliang/webterm/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/harryliang/webterm/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/harryliang/webterm/releases/tag/v0.1.0
