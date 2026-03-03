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

## 安装

```bash
cargo install --path crates/webterm
```

## 使用方法

```bash
webterm [OPTIONS] [-- <命令参数>...]
```

### 参数说明

| 参数 | 简写 | 说明 | 默认值 |
|------|------|------|--------|
| `--bind` | `-b` | Web 服务监听地址 | 自动获取可用 IP 和端口 |
| `--command` | `-c` | 要运行的命令 | Windows: `cmd.exe`<br>Linux/Mac: `/bin/bash` |
| `--server-name` | - | 在 Hub 中显示的 Server 名称 | 主机名 |
| `--hub` | - | Hub 中心服务 MQTT 地址 | - |
| `--mqtt` | - | 启用 MQTT 通知 | false |
| `--` | - | 命令参数（跟在 -- 后面） | - |

### 使用示例

#### 示例 1：启动默认终端

Windows 默认启动 cmd.exe，Linux/Mac 默认启动 bash：

```bash
webterm
```

访问 http://127.0.0.1:3000 即可在浏览器中使用终端。

#### 示例 2：指定监听地址

在局域网可访问的地址启动：

```bash
webterm -b 0.0.0.0:8080
```

局域网内其他设备可以通过 http://<你的IP>:8080 访问终端。

#### 示例 3：运行特定命令

启动 PowerShell：

```bash
webterm -c powershell.exe
```

启动 Python 交互式环境：

```bash
webterm -c python
```

#### 示例 4：带参数的命令

启动具有特定参数的 bash：

```bash
webterm -c /bin/bash -- -l
```

启动 Node.js 程序：

```bash
webterm -c node -- server.js --port 8080
```

#### 示例 5：启动 SSH 会话（在浏览器中使用 SSH）

```bash
webterm -c ssh -- user@example.com
```

## 安全提示

⚠️ **警告**：Web 终端提供完整的命令行访问权限，在公网开放时请务必：

1. 绑定到 `127.0.0.1`（仅本地访问）
2. 或配置防火墙限制访问来源
3. 不要在对公网开放的服务器上使用 `0.0.0.0` 绑定且不加认证

## Hub 模式（多 Server 管理）

通过 Hub 中心服务，可以统一管理多台 PC 上的 Web 终端，手机端通过下拉刷新即可查看所有可用的 Server。

### 架构

```
┌─────────────┐      MQTT       ┌─────────────┐      HTTP       ┌─────────────┐
│  PC Server  │ ───────────────>│  Hub Center │<───────────────>│  Android    │
│  (webterm)  │   注册/心跳      │ (MQTT+HTTP) │   获取列表      │   App       │
└─────────────┘                 └─────────────┘                └─────────────┘
```

### 快速启动

#### 1. 启动 MQTT Broker（可选）

```bash
docker run -d --name mosquitto -p 1883:1883 eclipse-mosquitto
```

#### 2. 启动 Hub 中心服务

```bash
cd hub
cargo run --release
```

#### 3. PC 端启动 webterm 并注册到 Hub

```bash
# 机器 A
webterm --hub localhost:1883 --server-name "张三办公电脑" -c powershell.exe

# 机器 B
webterm --hub localhost:1883 --server-name "李四测试机" -c cmd.exe
```

#### 4. 安装并启动 Android App

```bash
cd webterm_client
.\install.bat
```

### 同一个 Server 启动多个 WebTerm

在同一台电脑上可以启动多个 webterm 会话，每个会话会自动分配不同端口：

```bash
# 第一个会话
webterm --hub localhost:1883 --server-name "我的电脑" -c powershell.exe

# 第二个会话
webterm --hub localhost:1883 --server-name "我的电脑" -c cmd.exe

# 第三个会话
webterm --hub localhost:1883 --server-name "我的电脑" -c python
```

在手机端可以看到 "我的电脑" 下有 3 个会话可供选择。

## 配置文件

配置文件使用 TOML 格式。

### 配置文件路径（按优先级）

1. 命令行指定的路径：`--config /path/to/config.toml`
2. 当前目录：`./webterm.toml`
3. 用户配置目录：
   - Windows: `%APPDATA%/portmap/config.toml`
   - Linux/macOS: `~/.config/portmap/config.toml`

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
topic = "portmap/webterm"

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
```

### Hub 服务环境变量

Hub 服务支持以下环境变量：

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `HUB_HTTP_BIND` | HTTP API 监听地址 | `0.0.0.0:8080` |
| `HUB_MQTT_HOST` | MQTT Broker 地址 | `localhost` |
| `HUB_MQTT_PORT` | MQTT Broker 端口 | `1883` |
| `HUB_MQTT_USER` | MQTT 用户名 | - |
| `HUB_MQTT_PASS` | MQTT 密码 | - |
| `HUB_HEARTBEAT_TIMEOUT` | 心跳超时（秒） | `90` |
| `HUB_CLEANUP_INTERVAL` | 清理间隔（秒） | `30` |

## 故障排除

### Web 终端显示异常

1. 确保浏览器支持 WebSocket
2. 清除浏览器缓存后刷新
3. 检查浏览器控制台（F12）查看错误信息

### Hub 模式下 Server 列表为空

浏览器访问 `http://HubIP:8080/api/servers` 返回 `[]`：

1. 检查 PC 端是否正常启动并显示 `已向 Hub 注册 WebTerm: xxx`
2. 检查 Hub 日志是否显示 `新 Server 注册: xxx`
3. 确认 MQTT 连接正常

### 同一个 Server 启动多个 web-term 时端口冲突

```
Error: 无法绑定到地址: 10.126.126.6:30000
通常每个套接字地址(协议/网络地址/端口)只允许使用一次。
```

**解决方法**：这是预期行为，第二个 webterm 会自动尝试 30001、30002... 直到找到可用端口。如果仍报错，请检查端口范围 30000-40000 是否已满。

### Android App 无法连接 Hub

1. 确认手机和 Hub 在同一网段
2. 检查 `ServerListActivity.java` 中的 `DEFAULT_HUB_URL` 是否正确
3. 检查防火墙是否放行 8080 端口
4. 尝试在浏览器中访问 `http://HubIP:8080/api/servers` 测试

## 许可证

MIT License
