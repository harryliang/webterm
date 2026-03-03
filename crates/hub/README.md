# Webterm Hub

Webterm 中心管理服务，用于管理多个 Server 的 WebTerm 会话。

## 功能

- **MQTT 接收注册**：接收 PC 端 web-term 的注册和心跳
- **HTTP API**：提供 Server 列表查询接口供手机端使用
- **自动清理**：90秒无心跳自动标记离线

## 运行依赖

需要 MQTT Broker，推荐使用 [mosquitto](https://mosquitto.org/)：

```bash
# Windows (使用 chocolatey)
choco install mosquitto
mosquitto

# Linux
apt install mosquitto
mosquitto -d

# Docker
docker run -it -p 1883:1883 -p 9001:9001 eclipse-mosquitto
```

## 构建运行

```bash
# 构建
cargo build --release

# 运行
./target/release/webterm-hub

# HTTP API 监听 0.0.0.0:8080
# MQTT 连接 127.0.0.1:1883
```

## HTTP API

### 获取所有 Server 列表
```bash
GET /api/servers
```

Response:
```json
[
  {
    "id": "pc-zhangsan-001",
    "name": "张三办公电脑",
    "user": "zhangsan",
    "hostname": "DESKTOP-ABC123",
    "webterms": [
      {
        "id": "wt-123456",
        "url": "http://10.126.1.100:30000",
        "command": "kimi --yolo",
        "created_at": "2026-02-27T10:30:00Z"
      }
    ]
  }
]
```

### 获取单个 Server 详情
```bash
GET /api/servers/{id}
```

### 健康检查
```bash
GET /api/health
```

## MQTT 主题

### 注册（PC → Hub）
```
topic: webterm/hub/register
payload: {
  "type": "register",
  "server_id": "pc-zhangsan-001",
  "server_name": "张三办公电脑",
  "user": "zhangsan",
  "hostname": "DESKTOP-ABC123",
  "webterm": {
    "id": "wt-123456",
    "url": "http://10.126.1.100:30000",
    "command": "kimi --yolo"
  }
}
```

### 心跳（PC → Hub）
```
topic: webterm/hub/heartbeat
payload: {
  "type": "heartbeat",
  "server_id": "pc-zhangsan-001",
  "active_webterms": ["wt-123456"]
}
interval: 30秒
```

### 注销（PC → Hub）
```
topic: webterm/hub/unregister
payload: {
  "type": "unregister",
  "server_id": "pc-zhangsan-001",
  "webterm_id": "wt-123456"
}
```

## 配置说明

默认配置：
- MQTT Broker: `127.0.0.1:1883`
- HTTP 端口: `8080`
- 心跳超时: `90秒`
- 清理间隔: `30秒`

如需修改，请编辑 `src/main.rs` 中的相应常量。
