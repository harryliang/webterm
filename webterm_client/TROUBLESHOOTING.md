# Android 端故障排查指南

## 问题：手机没有自动打开终端界面

### 排查步骤

#### 步骤 1: 检查 Android 日志

确保手机通过 USB 连接，然后运行：

```bash
# 清空旧日志
F:\android-sdk\platform-tools\adb logcat -c

# 启动应用
F:\android-sdk\platform-tools\adb shell am start -n com.example.hello/.MainActivity

# 查看日志（在另一个窗口运行）
F:\android-sdk\platform-tools\adb logcat -s MainActivity:MqttService | findstr "==="
```

**预期看到的日志：**
```
I/MainActivity: === onCreate ===
I/MainActivity: 启动 MQTT 服务...
I/MainActivity: bindService 结果: true
I/MqttService: === MQTT 服务创建 ===
I/MqttService: === 开始连接 MQTT ===
I/MqttService: Broker: tcp://mqtt.iyanjiu.com:1883
I/MqttService: === MQTT 连接成功 ===
I/MqttService: === 主题订阅成功 ===
I/MainActivity: === onServiceConnected called ===
```

**如果看到：**
- `!!! MQTT 连接失败` → 检查网络连接，确认手机能访问 mqtt.iyanjiu.com:1883
- `messageCallback 为 null` → 回调设置问题（已修复）

---

#### 步骤 2: 检查 PC 端 MQTT 发送

在 PC 端运行测试：

```bash
# 窗口 1: 启动订阅
python test_mqtt.py

# 窗口 2: 启动 web-term
.\target\release\portmap.exe web-term -c cmd.exe
```

确认 Python 订阅端能收到消息。

---

#### 步骤 3: 检查网络连通性

在手机上：
1. 打开手机浏览器
2. 访问 `http://mqtt.iyanjiu.com:1883`
3. 应该显示连接错误（证明端口通）

在 PC 端检查手机 IP：
```bash
# 查看 PC 和手机是否在同一网络
ipconfig  # PC IP
# 对比手机 WiFi 的 IP 地址
```

---

#### 步骤 4: 常见问题

##### 问题 A: MQTT 连接失败

**现象**: 日志显示 `!!! MQTT 连接失败`

**原因**:
1. 手机无法访问 mqtt.iyanjiu.com:1883
2. 防火墙阻挡

**解决**:
```bash
# 测试网络连通性
F:\android-sdk\platform-tools\adb shell ping mqtt.iyanjiu.com

# 或者用手机浏览器访问 http://mqtt.iyanjiu.com:1883
# 应该显示 "连接被拒绝" 或类似错误（表示端口通）
```

##### 问题 B: 收到消息但没打开 URL

**现象**: 日志显示 `收到 MQTT 消息` 但没有 `打开 URL`

**原因**: URL 解析失败或 WebView 问题

**解决**: 检查日志中是否有 `URL: xxx` 和 `打开 URL: xxx`

##### 问题 C: WebView 显示空白

**现象**: 显示了 URL 但页面空白

**原因**: 
1. 手机和 PC 不在同一网络
2. Windows 防火墙阻挡
3. URL 使用了 localhost 而不是实际 IP

**解决**:
1. 确认 PC 和手机在同一 WiFi
2. 关闭 Windows 防火墙测试
3. 确认 URL 是 `http://10.126.x.x:30000` 而不是 `http://127.0.0.1:30000`

---

### 快速诊断脚本

创建 `diagnose.bat`：

```batch
@echo off
echo ===== Android 诊断工具 =====
echo.

echo [1] 检查设备连接...
F:\android-sdk\platform-tools\adb devices
echo.

echo [2] 清空日志...
F:\android-sdk\platform-tools\adb logcat -c
echo.

echo [3] 启动应用...
F:\android-sdk\platform-tools\adb shell am start -n com.example.hello/.MainActivity
echo.

echo [4] 等待 3 秒...
timeout /t 3 /nobreak >nul
echo.

echo [5] 收集日志（按 Ctrl+C 停止）...
F:\android-sdk\platform-tools\adb logcat -s MainActivity:MqttService *:S
echo.
```

---

### 手动测试流程

如果自动流程不工作，手动测试每一步：

1. **在手机上打开应用**，点击"显示日志"查看日志
2. **在 PC 端运行** `python test_mqtt.py` 启动订阅
3. **在 PC 端运行** `python test_mqtt_send.py` 发送测试消息
4. **确认 Python 订阅收到消息** → MQTT Broker 正常
5. **查看手机日志** 是否显示 `收到 MQTT 消息`

---

### 联系方式

如果以上步骤都无法解决问题，请提供：
1. `adb logcat -s MainActivity:MqttService` 的完整输出
2. PC 端 `python test_mqtt.py` 的输出
3. 手机的 IP 地址和 PC 的 IP 地址
