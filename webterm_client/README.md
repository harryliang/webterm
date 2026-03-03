# Portmap Android 客户端

Portmap Android 客户端用于接收 PC 端 `portmap web-term` 启动通知，并自动打开 WebView 访问 Web 终端。

## 功能特性

- 📡 **MQTT 自动连接** - 启动时自动连接 `mqtt.iyanjiu.com` 并订阅 `portmap/webterm` 主题
- 🔗 **自动打开 URL** - 收到 MQTT 消息后自动提取 URL 并在 WebView 中打开
- 📱 **全屏 WebView** - 基于 xterm.js 的终端界面，支持缩放和返回
- 📊 **状态显示** - 显示 MQTT 连接状态和日志信息
- 🔒 **主题编码** - 与 PC 端一致的 MD5 主题编码

## 项目结构

```
webterm_client/
├── app/
│   ├── libs/
│   │   └── org.eclipse.paho.client.mqttv3-1.2.5.jar  # MQTT 客户端库
│   └── src/main/
│       ├── AndroidManifest.xml                        # 应用配置
│       ├── java/com/example/hello/
│       │   ├── MainActivity.java                      # 主界面
│       │   └── MqttService.java                       # MQTT 后台服务
│       └── res/                                       # 资源文件
├── build.bat                                          # 构建脚本
└── README.md                                          # 本文件
```

## 构建要求

- Android SDK (API 21+, 推荐 API 35)
- Build Tools 35.0.0+
- Java 17+

## 配置 Android SDK 路径

编辑 `build.bat`，修改 `ANDROID_SDK` 路径：

```batch
set ANDROID_SDK=F:\android-sdk
```

## 构建步骤

### 1. 自动构建（推荐）

双击运行 `build.bat`：

```batch
build.bat
```

脚本会自动：
1. 编译资源
2. 编译 Java 代码
3. 转换为 DEX 格式
4. 打包 APK
5. 签名 APK
6. 安装到设备
7. 启动应用

### 2. 手动构建

```batch
cd webterm_client

:: 设置环境变量
set ANDROID_SDK=F:\android-sdk
set BUILD_TOOLS=%ANDROID_SDK%\build-tools\35.0.0
set PLATFORM=%ANDROID_SDK%\platforms\android-35

:: 创建输出目录
mkdir app\build\classes

:: 编译资源
%BUILD_TOOLS%\aapt2 compile -o app\build\res.zip --dir app\src\main\res

:: 链接资源
%BUILD_TOOLS%\aapt2 link -o app\build\linked.apk -I %PLATFORM%\android.jar ^
    --java app\build --manifest app\src\main\AndroidManifest.xml app\build\res.zip

:: 编译 Java
javac -encoding UTF-8 ^
    -cp "%PLATFORM%\android.jar;app\libs\*" ^
    -d app\build\classes ^
    app\src\main\java\com\example\hello\*.java ^
    app\build\com\example\hello\R.java

:: 转换为 DEX
%BUILD_TOOLS%\d8 --output app\build ^
    app\build\classes\**\*.class ^
    app\libs\*.jar

:: 打包
 copy app\build\linked.apk app\build\app.zip
 powershell Compress-Archive -Path app\build\classes.dex -Update -DestinationPath app\build\app.zip
 move app\build\app.zip app\build\app.apk

:: 签名
keytool -genkey -v -keystore app\build\debug.keystore -storepass android ^
    -alias androiddebugkey -keypass android -keyalg RSA -validity 10000 ^
    -dname "CN=Android Debug,O=Android,C=US"

%BUILD_TOOLS%\apksigner sign --ks app\build\debug.keystore --ks-pass pass:android ^
    --key-pass pass:android --out app\build\hello.apk app\build\app.apk

:: 安装
%ADB%\platform-tools\adb install -r app\build\hello.apk

:: 启动
%ADB%\platform-tools\adb shell am start -n com.example.hello/.MainActivity
```

## 使用方法

### 1. 启动 Android App

应用启动后会显示等待界面：

```
🖥️ Portmap WebTerm
等待 PC 端启动...
请确保 PC 端运行: portmap web-term
```

### 2. 启动 PC 端 portmap

在 PC 上运行：

```bash
portmap web-term -c cmd.exe
```

或：

```bash
portmap web-term -c kimi -- --yolo
```

### 3. 自动连接

PC 端启动后，Android App 会：
1. 收到 MQTT 消息
2. 自动提取 URL
3. 在 WebView 中打开终端

## MQTT 配置

MQTT 配置与 PC 端保持一致：

| 配置项 | 值 |
|--------|-----|
| Broker | `mqtt.iyanjiu.com` |
| Port | `1883` |
| Username | `idiaoyan` |
| Password | `Idy@3984#24039` |
| Topic | `portmap/webterm` |
| 编码方式 | MD5(topic + secret_key) |

## 故障排查

### 问题 1: 编译失败 "找不到符号"

**原因**: MQTT 库未正确引用

**解决**: 确保 `app/libs/` 目录包含 `org.eclipse.paho.client.mqttv3-1.2.5.jar`

### 问题 2: 应用启动崩溃

**原因**: 权限未声明或 MQTT 库冲突

**解决**: 检查 `AndroidManifest.xml` 是否包含所有权限：
- `INTERNET`
- `ACCESS_NETWORK_STATE`
- `FOREGROUND_SERVICE`
- `WAKE_LOCK`

### 问题 3: 无法接收 MQTT 消息

**排查步骤**:
1. 点击"显示日志"按钮查看日志
2. 确认 PC 端已成功发送 MQTT 消息
3. 使用 Python 测试脚本验证：
   ```bash
   python test_mqtt.py
   ```
4. 检查网络连接（手机需能访问 `mqtt.iyanjiu.com:1883`）

### 问题 4: WebView 显示空白

**原因**: 未开启明文流量（HTTP）支持

**解决**: `AndroidManifest.xml` 中已设置 `android:usesCleartextTraffic="true"`

## 技术实现

### MQTT 主题编码

与 PC 端保持一致：

```java
String topicEncode(String topic) {
    String s = topic + "3444462b-0f6f-4523-b382-92a1288345ef";
    MessageDigest md = MessageDigest.getInstance("MD5");
    byte[] digest = md.digest(s.getBytes());
    // 转换为 32 位小写十六进制字符串
}
```

### 消息格式

```json
{
  "event": "webterm_started",
  "ip": "10.126.126.6",
  "port": 30000,
  "url": "http://10.126.126.6:30000",
  "command": "cmd.exe",
  "timestamp": "2024-01-15T10:30:00+08:00"
}
```

## 依赖库

- [Eclipse Paho Java Client](https://www.eclipse.org/paho/) - MQTT 客户端库
  - 版本: 1.2.5
  - 下载: https://repo1.maven.org/maven2/org/eclipse/paho/org.eclipse.paho.client.mqttv3/1.2.5/

## 许可证

MIT License
