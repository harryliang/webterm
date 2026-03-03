# Webterm

Web 终端工具 - 通过浏览器访问命令行程序，支持 xterm.js 终端界面。

## 功能特性

- **🖥️ Web 终端** - 通过浏览器访问命令行程序
- **🎨 彩色终端** - 支持 16 色 ANSI 颜色代码
- **⌨️ 全键盘支持** - 支持 Tab 补全、Ctrl+C 中断等功能键
- **📐 自适应大小** - 终端窗口自动适应浏览器大小
- **🔄 自动重连** - 连接断开 5 秒后自动重连
- **💓 心跳检测** - 30 秒间隔心跳检测连接状态
- **📜 会话保持** - 支持多客户端同时连接同一会话
- **🔗 Hub 模式** - 支持多 Server 统一管理

## 项目结构

```
webterm/
├── webterm/         # Web 终端服务
├── common/          # 共享库（配置、工具函数、会话管理等）
├── hub/             # Hub 中心服务（多 Server 管理）
├── static/          # Web 终端静态文件
└── webterm_client/  # Android 客户端
```

## 安装

### 从源码编译

```bash
# 克隆仓库
git clone <repository-url>
cd webterm

# 编译所有工具
cargo build --release

# 或者分别编译
cargo build --release -p webterm
cargo build --release -p webterm-hub

# 编译后的可执行文件位于 target/release/
```

### 环境要求

- Rust 1.70+
- Windows / Linux / macOS
- Android SDK（如果需要编译 Android 客户端）
- MQTT Broker（如果需要使用 Hub 模式）

## 快速开始

### Webterm - Web 终端

```bash
# 启动默认终端（Windows: cmd.exe, Linux/Mac: /bin/bash）
webterm

# 指定监听地址
webterm -b 0.0.0.0:8080

# 运行特定命令
webterm -c powershell.exe
webterm -c python

# 带参数的命令
webterm -c /bin/bash -- -l
webterm -c node -- server.js --port 8080

# Hub 模式（多 Server 管理）
webterm --hub localhost:1883 --server-name "我的办公电脑"
```

### Hub 中心服务

```bash
# 1. 启动 MQTT Broker（可选，可使用本地或公有云服务）
docker run -d --name mosquitto -p 1883:1883 eclipse-mosquitto

# 2. 启动 Hub 中心服务
cd hub
cargo run --release

# 3. 各 PC 启动 webterm 并注册到 Hub
webterm --hub localhost:1883 --server-name "张三办公电脑"
webterm --hub localhost:1883 --server-name "李四测试机"

# 4. 安装并启动 Android App
cd webterm_client
.\install.bat
```

## 详细文档

- [Webterm 详细说明](webterm/README.md)
- [Hub 详细说明](hub/README.md)

## 配置文件

支持 TOML 格式配置文件。

### 配置文件路径（按优先级）

1. 命令行指定的路径：`--config /path/to/config.toml`
2. 当前目录：`./webterm.toml`
3. 用户配置目录：
   - Windows: `%APPDATA%/webterm/config.toml`
   - Linux/macOS: `~/.config/webterm/config.toml`

### 生成默认配置

```bash
webterm --init-config
```

### 配置示例

```toml
[mqtt]
host = "localhost"
port = 1883
username = "your_username"
password = "your_password"
topic = "webterm"

[hub]
bind = "0.0.0.0:8080"
heartbeat_timeout = 90

[webterm]
windows_cmd = "cmd.exe"
unix_cmd = "/bin/bash"

[session]
max_history_lines = 10000
timeout_secs = 3600
max_sessions = 10

[network]
preferred_ip_prefixes = ["192.168.", "10.0."]
port_start = 30000
port_end = 40000
buffer_size = 8192
```

## 技术栈

- **异步运行时**: [Tokio](https://tokio.rs/)
- **Web 框架**: [Axum](https://github.com/tokio-rs/axum)
- **命令行解析**: [Clap](https://github.com/clap-rs/clap)
- **PTY 终端**: [portable-pty](https://github.com/wez/wezterm/tree/main/pty)
- **前端终端**: [xterm.js](https://xtermjs.org/)
- **MQTT 客户端**: [rumqttc](https://github.com/bytebeamio/rumqtt)
- **并发数据结构**: [DashMap](https://github.com/xacrimon/dashmap)

## 许可证

MIT License

## 第三方依赖

本项目使用了以下开源项目：

- **[xterm.js](https://xtermjs.org/)** - MIT License
- **[xterm-addon-fit](https://github.com/xtermjs/xterm.js)** - MIT License  
- **[Eclipse Paho MQTT Java Client](https://www.eclipse.org/paho/)** - EPL 2.0 License

Android 客户端使用的 MQTT 库遵循 EPL 2.0 许可证。

## 贡献

欢迎提交 Issue 和 Pull Request！请参阅 [CONTRIBUTING.md](CONTRIBUTING.md) 了解详情。

## 更新日志

详见 [CHANGELOG.md](CHANGELOG.md)。
